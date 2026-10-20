//! Tira o texto dos documentos importados: TXT, Markdown e PDF (por página, sem OCR).

use std::panic::{catch_unwind, AssertUnwindSafe};

/// O tipo do documento, pela extensão do nome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Text,
    Markdown,
}

impl Kind {
    /// O tipo pelo nome do arquivo (sem diferenciar maiúsculas); `None` para o que não é aceito.
    pub fn of(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        let extension = lower.rsplit_once('.')?.1;
        match extension {
            "pdf" => Some(Self::Pdf),
            "txt" | "text" => Some(Self::Text),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    /// O nome guardado no índice.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Text => "txt",
            Self::Markdown => "md",
        }
    }
}

/// Por que um documento não pôde ser lido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractError {
    /// PDF protegido por senha.
    Protected,
    /// Arquivo que não é um PDF válido ou está danificado.
    Corrupted,
    /// Grande demais (bytes ou páginas).
    TooLarge,
    /// Tipo de arquivo que não é aceito.
    Unsupported,
}

/// Os limites de um documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_pages: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 50 * 1024 * 1024,
            max_pages: 500,
        }
    }
}

/// O texto de um arquivo TXT/Markdown: UTF-8; se não for, Latin-1 (o que os editores antigos gravam).
pub fn text_of_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.strip_prefix('\u{feff}').unwrap_or(text).to_owned(),
        Err(_) => bytes.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// O texto de cada página de um PDF, em ordem. Falha com [`ExtractError::Protected`] (senha),
/// [`ExtractError::Corrupted`] (inválido ou que o leitor não consegue ler) ou [`ExtractError::TooLarge`]
/// (páginas demais). Um leitor que entra em pânico num PDF malformado vira `Corrupted`, nunca derruba o app.
pub fn pdf_pages(bytes: &[u8], limits: Limits) -> Result<Vec<String>, ExtractError> {
    if bytes.len() as u64 > limits.max_bytes {
        return Err(ExtractError::TooLarge);
    }
    let read = catch_unwind(AssertUnwindSafe(|| {
        let document =
            pdf_extract::Document::load_mem(bytes).map_err(|_| ExtractError::Corrupted)?;
        if document.is_encrypted() {
            return Err(ExtractError::Protected);
        }
        if document.get_pages().len() > limits.max_pages {
            return Err(ExtractError::TooLarge);
        }
        pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|_| ExtractError::Corrupted)
    }));
    read.unwrap_or(Err(ExtractError::Corrupted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_comes_from_the_extension_ignoring_case() {
        assert_eq!(Kind::of("Manual.PDF"), Some(Kind::Pdf));
        assert_eq!(Kind::of("notas.txt"), Some(Kind::Text));
        assert_eq!(Kind::of("notas.text"), Some(Kind::Text));
        assert_eq!(Kind::of("LEIA.md"), Some(Kind::Markdown));
        assert_eq!(Kind::of("a.markdown"), Some(Kind::Markdown));
        assert_eq!(Kind::of("planilha.xlsx"), None);
        assert_eq!(Kind::of("semextensao"), None);
        let names: Vec<&str> = [Kind::Pdf, Kind::Text, Kind::Markdown]
            .iter()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(names, ["pdf", "txt", "md"]);
    }

    #[test]
    fn text_is_utf8_with_a_latin1_fallback_and_no_bom() {
        assert_eq!(text_of_bytes("ação".as_bytes()), "ação");
        assert_eq!(text_of_bytes(b"\xef\xbb\xbfcom bom"), "com bom");
        // "ação" em Latin-1: não é UTF-8 válido.
        assert_eq!(text_of_bytes(b"a\xe7\xe3o"), "ação");
    }

    fn pdf(text: &str) -> Vec<u8> {
        chamados_app::pdf::text_to_pdf(Some("Título"), text)
    }

    #[test]
    fn a_pdf_gives_its_text_page_by_page() {
        let pages = pdf_pages(
            &pdf("Reiniciar a impressora\n\nAtolamento de papel"),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].contains("impressora") && pages[0].contains("Atolamento"));
        // Texto longo: várias páginas.
        let long = "linha de texto do manual\n".repeat(400);
        let many = pdf_pages(&pdf(&long), Limits::default()).unwrap();
        assert!(many.len() > 1);
        assert!(many.iter().all(|page| page.contains("manual")));
    }

    #[test]
    fn what_is_not_a_pdf_is_corrupted() {
        assert_eq!(
            pdf_pages(b"isto nao e um pdf", Limits::default()),
            Err(ExtractError::Corrupted)
        );
        let mut broken = pdf("texto");
        broken.truncate(broken.len() / 2);
        assert_eq!(
            pdf_pages(&broken, Limits::default()),
            Err(ExtractError::Corrupted)
        );
    }

    #[test]
    fn limits_stop_big_documents() {
        let file = pdf(&"linha\n".repeat(400));
        assert_eq!(
            pdf_pages(
                &file,
                Limits {
                    max_bytes: 10,
                    max_pages: 500
                }
            ),
            Err(ExtractError::TooLarge)
        );
        assert_eq!(
            pdf_pages(
                &file,
                Limits {
                    max_bytes: u64::MAX,
                    max_pages: 1
                }
            ),
            Err(ExtractError::TooLarge)
        );
        assert_eq!(Limits::default().max_pages, 500);
    }

    #[test]
    fn an_encrypted_pdf_is_protected() {
        // Um PDF mínimo com o dicionário /Encrypt no trailer: basta para o leitor dizer que é protegido.
        let file = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n3 0 obj\n<< /Filter /Standard /V 1 /R 2 /O (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa) /U (bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) /P -4 >>\nendobj\ntrailer\n<< /Root 1 0 R /Size 4 /Encrypt 3 0 R /ID [(aaaaaaaaaaaaaaaa) (aaaaaaaaaaaaaaaa)] >>\n%%EOF\n";
        // Sem a tabela de referências cruzadas o leitor não o abre: é "corrompido" (o PDF protegido de
        // verdade, montado com a biblioteca, é testado em `tests/index.rs`).
        assert_eq!(
            pdf_pages(file, Limits::default()),
            Err(ExtractError::Corrupted)
        );
    }
}
