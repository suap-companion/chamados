//! Nomes de arquivo seguros para gravar o que o SUAP entrega.

/// Names Windows reserves for devices, whatever the extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
/// Longest file name kept, in characters.
pub const MAX_FILE_NAME_CHARS: usize = 150;

/// A file name that is safe to create in a chosen folder, from a name SUAP gave: no folders, no
/// characters the file systems refuse, no reserved or empty names. `fallback` is used when nothing
/// usable is left.
pub fn safe_file_name(name: &str, fallback: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(MAX_FILE_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_owned();
    if cleaned.is_empty() {
        return fallback.to_owned();
    }
    let stem = cleaned.split('.').next().unwrap_or_default().to_uppercase();
    if RESERVED_NAMES.contains(&stem.as_str()) {
        return format!("_{cleaned}");
    }
    cleaned
}

/// `name` with the extension of the stored file added when it does not already end with it. The
/// name SUAP shows for an attachment is the description given to it, which may have no extension.
pub fn with_stored_extension(name: &str, stored_name: &str) -> String {
    let extension = match stored_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => extension,
        _ => return name.to_owned(),
    };
    let suffix = format!(".{}", extension.to_lowercase());
    if name.to_lowercase().ends_with(&suffix) {
        return name.to_owned();
    }
    format!("{name}.{extension}")
}

/// `name` made different from every name in `taken` by adding " (2)", " (3)"... before the extension.
pub fn unique_name(name: &str, taken: &[String]) -> String {
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let mut candidate = name.to_owned();
    let mut number = 1;
    while taken.contains(&candidate) {
        number += 1;
        candidate = format!("{stem} ({number}){extension}");
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_from_suap_are_made_safe_and_unique() {
        for (given, expected) in [
            ("dados.csv", "dados.csv"),
            ("relatório final.pdf", "relatório final.pdf"),
            ("../../etc/passwd", "passwd"),
            ("C:\\Users\\x\\segredo.docx", "segredo.docx"),
            ("a<b>c:d\"e|f?g*h.txt", "a_b_c_d_e_f_g_h.txt"),
            ("  espaço.png  ", "espaço.png"),
            ("termina.com.ponto...", "termina.com.ponto"),
            ("con.txt", "_con.txt"),
            ("NUL", "_NUL"),
            ("Lpt1.pdf", "_Lpt1.pdf"),
            ("com10.txt", "com10.txt"),
            ("", "anexo-3"),
            ("...", "anexo-3"),
            ("..", "anexo-3"),
            ("pasta/", "anexo-3"),
            ("tab\there.txt", "tab_here.txt"),
        ] {
            assert_eq!(safe_file_name(given, "anexo-3"), expected, "{given:?}");
        }
        let long = format!("{}.pdf", "x".repeat(400));
        assert_eq!(
            safe_file_name(&long, "f").chars().count(),
            MAX_FILE_NAME_CHARS
        );

        for (name, stored, expected) in [
            ("Tela do erro", "foto-3f2a.png", "Tela do erro.png"),
            ("dados.csv", "dados-3f2a.csv", "dados.csv"),
            ("DADOS.CSV", "dados-3f2a.csv", "DADOS.CSV"),
            ("Versão 1.2", "relatorio.pdf", "Versão 1.2.pdf"),
            ("relatorio.pdf", "relatorio.docx", "relatorio.pdf.docx"),
            ("sem extensão", "anexo", "sem extensão"),
            ("sem extensão", "", "sem extensão"),
            ("sem extensão", ".oculto", "sem extensão"),
            ("sem extensão", "arquivo.", "sem extensão"),
        ] {
            assert_eq!(
                with_stored_extension(name, stored),
                expected,
                "{name} {stored}"
            );
        }

        let taken = [
            "a.csv".to_owned(),
            "a (2).csv".to_owned(),
            "semponto".to_owned(),
        ];
        assert_eq!(unique_name("b.csv", &taken), "b.csv");
        assert_eq!(unique_name("a.csv", &taken), "a (3).csv");
        assert_eq!(unique_name("semponto", &taken), "semponto (2)");
        assert_eq!(
            unique_name(".oculto", &[".oculto".to_owned()]),
            ".oculto (2)"
        );
    }
}
