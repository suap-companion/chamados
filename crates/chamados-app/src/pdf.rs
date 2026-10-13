//! Texto simples em PDF, para anexar a um chamado o que veio como texto (o SUAP não aceita `.txt`).
//!
//! Um gerador mínimo, sem dependências: páginas A4 só com texto em Helvetica (codificação
//! WinAnsi), com quebra de linha por palavras e várias páginas quando o texto é longo. Caracteres
//! que a WinAnsi não tem viram `?`.

const PAGE_WIDTH: u32 = 595;
const PAGE_HEIGHT: u32 = 842;
const MARGIN: u32 = 56;
const BODY_SIZE: u32 = 11;
const TITLE_SIZE: u32 = 14;
const LEADING: u32 = 14;
/// Characters per line: the text width over the average width of a Helvetica character at 11 pt.
const COLUMNS: usize = 86;
const LINES_PER_PAGE: usize = ((PAGE_HEIGHT - 2 * MARGIN) / LEADING) as usize;

/// The byte WinAnsi uses for `c`, or `?` when it has none.
fn win_ansi(c: char) -> u8 {
    match c {
        '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => c as u8,
        '€' => 0x80,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ => b'?',
    }
}

/// A PDF string literal with the bytes of `line` (parentheses and backslashes escaped).
fn literal(line: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for byte in line.chars().map(win_ansi) {
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out.push(b')');
    out
}

/// Breaks `text` into lines of at most [`COLUMNS`] characters, at spaces when possible.
fn wrap(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.replace('\r', "").replace('\t', "    ").split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let mut word = word.to_owned();
            while word.chars().count() > COLUMNS {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let head: String = word.chars().take(COLUMNS).collect();
                word = word.chars().skip(COLUMNS).collect();
                lines.push(head);
            }
            let needed =
                line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if needed > COLUMNS && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        lines.push(line);
    }
    lines
}

/// The content stream of one page: the title (first page only) and its lines of text.
fn page_stream(title: Option<&str>, lines: &[String]) -> Vec<u8> {
    let top = PAGE_HEIGHT - MARGIN - BODY_SIZE;
    let mut stream = format!("BT\n{MARGIN} {top} Td\n{LEADING} TL\n").into_bytes();
    if let Some(title) = title {
        stream.extend(format!("/F2 {TITLE_SIZE} Tf\n").into_bytes());
        stream.extend(literal(title));
        stream.extend(b" Tj\nT*\nT*\n");
    }
    stream.extend(format!("/F1 {BODY_SIZE} Tf\n").into_bytes());
    for line in lines {
        stream.extend(literal(line));
        stream.extend(b" Tj\nT*\n");
    }
    stream.extend(b"ET");
    stream
}

