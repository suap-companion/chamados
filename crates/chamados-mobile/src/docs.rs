//! A base de documentos de apoio do assistente: o app importa PDF, TXT e Markdown, o Rust indexa no
//! aparelho (por página, com progresso e cancelamento) e devolve os trechos mais relevantes, com o
//! arquivo e a página, para o rascunho citar. Nada sai do aparelho.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use chamados_rag as rag;

use crate::{failed, sync::unix_now, ChamadosError};

/// Em que pé está um documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DocState {
    /// Esperando para ser indexado (ou parado no meio: continua de onde parou).
    Pending,
    Indexing,
    Ready,
    /// Sem texto (por exemplo, PDF digitalizado: o app não faz OCR).
    NoText,
    /// PDF protegido por senha.
    Protected,
    /// Arquivo inválido, danificado ou que sumiu.
    Corrupted,
    /// Grande demais (tamanho ou páginas).
    TooLarge,
}

impl From<rag::State> for DocState {
    fn from(state: rag::State) -> Self {
        match state {
            rag::State::Pending => Self::Pending,
            rag::State::Indexing => Self::Indexing,
            rag::State::Ready => Self::Ready,
            rag::State::NoText => Self::NoText,
            rag::State::Protected => Self::Protected,
            rag::State::Corrupted => Self::Corrupted,
            rag::State::TooLarge => Self::TooLarge,
        }
    }
}

/// Um documento da base.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DocInfo {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub state: DocState,
    /// Páginas do PDF (só depois de lido).
    pub pages: Option<u32>,
    pub indexed_pages: u32,
    pub chunks: u32,
}

impl From<rag::Document> for DocInfo {
    fn from(document: rag::Document) -> Self {
        Self {
            id: document.id,
            name: document.name,
            kind: document.kind,
            size: document.size,
            state: document.state.into(),
            pages: document.pages,
            indexed_pages: document.indexed_pages,
            chunks: document.chunks,
        }
    }
}

/// Um trecho achado na busca.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DocHit {
    pub doc_id: i64,
    /// O nome do arquivo de onde veio.
    pub source: String,
    pub page: Option<u32>,
    pub text: String,
    pub score: f64,
}

impl From<rag::Hit> for DocHit {
    fn from(hit: rag::Hit) -> Self {
        Self {
            doc_id: hit.doc_id,
            source: hit.doc_name,
            page: hit.page,
            text: hit.text,
            score: hit.score,
        }
    }
}

/// Como terminou uma rodada de indexação.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct IndexResult {
    pub state: DocState,
    /// A pessoa cancelou: o que já foi gravado fica e a próxima rodada continua dali.
    pub cancelled: bool,
}

/// O controle de uma indexação em curso: quem chamou lê o progresso (de outra thread, enquanto a
/// indexação roda) e pode cancelar; o que já foi gravado fica e a próxima rodada continua dali.
#[derive(Default, uniffi::Object)]
pub struct IndexControl {
    cancelled: AtomicBool,
    done: AtomicU32,
    total: AtomicU32,
}

#[uniffi::export]
impl IndexControl {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Pede para parar; a indexação termina a página em andamento e devolve `cancelled`.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Páginas já indexadas.
    pub fn done(&self) -> u32 {
        self.done.load(Ordering::SeqCst)
    }

    /// Total de páginas (0 até o documento ser lido).
    pub fn total(&self) -> u32 {
        self.total.load(Ordering::SeqCst)
    }
}

impl IndexControl {
    /// Registra uma página feita e diz se deve continuar.
    fn page(&self, done: u32, total: u32) -> bool {
        self.done.store(done, Ordering::SeqCst);
        self.total.store(total, Ordering::SeqCst);
        !self.cancelled.load(Ordering::SeqCst)
    }
}

/// A base de documentos de uma pasta: o índice SQLite e as cópias privadas dos arquivos.
#[derive(uniffi::Object)]
pub struct Docs {
    db: PathBuf,
    files: PathBuf,
}

impl Docs {
    /// Cada chamada abre a sua conexão: ler a lista não espera uma indexação longa terminar.
    fn index(&self) -> Result<rag::Index, ChamadosError> {
        rag::Index::open(&self.db).map_err(failed)
    }
}

