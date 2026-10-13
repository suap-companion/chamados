//! Tickets archived only on this machine (SUAP has no such status: it is a private marker, like the
//! local titles).
//!
//! One JSON document per profile, one entry per ticket. Every entry carries the Unix time (ms) of its
//! last change and unarchiving keeps the entry (`archived: false`, a tombstone), so two copies of the
//! file can later be merged entry by entry, the newest change winning.

use std::{
    collections::BTreeMap,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{check_ticket_id, TicketError};

const FORMAT_VERSION: u32 = 1;

/// Whether one ticket is archived, and when that last changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub archived: bool,
    /// Unix time in milliseconds of the last change.
    pub updated_at: u64,
}

impl ArchiveEntry {
    /// Whether this entry should replace `other` when merging two copies: the later change wins and,
    /// at the same time, archived wins over unarchived (a total order, so the merge is symmetric).
    pub fn newer_than(&self, other: &Self) -> bool {
        (self.updated_at, self.archived) > (other.updated_at, other.archived)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchiveFile {
    version: u32,
    tickets: BTreeMap<String, ArchiveEntry>,
}

/// The archived tickets of one profile, backed by a JSON file.
#[derive(Debug)]
pub struct ArchiveStore {
    path: PathBuf,
    entries: BTreeMap<String, ArchiveEntry>,
}

fn file_error(path: &Path, error: impl Display) -> TicketError {
    TicketError::Source(format!("archive file {}: {error}", path.display()))
}

impl ArchiveStore {
    /// Loads the store at `path`; a missing file is an empty store, an unreadable one is an error
    /// (the marks are user data and are never discarded silently).
    pub fn open(path: PathBuf) -> Result<Self, TicketError> {
        let mut entries = BTreeMap::new();
        if path.exists() {
            let content = fs::read_to_string(&path).map_err(|error| file_error(&path, error))?;
            let file: ArchiveFile =
                serde_json::from_str(&content).map_err(|error| file_error(&path, error))?;
            if file.version != FORMAT_VERSION {
                let message = format!("unsupported format version {}", file.version);
                return Err(file_error(&path, message));
            }
            entries = file.tickets;
        }
        Ok(Self { path, entries })
    }

    /// Every entry, including the unarchived ones (tombstones).
    pub fn entries(&self) -> &BTreeMap<String, ArchiveEntry> {
        &self.entries
    }

    /// Takes `entry` (from another copy) if it is newer than what is stored; returns whether
    /// anything changed. The id is validated, since it may come from the cloud.
    pub fn merge_entry(&mut self, id: &str, entry: &ArchiveEntry) -> Result<bool, TicketError> {
        check_ticket_id(id)?;
        let newer = self
            .entries
            .get(id)
            .is_none_or(|current| entry.newer_than(current));
        if newer {
            self.entries.insert(id.to_owned(), entry.clone());
        }
        Ok(newer)
    }

    /// Whether ticket `id` is archived.
    pub fn is_archived(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.archived)
    }

    /// The ids of the archived tickets.
    pub fn archived_ids(&self) -> Vec<String> {
        let archived = self.entries.iter().filter(|(_, entry)| entry.archived);
        archived.map(|(id, _)| id.clone()).collect()
    }

    /// Archives (or unarchives) ticket `id` with `now` as the change time; returns whether the state
    /// changed (asking for what is already so changes nothing).
    pub fn set(&mut self, id: &str, archived: bool, now: u64) -> Result<bool, TicketError> {
        check_ticket_id(id)?;
        if self.is_archived(id) == archived {
            return Ok(false);
        }
        let entry = ArchiveEntry {
            archived,
            updated_at: now,
        };
        self.entries.insert(id.to_owned(), entry);
        Ok(true)
    }

    /// Writes the store atomically (temporary file, then rename), creating the directory if needed.
    pub fn save(&self) -> Result<(), TicketError> {
        let file = ArchiveFile {
            version: FORMAT_VERSION,
            tickets: self.entries.clone(),
        };
        let content = serde_json::to_string_pretty(&file).expect("the marks are serializable");
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|error| file_error(&self.path, error))?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, content).map_err(|error| file_error(&self.path, error))?;
        fs::rename(&temporary, &self.path).map_err(|error| file_error(&self.path, error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn archives_unarchives_and_persists() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("dados").join("archive.json");
        let mut store = ArchiveStore::open(path.clone()).unwrap();
        assert!(!store.is_archived("1"));
        assert!(!store.set("1", false, 5).unwrap());

        assert!(store.set("1", true, 10).unwrap());
        assert!(store.set("2", true, 11).unwrap());
        assert!(!store.set("1", true, 12).unwrap());
        assert!(store.set("2", false, 13).unwrap());
        store.save().unwrap();

        let reloaded = ArchiveStore::open(path.clone()).unwrap();
        assert!(reloaded.is_archived("1") && !reloaded.is_archived("2"));
        assert_eq!(reloaded.archived_ids(), ["1"]);
        assert!(reloaded.entries().contains_key("2"));
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"updated_at\": 13") && saved.contains("\"version\": 1"));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn merges_entries_from_another_copy_by_age() {
        let directory = tempdir().unwrap();
        let mut store = ArchiveStore::open(directory.path().join("archive.json")).unwrap();
        let entry = |archived: bool, updated_at: u64| ArchiveEntry {
            archived,
            updated_at,
        };
        assert!(store.merge_entry("1", &entry(true, 5)).unwrap());
        assert!(!store.merge_entry("1", &entry(true, 5)).unwrap());
        assert!(!store.merge_entry("1", &entry(false, 4)).unwrap());
        assert!(store.merge_entry("1", &entry(false, 6)).unwrap());
        assert!(!store.is_archived("1"));
        // Ties: archived beats unarchived.
        assert!(entry(true, 6).newer_than(&entry(false, 6)));
        assert!(!entry(false, 6).newer_than(&entry(true, 6)));
        assert!(store.merge_entry("1", &entry(true, 6)).unwrap());
        assert!(store.merge_entry("../1", &entry(true, 1)).is_err());
    }

