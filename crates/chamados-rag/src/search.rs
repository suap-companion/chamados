//! A busca nos trechos indexados: palavras da pergunta (sem acento, sem maiúsculas, sem palavras comuns),
//! FTS5/BM25 e um filtro de cobertura que descarta trechos que só casam com uma palavra solta.

use rusqlite::Connection;

use crate::index::RagError;

/// Um trecho achado, com de onde veio.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub doc_id: i64,
    pub doc_name: String,
    /// A página (PDF); `None` em TXT e Markdown.
    pub page: Option<u32>,
    pub text: String,
    /// Quanto maior, mais relevante (o oposto do BM25 do SQLite).
    pub score: f64,
}

/// Palavras comuns do português que não ajudam a achar nada.
const STOPWORDS: &[&str] = &[
    "que", "com", "uma", "uns", "umas", "por", "para", "pra", "como", "mais", "mas", "das", "dos",
    "nas", "nos", "num", "numa", "ele", "ela", "eles", "elas", "seu", "sua", "seus", "suas", "meu",
    "minha", "isso", "isto", "esse", "essa", "este", "esta", "esses", "essas", "aquele", "aquela",
    "qual", "quais", "quando", "onde", "quem", "foi", "ser", "sao", "tem", "ter", "tinha", "esta",
    "estao", "ate", "ainda", "tambem", "sobre", "entre", "sem", "sob", "pois", "porque", "voce",
    "voces", "ja",
];

/// Minúsculas e sem acentos do português.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|letter| match letter {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// As palavras que valem na pergunta: ao menos 3 letras, sem as comuns, sem repetir (até 12).
pub fn terms(query: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for word in fold(query).split(|letter: char| !letter.is_alphanumeric()) {
        if word.chars().count() >= 3
            && !STOPWORDS.contains(&word)
            && !found.iter().any(|known| known == word)
        {
            found.push(word.to_owned());
        }
    }
    found.truncate(12);
    found
}

/// A consulta do FTS5: cada palavra entre aspas, com `*` (prefixo) nas de 4 letras ou mais, ligadas por OR.
fn match_expression(terms: &[String]) -> String {
    terms
        .iter()
        .map(|term| {
            if term.chars().count() >= 4 {
                format!("\"{term}\"*")
            } else {
                format!("\"{term}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Quantas das palavras da pergunta aparecem (como início de palavra) no trecho.
fn covered(terms: &[String], text: &str) -> usize {
    let folded = fold(text);
    let words: Vec<&str> = folded
        .split(|letter: char| !letter.is_alphanumeric())
        .collect();
    terms
        .iter()
        .filter(|term| words.iter().any(|word| word.starts_with(term.as_str())))
        .count()
}

const SEARCH_SQL: &str = "SELECT c.doc_id, d.name, c.page, c.text, bm25(chunk_fts) FROM chunk_fts JOIN chunks c ON c.id = chunk_fts.rowid JOIN documents d ON d.id = c.doc_id WHERE chunk_fts MATCH ?1 AND d.state = 'ready' ORDER BY bm25(chunk_fts) LIMIT 30";

/// Uma linha do resultado da busca como [`Hit`].
fn hit_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<Hit> {
    Ok(Hit {
        doc_id: row.get(0)?,
        doc_name: row.get(1)?,
        page: row.get::<_, Option<i64>>(2)?.map(|page| page as u32),
        text: row.get(3)?,
        score: -row.get::<_, f64>(4)?,
    })
}

pub(crate) fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Hit>, RagError> {
    let terms = terms(query);
    if terms.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(SEARCH_SQL)?;
    let rows = statement.query_map([match_expression(&terms)], hit_of)?;
    // Pede metade das palavras (arredondado para cima): um trecho que só casa com uma entre várias não serve.
    let needed = terms.len().div_ceil(2);
    let mut hits = Vec::new();
    for row in rows {
        let hit = row?;
        if covered(&terms, &hit.text) >= needed {
            hits.push(hit);
        }
    }
    hits.truncate(limit);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_lowercases_and_drops_portuguese_accents() {
        assert_eq!(
            fold("AÇÃO é Útil, Ñ ÀÂÄ ÈÊË ÌÎÏ ÒÔÕÖ ÙÛÜ"),
            "acao e util, n aaa eee iii oooo uuu"
        );
    }

    #[test]
    fn terms_drop_short_common_and_repeated_words() {
        assert_eq!(
            terms("Como reiniciar a impressora? Reiniciar a IMPRESSORA do setor"),
            ["reiniciar", "impressora", "setor"]
        );
        assert!(terms("o que é isso?").is_empty());
        let many: String = (0..20).map(|n| format!("palavra{n} ")).collect();
        assert_eq!(terms(&many).len(), 12);
    }

    #[test]
    fn the_match_expression_quotes_every_term_and_uses_prefixes_for_long_ones() {
        let terms = vec!["rede".to_owned(), "vpn".to_owned()];
        assert_eq!(match_expression(&terms), "\"rede\"* OR \"vpn\"");
    }

    #[test]
    fn coverage_counts_terms_found_as_word_starts() {
        let terms = vec![
            "impressora".to_owned(),
            "toner".to_owned(),
            "vpn".to_owned(),
        ];
        assert_eq!(covered(&terms, "A Impressoras com TONER baixo"), 2);
        assert_eq!(covered(&terms, "nada a ver"), 0);
    }
}
