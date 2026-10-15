//! Tickets snoozed only on this machine ("Adiar", as in Spark): hidden from the lists until a chosen
//! moment, when they come back on top as new. SUAP has no such state: it is a private mark, like the
//! local titles and the archive.
//!
//! One JSON document per profile, one entry per ticket. An entry holds the moment (Unix ms) the ticket
//! comes back and the time of its last change. A snooze that came due stays in the file until somebody
//! sees the ticket (then it is cleared), so "came back" can be told from "was never snoozed". Clearing
//! keeps the entry (`until: null`, a tombstone), so two copies of the file can be merged entry by
//! entry, the newest change winning.

use std::{
    collections::BTreeMap,
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{check_ticket_id, TicketError};

const FORMAT_VERSION: u32 = 1;

/// When one ticket comes back (`None` when the snooze was cleared) and when that last changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozeEntry {
    /// Unix time in milliseconds when the ticket comes back.
    pub until: Option<u64>,
    /// Unix time in milliseconds of the last change.
    pub updated_at: u64,
}

impl SnoozeEntry {
    /// Whether this entry should replace `other` when merging two copies: the later change wins and
    /// then the later return time (a total order, so the merge is symmetric).
    pub fn newer_than(&self, other: &Self) -> bool {
        (self.updated_at, self.until) > (other.updated_at, other.until)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SnoozeFile {
    version: u32,
    tickets: BTreeMap<String, SnoozeEntry>,
}

/// The snoozed tickets of one profile, backed by a JSON file.
#[derive(Debug)]
pub struct SnoozeStore {
    path: PathBuf,
    entries: BTreeMap<String, SnoozeEntry>,
}

fn file_error(path: &Path, error: impl Display) -> TicketError {
    TicketError::Source(format!("snooze file {}: {error}", path.display()))
}

impl SnoozeStore {
    /// Loads the store at `path`; a missing file is an empty store, an unreadable one is an error
    /// (the marks are user data and are never discarded silently).
    pub fn open(path: PathBuf) -> Result<Self, TicketError> {
        let mut entries = BTreeMap::new();
        if path.exists() {
            let content = fs::read_to_string(&path).map_err(|error| file_error(&path, error))?;
            let file: SnoozeFile =
                serde_json::from_str(&content).map_err(|error| file_error(&path, error))?;
            if file.version != FORMAT_VERSION {
                let message = format!("unsupported format version {}", file.version);
                return Err(file_error(&path, message));
            }
            entries = file.tickets;
        }
        Ok(Self { path, entries })
    }

    /// Every entry, including the cleared ones (tombstones).
    pub fn entries(&self) -> &BTreeMap<String, SnoozeEntry> {
        &self.entries
    }

    /// Takes `entry` (from another copy) if it is newer than what is stored; returns whether
    /// anything changed. The id is validated, since it may come from the cloud.
    pub fn merge_entry(&mut self, id: &str, entry: &SnoozeEntry) -> Result<bool, TicketError> {
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

    /// When ticket `id` comes back, if it is snoozed (still waiting or already due).
    pub fn until(&self, id: &str) -> Option<u64> {
        self.entries.get(id)?.until
    }

    /// Whether ticket `id` is still hidden at `now`.
    pub fn is_pending(&self, id: &str, now: u64) -> bool {
        self.until(id).is_some_and(|until| until > now)
    }

    /// The tickets still waiting at `now`, with the moment each comes back, soonest first.
    pub fn pending(&self, now: u64) -> Vec<(String, u64)> {
        self.sorted(|until| until > now)
    }

    /// The tickets whose time has come at `now` and that nobody has seen since, oldest first.
    pub fn due(&self, now: u64) -> Vec<(String, u64)> {
        self.sorted(|until| until <= now)
    }

    fn sorted(&self, keep: impl Fn(u64) -> bool) -> Vec<(String, u64)> {
        let entries = self.entries.iter();
        let mut found: Vec<(String, u64)> = entries
            .filter_map(|(id, entry)| Some((id.clone(), entry.until.filter(|&until| keep(until))?)))
            .collect();
        found.sort_by_key(|(id, until)| (*until, id.len(), id.clone()));
        found
    }

    /// Snoozes ticket `id` until `until` (or clears the snooze with `None`), with `now` as the change
    /// time; returns whether the state changed (asking for what is already so changes nothing).
    pub fn set(&mut self, id: &str, until: Option<u64>, now: u64) -> Result<bool, TicketError> {
        check_ticket_id(id)?;
        if self.until(id) == until {
            return Ok(false);
        }
        self.entries.insert(
            id.to_owned(),
            SnoozeEntry {
                until,
                updated_at: now,
            },
        );
        Ok(true)
    }

    /// Writes the store atomically (temporary file, then rename), creating the directory if needed.
    pub fn save(&self) -> Result<(), TicketError> {
        let file = SnoozeFile {
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
    fn snoozes_changes_clears_and_persists() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("dados").join("snooze.json");
        let mut store = SnoozeStore::open(path.clone()).unwrap();
        assert_eq!(store.until("1"), None);
        assert!(!store.set("1", None, 5).unwrap());

        assert!(store.set("1", Some(100), 10).unwrap());
        assert!(store.set("2", Some(50), 11).unwrap());
        assert!(!store.set("1", Some(100), 12).unwrap());
        assert!(store.set("1", Some(200), 13).unwrap());
        assert!(store.set("3", Some(300), 14).unwrap());
        assert!(store.set("3", None, 15).unwrap());
        store.save().unwrap();

        let reloaded = SnoozeStore::open(path.clone()).unwrap();
        assert_eq!(reloaded.until("1"), Some(200));
        assert_eq!(reloaded.until("3"), None);
        assert!(reloaded.entries().contains_key("3"));
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"updated_at\": 15") && saved.contains("\"version\": 1"));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn tells_pending_from_due_and_orders_them() {
        let directory = tempdir().unwrap();
        let mut store = SnoozeStore::open(directory.path().join("snooze.json")).unwrap();
        for (id, until) in [("10", 500), ("2", 500), ("3", 100), ("4", 900)] {
            store.set(id, Some(until), 1).unwrap();
        }
        store.set("5", Some(50), 1).unwrap();
        store.set("5", None, 2).unwrap();
        assert!(store.is_pending("4", 600) && !store.is_pending("3", 600));
        assert!(!store.is_pending("5", 0) && !store.is_pending("9", 0));
        // Same moment: the shorter (older) id first, so the order is stable.
        assert_eq!(
            store.pending(99),
            [
                ("3".to_owned(), 100),
                ("2".to_owned(), 500),
                ("10".to_owned(), 500),
                ("4".to_owned(), 900)
            ]
        );
        assert_eq!(store.pending(500), [("4".to_owned(), 900)]);
        assert_eq!(
            store.due(500),
            [
                ("3".to_owned(), 100),
                ("2".to_owned(), 500),
                ("10".to_owned(), 500)
            ]
        );
        assert!(store.due(99).is_empty());
    }

    #[test]
    fn merges_entries_from_another_copy_by_age() {
        let directory = tempdir().unwrap();
        let mut store = SnoozeStore::open(directory.path().join("snooze.json")).unwrap();
        let entry = |until: Option<u64>, updated_at: u64| SnoozeEntry { until, updated_at };
        assert!(store.merge_entry("1", &entry(Some(100), 5)).unwrap());
        assert!(!store.merge_entry("1", &entry(Some(100), 5)).unwrap());
        assert!(!store.merge_entry("1", &entry(Some(900), 4)).unwrap());
        assert!(store.merge_entry("1", &entry(None, 6)).unwrap());
        assert_eq!(store.until("1"), None);
        // Ties: the later return time beats a cleared snooze, and the later of two times wins.
        assert!(entry(Some(1), 6).newer_than(&entry(None, 6)));
        assert!(!entry(None, 6).newer_than(&entry(Some(1), 6)));
        assert!(entry(Some(2), 6).newer_than(&entry(Some(1), 6)));
        assert!(store.merge_entry("1", &entry(Some(1), 6)).unwrap());
        assert!(store.merge_entry("../1", &entry(Some(1), 1)).is_err());
    }

    #[test]
    fn rejects_bad_ids_and_files() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("snooze.json");
        let mut store = SnoozeStore::open(path.clone()).unwrap();
        assert!(store.set("x", Some(1), 1).is_err());

        fs::write(&path, "não é json").unwrap();
        let corrupt = SnoozeStore::open(path.clone()).unwrap_err().to_string();
        assert!(corrupt.contains("snooze file") && corrupt.contains("snooze.json"));

        fs::write(&path, r#"{"version": 2, "tickets": {}}"#).unwrap();
        let version = SnoozeStore::open(path).unwrap_err().to_string();
        assert!(version.contains("unsupported format version 2"));

        let unreadable = SnoozeStore::open(directory.path().to_path_buf()).unwrap_err();
        assert!(unreadable.to_string().contains("snooze file"));
    }

    #[test]
    fn save_reports_unwritable_locations() {
        let directory = tempdir().unwrap();
        let blocker = directory.path().join("arquivo");
        fs::write(&blocker, "x").unwrap();
        let store = SnoozeStore::open(blocker.join("dentro").join("snooze.json")).unwrap();
        assert!(store
            .save()
            .unwrap_err()
            .to_string()
            .contains("snooze file"));

        // The temporary file's name is taken by a directory, so writing it fails.
        let busy = directory.path().join("ocupado.json");
        fs::create_dir(busy.with_extension("json.tmp")).unwrap();
        assert!(SnoozeStore::open(busy).unwrap().save().is_err());

        // The destination is a directory, so the final rename fails.
        let target = directory.path().join("destino");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("dentro"), "x").unwrap();
        let mut store = SnoozeStore::open(directory.path().join("snooze.json")).unwrap();
        store.path = target;
        assert!(store.save().is_err());
    }
}