/// A PDF with `text` (and `title` in bold above it, when given).
pub fn text_to_pdf(title: Option<&str>, text: &str) -> Vec<u8> {
    let lines = wrap(text.trim_end_matches(['\r', '\n']));
    // The title takes three lines of the first page (itself and two of spacing).
    let first_capacity = LINES_PER_PAGE - if title.is_some() { 3 } else { 0 };
    let mut pages: Vec<(Option<&str>, &[String])> = Vec::new();
    let split = first_capacity.min(lines.len());
    pages.push((title, &lines[..split]));
    for chunk in lines[split..].chunks(LINES_PER_PAGE) {
        pages.push((None, chunk));
    }

    // Objects: 1 catalog, 2 page tree, 3 and 4 fonts, then a page and its content per page.
    let mut objects: Vec<Vec<u8>> = vec![Vec::new(); 4 + 2 * pages.len()];
    let kids: Vec<String> = (0..pages.len())
        .map(|n| format!("{} 0 R", 5 + 2 * n))
        .collect();
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    )
    .into_bytes();
    objects[2] =
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec();
    objects[3] =
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec();
    for (n, (page_title, page_lines)) in pages.iter().enumerate() {
        let (page, content) = (4 + 2 * n, 5 + 2 * n);
        objects[page] = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
             /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
            content + 1
        )
        .into_bytes();
        let stream = page_stream(*page_title, page_lines);
        let mut body = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        body.extend(stream);
        body.extend(b"\nendstream");
        objects[content] = body;
    }

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", index + 1).into_bytes());
        pdf.extend(object);
        pdf.extend(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for offset in offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").into_bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .into_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(pdf: &[u8]) -> String {
        pdf.iter().map(|byte| *byte as char).collect()
    }

    fn position(haystack: &[u8], needle: &str, from: usize) -> usize {
        let needle = needle.as_bytes();
        let found = haystack[from..]
            .windows(needle.len())
            .position(|window| window == needle);
        from + found.expect("the PDF has what the test looks for")
    }

    fn number_after(pdf: &[u8], marker: &str, from: usize) -> usize {
        let start = position(pdf, marker, from) + marker.len();
        let digits: String = pdf[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .map(|byte| *byte as char)
            .collect();
        digits.parse().unwrap()
    }

    /// Every object starts exactly where the cross-reference table says, the table is where
    /// `startxref` points and each stream is as long as it says: what a PDF reader checks first.
    fn assert_well_formed(pdf: &[u8]) {
        assert!(
            pdf.starts_with(
                b"%PDF-1.4
"
            ) && pdf.ends_with(
                b"%%EOF
"
            )
        );
        let last = pdf.windows(9).rposition(|w| w == b"startxref").unwrap();
        let start = number_after(
            pdf,
            "startxref
",
            last,
        );
        assert!(pdf[start..].starts_with(
            b"xref
"
        ));
        let count = number_after(
            pdf, "xref
0 ", start,
        );
        let first_entry = position(
            pdf,
            "
",
            position(
                pdf, "
", start,
            ) + 1,
        ) + 1;
        for number in 1..count {
            let at = first_entry + number * 20;
            let offset: usize = std::str::from_utf8(&pdf[at..at + 10])
                .unwrap()
                .parse()
                .unwrap();
            let expected = format!(
                "{number} 0 obj
"
            );
            assert!(
                pdf[offset..].starts_with(expected.as_bytes()),
                "object {number} is not at {offset}"
            );
        }
        let mut from = 0;
        while let Some(found) = pdf[from..].windows(8).position(|w| w == b"/Length ") {
            let at = from + found;
            let length = number_after(pdf, "/Length ", at);
            let body = position(
                pdf, "stream
", at,
            ) + "stream
"
            .len();
            assert_eq!(
                &pdf[body + length..body + length + 10],
                b"
endstream"
            );
            from = body;
        }
    }

    #[test]
    fn a_short_text_becomes_one_valid_page_with_its_title() {
        let pdf = text_to_pdf(
            Some("Título do texto"),
            "primeira linha\nsegunda (com parênteses)\\",
        );
        assert_well_formed(&pdf);
        let text = text_of(&pdf);
        assert_eq!(text.matches("/Type /Page ").count(), 1);
        assert!(text.contains("/Count 1"));
        // Accents use their WinAnsi byte (ç = 0xE7), and (, ) and \ are escaped.
        assert!(text.contains("(T\u{ed}tulo do texto) Tj") || text.contains("(T\u{ed}tulo"));
        assert!(text.contains("par\u{ea}nteses"), "{text}");
        assert!(text.contains("\\(com par"), "{text}");
        assert!(text.contains("teses\\)"), "{text}");
        assert!(text.contains("\\\\) Tj"), "{text}");
        assert!(text.contains("/F2 14 Tf"));
        // Without a title there is no bold line.
        assert!(!text_of(&text_to_pdf(None, "só texto")).contains("/F2 14 Tf"));
    }

    #[test]
    fn long_lines_wrap_and_long_texts_take_several_pages() {
        let words = "palavra ".repeat(40);
        let wrapped = wrap(&words);
        assert!(wrapped.len() > 1 && wrapped.iter().all(|line| line.chars().count() <= COLUMNS));
        // A word longer than a line is cut, after what was already on the line.
        let giant = format!("curta {}", "x".repeat(COLUMNS * 2 + 5));
        let cut = wrap(&giant);
        assert_eq!(cut[0], "curta");
        assert_eq!(cut[1].len(), COLUMNS);
        assert_eq!(cut.last().unwrap().len(), 5);
        // Blank lines are kept, tabs become spaces, a CRLF ending is the same as LF.
        assert_eq!(wrap("a\r\n\r\nb\tc"), ["a", "", "b    c"]);

        let many: String = (1..=130).map(|n| format!("linha {n}\n")).collect();
        let pdf = text_to_pdf(Some("Longo"), &many);
        assert_well_formed(&pdf);
        let text = text_of(&pdf);
        let pages = text.matches("/Type /Page ").count();
        assert!(pages >= 3, "{pages}");
        assert!(text.contains(&format!("/Count {pages}")));
        assert!(text.contains("(linha 130)"));
    }

    #[test]
    fn characters_outside_winansi_become_question_marks_and_empty_text_is_one_empty_page() {
        for (given, expected) in [
            ('a', b'a'),
            ('ç', 0xe7),
            ('€', 0x80),
            ('…', 0x85),
            ('‘', 0x91),
            ('’', 0x92),
            ('“', 0x93),
            ('”', 0x94),
            ('•', 0x95),
            ('–', 0x96),
            ('—', 0x97),
            ('日', b'?'),
            ('\u{85}', b'?'),
        ] {
            assert_eq!(win_ansi(given), expected, "{given:?}");
        }
        let pdf = text_to_pdf(None, "");
        assert_well_formed(&pdf);
        assert_eq!(text_of(&pdf).matches("/Type /Page ").count(), 1);
        assert_well_formed(&text_to_pdf(Some("só título"), ""));
    }
}
