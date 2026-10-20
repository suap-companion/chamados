//! O índice dos documentos: um arquivo SQLite com esquema versionado, trechos e busca por palavras
//! (FTS5, BM25), no aparelho.
//!
//! Cada documento passa por estados (`Pending → Indexing → Ready`, ou um estado de problema). A
//! indexação grava página a página e guarda quantas já foram, então recomeça de onde parou depois de um
//! cancelamento ou de o processo ser morto.

use std::{fs, path::Path};

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    chunk::chunk_text,
    extract::{pdf_pages, text_of_bytes, ExtractError, Kind, Limits},
    search,
};

/// A versão atual do esquema (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 1;
/// O tamanho alvo de um trecho e a sobreposição, em caracteres.
pub const CHUNK_CHARS: usize = 800;
pub const CHUNK_OVERLAP: usize = 100;

const INSERT_DOCUMENT: &str = "INSERT INTO documents (name, path, kind, size, state, added_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)";
const PRAGMAS: &str =
    "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 10000;";
const SET_STATE: &str = "UPDATE documents SET state = ?2 WHERE id = ?1";
const DELETE_CHUNKS: &str = "DELETE FROM chunks WHERE doc_id = ?1";
const DELETE_DOCUMENT: &str = "DELETE FROM documents WHERE id = ?1";
const RESET_DOCUMENT: &str = "UPDATE documents SET state = 'pending', pages = NULL, indexed_pages = 0, chunks = 0 WHERE id = ?1";
const SET_PAGES: &str = "UPDATE documents SET pages = ?2 WHERE id = ?1";
const OPTIMIZE_FTS: &str = "INSERT INTO chunk_fts(chunk_fts) VALUES ('optimize')";
const GET_CHUNKS: &str = "SELECT chunks FROM documents WHERE id = ?1";
const INSERT_CHUNK: &str =
    "INSERT INTO chunks (doc_id, page, ordinal, text) VALUES (?1, ?2, ?3, ?4)";
const ADD_CHUNKS: &str =
    "UPDATE documents SET indexed_pages = ?2, chunks = chunks + ?3 WHERE id = ?1";
const LIST_DOCUMENTS: &str = "SELECT id, name, path, kind, size, state, pages, indexed_pages, chunks FROM documents ORDER BY added_at DESC, id DESC";
const GET_DOCUMENT: &str = "SELECT id, name, path, kind, size, state, pages, indexed_pages, chunks FROM documents WHERE id = ?1";

/// Executa um comando com parametros. Os chamadores cabem numa linha so: assim o formatador nao deixa
/// um `)?;` sozinho, que a medicao de cobertura conta como linha perdida.
fn exec(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<(), RagError> {
    conn.execute(sql, params)?;
    Ok(())
}

/// Erros do índice (o texto da causa nunca inclui o conteúdo dos documentos).
#[derive(Debug, thiserror::Error)]
pub enum RagError {
    #[error("erro no banco de dados do índice: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("não foi possível ler ou gravar o arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("o índice é de uma versão mais nova do app (esquema {0})")]
    NewerSchema(i64),
    #[error("documento {0} não existe")]
    NoSuchDocument(i64),
    #[error("tipo de arquivo não aceito (use PDF, TXT ou Markdown)")]
    UnsupportedKind,
}

/// Em que pé está um documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Indexing,
    Ready,
    /// Sem texto (por exemplo, PDF digitalizado: o app não faz OCR).
    NoText,
    Protected,
    Corrupted,
    TooLarge,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Indexing => "indexing",
            Self::Ready => "ready",
            Self::NoText => "no_text",
            Self::Protected => "protected",
            Self::Corrupted => "corrupted",
            Self::TooLarge => "too_large",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "indexing" => Self::Indexing,
            "ready" => Self::Ready,
            "no_text" => Self::NoText,
            "protected" => Self::Protected,
            "corrupted" => Self::Corrupted,
            "too_large" => Self::TooLarge,
            _ => Self::Pending,
        }
    }
}

impl From<ExtractError> for State {
    fn from(error: ExtractError) -> Self {
        match error {
            ExtractError::Protected => Self::Protected,
            ExtractError::TooLarge => Self::TooLarge,
            ExtractError::Corrupted | ExtractError::Unsupported => Self::Corrupted,
        }
    }
}

/// Um documento do índice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub state: State,
    /// Páginas do PDF (vazio antes de ler).
    pub pages: Option<u32>,
    pub indexed_pages: u32,
    pub chunks: u32,
}

/// Como terminou uma rodada de indexação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Terminou: o documento ficou neste estado (pronto ou com um problema).
    Done(State),
    /// A pessoa cancelou: as páginas já gravadas ficam e a próxima rodada continua dali.
    Cancelled,
}

pub struct Index {
    pub(crate) conn: Connection,
    limits: Limits,
}

impl Index {
    /// Abre (ou cria) o índice no arquivo `path`, migrando o esquema se preciso.
    pub fn open(path: &Path) -> Result<Self, RagError> {
        Self::init(Connection::open(path)?)
    }