    #[test]
    fn rejects_bad_ids_and_files() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("archive.json");
        let mut store = ArchiveStore::open(path.clone()).unwrap();
        assert!(store.set("x", true, 1).is_err());

        fs::write(&path, "não é json").unwrap();
        let corrupt = ArchiveStore::open(path.clone()).unwrap_err().to_string();
        assert!(corrupt.contains("archive file") && corrupt.contains("archive.json"));

        fs::write(&path, r#"{"version": 2, "tickets": {}}"#).unwrap();
        let version = ArchiveStore::open(path).unwrap_err().to_string();
        assert!(version.contains("unsupported format version 2"));

        let unreadable = ArchiveStore::open(directory.path().to_path_buf()).unwrap_err();
        assert!(unreadable.to_string().contains("archive file"));
    }

    #[test]
    fn save_reports_unwritable_locations() {
        let directory = tempdir().unwrap();
        let blocker = directory.path().join("arquivo");
        fs::write(&blocker, "x").unwrap();
        let store = ArchiveStore::open(blocker.join("dentro").join("archive.json")).unwrap();
        assert!(store
            .save()
            .unwrap_err()
            .to_string()
            .contains("archive file"));

        // The temporary file's name is taken by a directory, so writing it fails.
        let busy = directory.path().join("ocupado.json");
        fs::create_dir(busy.with_extension("json.tmp")).unwrap();
        assert!(ArchiveStore::open(busy).unwrap().save().is_err());

        // The destination is a directory, so the final rename fails.
        let target = directory.path().join("destino");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("dentro"), "x").unwrap();
        let mut store = ArchiveStore::open(directory.path().join("archive.json")).unwrap();
        store.path = target;
        assert!(store.save().is_err());
    }
}
