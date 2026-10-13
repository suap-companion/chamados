//! Baixar os anexos de um chamado para uma pasta, com nomes seguros e sem sobrescrever em silêncio.

use std::{
    fs,
    path::{Path, PathBuf},
};

use chamados_core::{SuapTicketSource, TicketDetails, TicketSource};

use crate::{
    files::{safe_file_name, unique_name, with_stored_extension},
    AppError,
};

/// A downloaded attachment, as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFile {
    /// Position in the ticket's list of attachments (1-based).
    pub number: usize,
    pub path: PathBuf,
    pub size: usize,
}

/// Downloads the attachments `wanted` (every one when empty; numbers start at 1 and repeats count
/// once) of the ticket described by `details` into `folder`, creating it when needed.
///
/// Everything is fetched before anything is written, and nothing is written when a file would
/// replace an existing one (unless `force`): the name SUAP shows may be only a description, and the
/// kind of file is known from the name it is stored under.
pub async fn download_attachments(
    source: &SuapTicketSource<'_>,
    details: &TicketDetails,
    wanted: &[usize],
    folder: &Path,
    force: bool,
) -> Result<Vec<SavedFile>, AppError> {
    let (id, total) = (&details.id, details.attachments.len());
    if total == 0 {
        return Err(AppError::Message(format!("o chamado #{id} não tem anexos")));
    }
    let mut numbers: Vec<usize> = if wanted.is_empty() {
        (1..=total).collect()
    } else {
        wanted.to_vec()
    };
    let mut seen = Vec::new();
    numbers.retain(|number| {
        let first = !seen.contains(number);
        seen.push(*number);
        first
    });
    if let Some(bad) = numbers.iter().find(|number| !(1..=total).contains(number)) {
        return Err(AppError::Message(format!(
            "o anexo {bad} não existe: o chamado #{id} tem {total}"
        )));
    }

    let mut files = Vec::new();
    for number in &numbers {
        let link = &details.attachments[number - 1];
        files.push(source.download_attachment(link).await?);
    }
    let mut names: Vec<String> = Vec::new();
    for (number, file) in numbers.iter().zip(&files) {
        let link = &details.attachments[number - 1];
        let safe = safe_file_name(&link.name, &format!("anexo-{number}"));
        let name = unique_name(&with_stored_extension(&safe, &file.stored_name), &names);
        names.push(name);
    }
    let existing: Vec<String> = names
        .iter()
        .filter(|name| folder.join(name).exists())
        .cloned()
        .collect();
    if !force && !existing.is_empty() {
        return Err(AppError::Message(format!(
            "já existe em {}: {} (use --force para sobrescrever)",
            folder.display(),
            existing.join(", ")
        )));
    }
    fs::create_dir_all(folder)?;
    let mut saved = Vec::new();
    for ((number, name), file) in numbers.iter().zip(&names).zip(&files) {
        let path = folder.join(name);
        fs::write(&path, &file.bytes)?;
        saved.push(SavedFile {
            number: *number,
            path,
            size: file.bytes.len(),
        });
    }
    Ok(saved)
}