/// O nome do arquivo sem o que atrapalha num caminho (só letras, números, ponto e hífen).
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[uniffi::export]
impl Docs {
    /// Abre (ou cria) a base na pasta `dir`: `dir/index.db` e `dir/files/`.
    #[uniffi::constructor]
    pub fn open(dir: String) -> Result<Arc<Self>, ChamadosError> {
        let root = Path::new(&dir);
        let files = root.join("files");
        fs::create_dir_all(&files).map_err(failed)?;
        let docs = Self {
            db: root.join("index.db"),
            files,
        };
        docs.index()?;
        Ok(Arc::new(docs))
    }

    /// Importa um arquivo (PDF, TXT ou Markdown): copia para a pasta privada, registra e devolve o número
    /// do documento, ainda sem indexar. `source` é uma cópia temporária que o chamador apaga depois.
    pub fn import_file(&self, name: String, source: String) -> Result<i64, ChamadosError> {
        let size = fs::metadata(&source).map_err(failed)?.len();
        let now = unix_now() as i64;
        let target = self.files.join(format!("{now}-{}", safe_name(&name)));
        let path = target.display().to_string();
        let id = self
            .index()?
            .add_document(&name, &path, size, now)
            .map_err(failed)?;
        fs::copy(&source, &target).map_err(failed)?;
        Ok(id)
    }

    /// Todos os documentos, os mais novos primeiro.
    pub fn list(&self) -> Result<Vec<DocInfo>, ChamadosError> {
        let documents = self.index()?.list().map_err(failed)?;
        Ok(documents.into_iter().map(DocInfo::from).collect())
    }

    /// Indexa o documento (ou continua de onde parou). Bloqueia até terminar: chame de uma thread de
    /// fundo e use `control` (de outra) para ver o progresso ou cancelar.
    pub fn index_document(
        &self,
        id: i64,
        control: Arc<IndexControl>,
    ) -> Result<IndexResult, ChamadosError> {
        let mut index = self.index()?;
        let outcome = index
            .index_document(id, &mut |done, total| control.page(done, total))
            .map_err(failed)?;
        Ok(match outcome {
            rag::Outcome::Done(state) => IndexResult {
                state: state.into(),
                cancelled: false,
            },
            rag::Outcome::Cancelled => IndexResult {
                state: DocState::Pending,
                cancelled: true,
            },
        })
    }

    /// Esquece o que foi indexado do documento (para indexar de novo).
    pub fn reindex(&self, id: i64) -> Result<(), ChamadosError> {
        self.index()?.reset(id).map_err(failed)
    }

    /// Apaga o documento: o arquivo, os trechos e o que a busca guardou deles.
    pub fn remove(&self, id: i64) -> Result<(), ChamadosError> {
        let path = self.index()?.remove(id).map_err(failed)?;
        // O arquivo pode já ter sumido; o que importa é que o índice não guarda mais nada.
        fs::remove_file(path).ok();
        Ok(())
    }

