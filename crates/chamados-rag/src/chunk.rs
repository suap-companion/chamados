//! Divide o texto de uma página em trechos de tamanho parecido, para o índice.
//!
//! Os trechos respeitam os parágrafos (linha em branco) e, quando um parágrafo é maior que o alvo, cortam
//! nos espaços; cada trecho novo repete o final do anterior (a sobreposição), para uma frase cortada na
//! fronteira ainda aparecer inteira em um dos dois.

/// Colapsa qualquer espaço, tabulação ou quebra de linha em um espaço só.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parte `text` (uma linha só) em pedaços de no máximo `target` caracteres, cortando nos espaços; uma
/// palavra maior que o alvo é cortada à força.
fn split_long(text: &str, target: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let word_len = word.chars().count();
        if word_len > target {
            if !current.is_empty() {
                pieces.push(std::mem::take(&mut current));
            }
            let letters: Vec<char> = word.chars().collect();
            pieces.extend(
                letters
                    .chunks(target)
                    .map(|part| part.iter().collect::<String>()),
            );
            continue;
        }
        let needed = if current.is_empty() {
            word_len
        } else {
            current.chars().count() + 1 + word_len
        };
        if needed > target {
            pieces.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

/// O final de `text`, com no máximo `overlap` caracteres, começando numa palavra inteira.
fn tail(text: &str, overlap: usize) -> String {
    let count = text.chars().count();
    if overlap == 0 || count <= overlap {
        return if overlap == 0 {
            String::new()
        } else {
            text.to_owned()
        };
    }
    let cut: String = text.chars().skip(count - overlap).collect();
    // O corte já começa numa palavra inteira se o caractere antes dele é um espaço.
    if text.chars().nth(count - overlap - 1) == Some(' ') {
        return cut;
    }
    match cut.find(' ') {
        Some(space) => cut[space + 1..].to_owned(),
        None => cut,
    }
}

/// Divide `text` em trechos de até `target` caracteres (mais a sobreposição de até `overlap`).
pub fn chunk_text(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let target = target.max(1);
    let pieces: Vec<String> = text
        .split("\n\n")
        .map(squash)
        .filter(|paragraph| !paragraph.is_empty())
        .flat_map(|paragraph| split_long(&paragraph, target))
        .collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        if current.is_empty() {
            current = piece;
        } else if current.chars().count() + 1 + piece.chars().count() <= target {
            current.push(' ');
            current.push_str(&piece);
        } else {
            let carried = tail(&current, overlap);
            chunks.push(std::mem::take(&mut current));
            current = if carried.is_empty() {
                piece
            } else {
                format!("{carried} {piece}")
            };
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_blank_text_has_no_chunks() {
        assert!(chunk_text("", 100, 10).is_empty());
        assert!(chunk_text(" \n\n \t \n\n", 100, 10).is_empty());
    }

    #[test]
    fn short_paragraphs_are_packed_into_one_chunk() {
        let chunks = chunk_text("Primeiro   parágrafo.\n\nSegundo\nparágrafo.", 100, 10);
        assert_eq!(chunks, ["Primeiro parágrafo. Segundo parágrafo."]);
    }

    #[test]
    fn a_full_chunk_starts_the_next_one_with_the_overlap_from_a_whole_word() {
        let text = "um dois três quatro\n\ncinco seis sete oito";
        let chunks = chunk_text(text, 22, 11);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "um dois três quatro");
        // "três quatro" (11) é o final do primeiro, e começa numa palavra inteira.
        assert_eq!(chunks[1], "três quatro cinco seis sete oito");
        assert!(chunks[1].starts_with("três quatro"));
    }

    #[test]
    fn long_paragraphs_are_cut_at_spaces_without_losing_words() {
        let words: Vec<String> = (0..60).map(|n| format!("p{n:02}")).collect();
        let text = words.join(" ");
        let chunks = chunk_text(&text, 40, 0);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 40));
        assert_eq!(chunks.join(" "), text);
    }

    #[test]
    fn a_word_longer_than_the_target_is_cut_hard() {
        let chunks = chunk_text(&format!("ab {} cd", "x".repeat(25)), 10, 0);
        assert_eq!(
            chunks,
            ["ab", "xxxxxxxxxx", "xxxxxxxxxx", "xxxxx cd"].map(String::from)
        );
    }

    #[test]
    fn overlap_edge_cases() {
        // Sobreposição maior que o trecho: leva o trecho inteiro; sem espaço no corte: leva o corte.
        assert_eq!(tail("abc", 10), "abc");
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc def", 0), "");
        // O corte cai no meio de uma palavra: a palavra partida fica de fora.
        assert_eq!(tail("um dois três quatro", 9), "quatro");
        let chunks = chunk_text("aaaa bbbb\n\ncccc dddd", 9, 100);
        assert_eq!(
            chunks,
            ["aaaa bbbb", "aaaa bbbb cccc dddd"].map(String::from)
        );
        // Alvo zero vira um (não trava).
        assert_eq!(chunk_text("ab", 0, 0), ["a", "b"].map(String::from));
    }

    #[test]
    fn accents_count_as_one_character() {
        let chunks = chunk_text("ação ação ação", 9, 0);
        assert_eq!(chunks, ["ação ação", "ação"].map(String::from));
    }
}