    /// Um índice só na memória (para os testes).
    pub fn in_memory() -> Result<Self, RagError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, RagError> {
        // Leitores e a indexação em curso convivem (WAL) e esperam um pouco em vez de falhar na hora.
        conn.execute_batch(PRAGMAS)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(RagError::NewerSchema(version));
        }
        if version < 1 {
            conn.execute_batch(include_str!("schema_v1.sql"))?;
        }
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(Self {
            conn,
            limits: Limits::default(),
        })
    }

    /// Muda os limites (tamanho e páginas); o padrão é 50 MB e 500 páginas.
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// Registra um documento (ainda sem indexar). `path` é a cópia privada do arquivo.
    pub fn add_document(
        &self,
        name: &str,
        path: &str,
        size: u64,
        now: i64,
    ) -> Result<i64, RagError> {
        let kind = Kind::of(name).ok_or(RagError::UnsupportedKind)?;
        let values = (name, path, kind.as_str(), size as i64, now);
        exec(&self.conn, INSERT_DOCUMENT, values)?;
        Ok(self.conn.last_insert_rowid())
    }

    fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
        Ok(Document {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            kind: row.get(3)?,
            size: row.get::<_, i64>(4)? as u64,
            state: State::parse(&row.get::<_, String>(5)?),
            pages: row.get::<_, Option<i64>>(6)?.map(|pages| pages as u32),
            indexed_pages: row.get::<_, i64>(7)? as u32,
            chunks: row.get::<_, i64>(8)? as u32,
        })
    }

    /// Todos os documentos, os mais novos primeiro.
    pub fn list(&self) -> Result<Vec<Document>, RagError> {
        let mut statement = self.conn.prepare(LIST_DOCUMENTS)?;
        let documents = statement
            .query_map([], Self::row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    /// Um documento, se existe.
    pub fn document(&self, id: i64) -> Result<Option<Document>, RagError> {
        let found = self
            .conn
            .query_row(GET_DOCUMENT, [id], Self::row)
            .optional()?;
        Ok(found)
    }

    fn set_state(&self, id: i64, state: State) -> Result<(), RagError> {
        exec(&self.conn, SET_STATE, params![id, state.as_str()])
    }

    /// Apaga o documento, os trechos e o que a busca guardou deles; devolve o caminho da cópia do arquivo
    /// para quem chamou apagá-la do disco.
    pub fn remove(&self, id: i64) -> Result<String, RagError> {
        let document = self.document(id)?.ok_or(RagError::NoSuchDocument(id))?;
        exec(&self.conn, DELETE_CHUNKS, [id])?;
        exec(&self.conn, DELETE_DOCUMENT, [id])?;
        // Compacta o índice de busca: nada do documento removido fica nele.
        exec(&self.conn, OPTIMIZE_FTS, [])?;
        Ok(document.path)
    }

    /// Esquece o que foi indexado e volta o documento a "pendente" (para indexar de novo).
    pub fn reset(&self, id: i64) -> Result<(), RagError> {
        self.document(id)?.ok_or(RagError::NoSuchDocument(id))?;
        exec(&self.conn, DELETE_CHUNKS, [id])?;
        exec(&self.conn, RESET_DOCUMENT, [id])
    }

    /// Grava os trechos de uma página (ou do texto todo) numa transação e conta a página como feita.
    fn store_page(
        &mut self,
        id: i64,
        page: Option<u32>,
        text: &str,
        done: u32,
    ) -> Result<(), RagError> {
        let transaction = self.conn.transaction()?;
        let first: i64 = transaction.query_row(GET_CHUNKS, [id], |row| row.get(0))?;
        let chunks = chunk_text(text, CHUNK_CHARS, CHUNK_OVERLAP);
        for (offset, chunk) in chunks.iter().enumerate() {
            transaction.execute(
                INSERT_CHUNK,
                params![id, page, first + offset as i64, chunk],
            )?;
        }
        transaction.execute(ADD_CHUNKS, params![id, done, chunks.len() as i64])?;
        transaction.commit()?;
        Ok(())
    }

    /// Indexa o documento (ou continua de onde parou). `progress(feitas, total)` roda a cada página e,
    /// se devolver `false`, cancela: o que já foi gravado fica.
    pub fn index_document(
        &mut self,
        id: i64,
        progress: &mut dyn FnMut(u32, u32) -> bool,
    ) -> Result<Outcome, RagError> {
        let document = self.document(id)?.ok_or(RagError::NoSuchDocument(id))?;
        if document.state == State::Ready {
            return Ok(Outcome::Done(State::Ready));
        }
        self.set_state(id, State::Indexing)?;
        let bytes = match fs::read(&document.path) {
            Ok(bytes) => bytes,
            Err(_) => return self.finish(id, State::Corrupted),
        };
        if bytes.len() as u64 > self.limits.max_bytes {
            return self.finish(id, State::TooLarge);
        }
        let is_pdf = document.kind == Kind::Pdf.as_str();
        let pages: Vec<String> = if is_pdf {
            match pdf_pages(&bytes, self.limits) {
                Ok(pages) => pages,
                Err(error) => return self.finish(id, error.into()),
            }
        } else {
            vec![text_of_bytes(&bytes)]
        };
        let total = pages.len() as u32;
        exec(&self.conn, SET_PAGES, params![id, total])?;
        for (number, text) in pages
            .iter()
            .enumerate()
            .skip(document.indexed_pages as usize)
        {
            let done = number as u32 + 1;
            let page = is_pdf.then_some(done);
            self.store_page(id, page, text, done)?;
            if !progress(done, total) {
                self.set_state(id, State::Pending)?;
                return Ok(Outcome::Cancelled);
            }
        }
        let chunks = self.document(id)?.map_or(0, |found| found.chunks);
        let state = if chunks == 0 {
            State::NoText
        } else {
            State::Ready
        };
        self.finish(id, state)
    }

    fn finish(&self, id: i64, state: State) -> Result<Outcome, RagError> {
        self.set_state(id, state)?;
        Ok(Outcome::Done(state))
    }

    /// Os trechos mais relevantes para a pergunta (só de documentos prontos), até `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<search::Hit>, RagError> {
        search::search(&self.conn, query, limit)
    }
}