    /// Os trechos mais relevantes para `query` (só de documentos prontos), até `limit`.
    pub fn search(&self, query: String, limit: u32) -> Result<Vec<DocHit>, ChamadosError> {
        let hits = self
            .index()?
            .search(&query, limit as usize)
            .map_err(failed)?;
        Ok(hits.into_iter().map(DocHit::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> (tempfile::TempDir, Arc<Docs>) {
        let dir = tempfile::tempdir().unwrap();
        let docs = Docs::open(dir.path().join("docs").display().to_string()).unwrap();
        (dir, docs)
    }

    fn source(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> String {
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        path.display().to_string()
    }

    #[test]
    fn a_document_is_imported_indexed_found_and_removed_with_its_file() {
        let (dir, docs) = docs();
        let text = "Para reiniciar a impressora, desligue da tomada.";
        let id = docs
            .import_file(
                "Manual da impressora.txt".to_owned(),
                source(&dir, "tmp.txt", text.as_bytes()),
            )
            .unwrap();
        let listed = docs.list().unwrap();
        assert_eq!(
            (listed[0].id, listed[0].state, listed[0].kind.as_str()),
            (id, DocState::Pending, "txt")
        );
        assert_eq!(listed[0].size, text.len() as u64);

        let control = IndexControl::new();
        assert_eq!((control.done(), control.total()), (0, 0));
        let result = docs.index_document(id, control.clone()).unwrap();
        assert!(!result.cancelled);
        assert_eq!(result.state, DocState::Ready);
        assert_eq!((control.done(), control.total()), (1, 1));

        let hits = docs.search("reiniciar impressora".to_owned(), 3).unwrap();
        assert_eq!(
            (hits[0].source.as_str(), hits[0].page, hits[0].doc_id),
            ("Manual da impressora.txt", None, id)
        );
        assert!(hits[0].score > 0.0 && hits[0].text.contains("tomada"));

        let files = dir.path().join("docs").join("files");
        let copy = fs::read_dir(files).unwrap().next().unwrap().unwrap().path();
        let name = copy.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("Manual_da_impressora.txt"));
        docs.remove(id).unwrap();
        assert!(!copy.exists());
        assert!(docs.list().unwrap().is_empty());
        assert!(docs.search("reiniciar".to_owned(), 3).unwrap().is_empty());
    }

    #[test]
    fn cancelling_leaves_the_document_pending_and_reindex_starts_over() {
        let (dir, docs) = docs();
        let pdf = chamados_app::pdf::text_to_pdf(None, &"linha do manual\n".repeat(400));
        let temp = source(&dir, "t.pdf", &pdf);
        let id = docs.import_file("manual.pdf".to_owned(), temp).unwrap();
        // Cancelado de antemão: a primeira página é gravada e a indexação para ali.
        let control = IndexControl::new();
        control.cancel();
        let result = docs.index_document(id, control.clone()).unwrap();
        assert!(result.cancelled);
        assert_eq!(result.state, DocState::Pending);
        assert_eq!(control.done(), 1);
        assert!(control.total() > 1);
        let paused = &docs.list().unwrap()[0];
        assert_eq!((paused.indexed_pages, paused.state), (1, DocState::Pending));
        assert!(paused.pages.is_some());

        docs.reindex(id).unwrap();
        let cleared = &docs.list().unwrap()[0];
        assert_eq!(
            (cleared.indexed_pages, cleared.chunks, cleared.pages),
            (0, 0, None)
        );
        let done = docs.index_document(id, IndexControl::new()).unwrap();
        assert_eq!(done.state, DocState::Ready);
    }

    #[test]
    fn problems_become_states_and_errors_become_messages() {
        let (dir, docs) = docs();
        let fake = source(&dir, "f.pdf", b"nao e pdf");
        let id = docs.import_file("falso.pdf".to_owned(), fake).unwrap();
        let result = docs.index_document(id, IndexControl::new()).unwrap();
        assert_eq!(result.state, DocState::Corrupted);
        assert_eq!(docs.list().unwrap()[0].state, DocState::Corrupted);

        let sheet = source(&dir, "p.xlsx", b"x");
        assert!(docs.import_file("planilha.xlsx".to_owned(), sheet).is_err());
        let missing = dir.path().join("nao-existe").display().to_string();
        assert!(docs.import_file("a.txt".to_owned(), missing).is_err());
        assert!(docs.remove(999).is_err());
        assert!(docs.reindex(999).is_err());
        assert!(docs.index_document(999, IndexControl::new()).is_err());
        // A pasta é um arquivo: não dá para abrir a base.
        let blocked = source(&dir, "bloqueado", b"x");
        assert!(Docs::open(blocked).is_err());
    }

    #[test]
    fn every_state_has_its_mobile_twin() {
        let states = [
            (rag::State::Pending, DocState::Pending),
            (rag::State::Indexing, DocState::Indexing),
            (rag::State::Ready, DocState::Ready),
            (rag::State::NoText, DocState::NoText),
            (rag::State::Protected, DocState::Protected),
            (rag::State::Corrupted, DocState::Corrupted),
            (rag::State::TooLarge, DocState::TooLarge),
        ];
        for (core, mobile) in states {
            assert_eq!(DocState::from(core), mobile);
        }
    }
}
