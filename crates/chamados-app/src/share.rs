//! Separar, no que chegou por "Compartilhar", o que o SUAP aceita como anexo do que ele recusa.

use chamados_core::{ALLOWED_ATTACHMENT_EXTENSIONS, MAX_ATTACHMENT_BYTES};

/// A file that was shared with the app: only what is needed to decide if SUAP takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedFile {
    pub name: String,
    pub size: u64,
}

/// A shared file SUAP will not take, and why (in Portuguese, to be shown as it is).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    /// Position in the list that was classified.
    pub index: usize,
    pub name: String,
    pub reason: String,
}

/// The shared files split into the ones to send and the ones to refuse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Classified {
    /// Positions (in the list that was classified) of the files SUAP accepts, in order.
    pub accepted: Vec<usize>,
    pub rejected: Vec<Rejected>,
}

fn problem(file: &SharedFile) -> Option<String> {
    let extension = file
        .name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if !ALLOWED_ATTACHMENT_EXTENSIONS.contains(&extension.as_str()) {
        return Some(format!(
            "tipo não aceito pelo SUAP (aceitos: {})",
            ALLOWED_ATTACHMENT_EXTENSIONS.join(", ")
        ));
    }
    if file.size == 0 {
        return Some("arquivo vazio".to_owned());
    }
    if file.size > MAX_ATTACHMENT_BYTES as u64 {
        let megabytes = MAX_ATTACHMENT_BYTES / (1024 * 1024);
        return Some(format!("maior que o limite de {megabytes} MB"));
    }
    None
}

/// Splits `files` into accepted and rejected. With `limit` (opening a ticket takes at most
/// [`chamados_core::MAX_ATTACHMENTS`] files), the files past the limit are rejected too.
pub fn classify_shared(files: &[SharedFile], limit: Option<usize>) -> Classified {
    let mut result = Classified::default();
    for (index, file) in files.iter().enumerate() {
        let reason = match problem(file) {
            Some(reason) => Some(reason),
            None if limit.is_some_and(|limit| result.accepted.len() >= limit) => Some(format!(
                "passa do limite de {} anexos",
                limit.unwrap_or_default()
            )),
            None => None,
        };
        match reason {
            Some(reason) => result.rejected.push(Rejected {
                index,
                name: file.name.clone(),
                reason,
            }),
            None => result.accepted.push(index),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> SharedFile {
        SharedFile {
            name: name.to_owned(),
            size,
        }
    }

    #[test]
    fn only_the_types_suap_takes_are_accepted() {
        let files = [
            file("foto.JPG", 10),
            file("relatorio.pdf", 10),
            file("planilha.xlsx", 10),
            file("texto.txt", 10),
            file("programa.exe", 10),
            file("semextensao", 10),
            file("png.", 10),
        ];
        let result = classify_shared(&files, None);
        assert_eq!(result.accepted, [0, 1, 2]);
        let rejected: Vec<usize> = result.rejected.iter().map(|r| r.index).collect();
        assert_eq!(rejected, [3, 4, 5, 6]);
        assert!(result.rejected[0]
            .reason
            .contains("tipo não aceito pelo SUAP"));
        assert!(result.rejected[0]
            .reason
            .contains("xlsx, xls, csv, docx, doc, pdf, jpg, jpeg, png"));
        assert_eq!(result.rejected[0].name, "texto.txt");
    }

    #[test]
    fn empty_and_oversized_files_are_refused_and_the_ticket_limit_applies() {
        let limit = MAX_ATTACHMENT_BYTES as u64;
        let files = [
            file("vazio.pdf", 0),
            file("no-limite.pdf", limit),
            file("grande.pdf", limit + 1),
            file("a.png", 1),
            file("b.png", 1),
            file("c.png", 1),
        ];
        let open_ticket = classify_shared(&files, Some(2));
        assert_eq!(open_ticket.accepted, [1, 3]);
        let reasons: Vec<&str> = open_ticket
            .rejected
            .iter()
            .map(|r| r.reason.as_str())
            .collect();
        assert_eq!(reasons[0], "arquivo vazio");
        assert_eq!(reasons[1], "maior que o limite de 10 MB");
        assert_eq!(reasons[2], "passa do limite de 2 anexos");
        assert_eq!(reasons[3], "passa do limite de 2 anexos");
        // Attaching to an existing ticket has no limit on how many.
        assert_eq!(classify_shared(&files, None).accepted, [1, 3, 4, 5]);
        assert_eq!(classify_shared(&[], Some(3)), Classified::default());
    }
}
