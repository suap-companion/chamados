//! Base de documentos do assistente de redação: importa TXT, Markdown e PDF (sem OCR), divide em
//! trechos, indexa num SQLite local com busca por palavras (FTS5/BM25) e devolve os trechos mais
//! relevantes, com o arquivo e a página de onde vieram.
//!
//! Tudo fica no aparelho: nada aqui fala com a rede. O índice é versionado (`PRAGMA user_version`), a
//! indexação é por página e retomável, e remover um documento apaga os trechos e o que a busca guardou.

pub mod chunk;
pub mod extract;
pub mod index;
pub mod search;

pub use extract::{ExtractError, Kind, Limits};
pub use index::{Document, Index, Outcome, RagError, State, SCHEMA_VERSION};
pub use search::Hit;
