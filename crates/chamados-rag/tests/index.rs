use std::{fs, path::Path};

use chamados_rag::{Index, Kind, Limits, Outcome, RagError, State, SCHEMA_VERSION};
use tempfile::TempDir;

fn write(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn add(index: &Index, dir: &TempDir, name: &str, bytes: &[u8]) -> i64 {
    let path = write(dir.path(), name, bytes);
    index
        .add_document(name, &path, bytes.len() as u64, 1)
        .unwrap()
}

fn run(index: &mut Index, id: i64) -> Outcome {
    index.index_document(id, &mut |_, _| true).unwrap()
}

const MANUAL: &str = "Manual da impressora\n\nPara reiniciar a impressora, desligue o aparelho da tomada por trinta segundos.\n\n\
Atolamento de papel: abra a tampa traseira e retire a folha com cuidado.\n\nTroca de toner: use apenas toner original.";

/// Um PDF de várias páginas, com uma palavra única na última linha.
fn long_pdf() -> Vec<u8> {
    let mut text: String = (0..400)
        .map(|n| format!("linha {n} do manual da rede\n"))
        .collect();
    text.push_str("palavraultima");
    chamados_app::pdf::text_to_pdf(Some("Manual"), &text)
}

#[test]
fn the_schema_is_created_versioned_and_kept_between_opens() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("docs.db");
    {
        let index = Index::open(&path).unwrap();
        let id = add(&index, &dir, "a.txt", b"texto sobre redes");
        assert_eq!(id, 1);
    }
    let again = Index::open(&path).unwrap();
    let documents = again.list().unwrap();
    assert_eq!(documents.len(), 1);
    assert_eq!(
        (
            documents[0].name.as_str(),
            documents[0].state,
            documents[0].kind.as_str()
        ),
        ("a.txt", State::Pending, "txt")
    );
    assert_eq!(count(&path, "PRAGMA user_version"), SCHEMA_VERSION);
}

/// Um número lido do arquivo do índice por uma segunda conexão (confere o que ficou gravado de fato).
fn count(path: &Path, sql: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

#[test]
fn an_index_from_a_newer_app_is_refused_and_a_bad_path_is_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("novo.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99;")
        .unwrap();
    let error = Index::open(&path).err().unwrap();
    assert!(matches!(error, RagError::NewerSchema(99)));
    assert!(error.to_string().contains("99"));
    // Um diretório no lugar do arquivo.
    assert!(matches!(
        Index::open(dir.path()).err().unwrap(),
        RagError::Db(_)
    ));
}

#[test]
fn unsupported_files_are_not_added_and_documents_are_listed_newest_first() {
    let dir = TempDir::new().unwrap();
    let index = Index::in_memory().unwrap();
    let error = index
        .add_document("planilha.xlsx", "x", 1, 1)
        .err()
        .unwrap();
    assert!(matches!(error, RagError::UnsupportedKind));
    assert!(error.to_string().contains("PDF"));
    let first = add(&index, &dir, "a.txt", b"a");
    let second = add(&index, &dir, "b.md", b"b");
    let ids: Vec<i64> = index
        .list()
        .unwrap()
        .iter()
        .map(|document| document.id)
        .collect();
    assert_eq!(ids, [second, first]);
    assert!(index.document(999).unwrap().is_none());
}

#[test]
fn a_text_document_is_indexed_and_found_ignoring_case_accents_and_plurals() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(&index, &dir, "manual.txt", MANUAL.as_bytes());
    assert_eq!(run(&mut index, id), Outcome::Done(State::Ready));
    let document = index.document(id).unwrap().unwrap();
    assert_eq!(
        (document.state, document.pages, document.indexed_pages),
        (State::Ready, Some(1), 1)
    );
    assert!(document.chunks >= 1);

    let hits = index.search("Como REINICIAR as impressoras?", 3).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        (hits[0].doc_name.as_str(), hits[0].page, hits[0].doc_id),
        ("manual.txt", None, id)
    );
    assert!(hits[0].text.contains("desligue o aparelho") && hits[0].score > 0.0);
    // Sem acento na pergunta e com acento no texto.
    assert_eq!(index.search("atolamento papel", 3).unwrap().len(), 1);
    // Nada a ver com o documento, pergunta sem palavras úteis e limite zero.
    assert!(index.search("campeonato de futebol", 3).unwrap().is_empty());
    assert!(index.search("o que é isso?", 3).unwrap().is_empty());
    assert!(index.search("impressora", 0).unwrap().is_empty());
}

#[test]
fn markdown_and_latin1_text_are_indexed_too() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let md = add(
        &index,
        &dir,
        "LEIA.MD",
        "# Guia\n\nConfigurar a **VPN** do campus".as_bytes(),
    );
    let latin1 = add(
        &index,
        &dir,
        "antigo.txt",
        b"Instala\xe7\xe3o do sistema operacional",
    );
    assert_eq!(run(&mut index, md), Outcome::Done(State::Ready));
    assert_eq!(run(&mut index, latin1), Outcome::Done(State::Ready));
    assert_eq!(
        index.search("configurar vpn", 3).unwrap()[0].doc_name,
        "LEIA.MD"
    );
    assert_eq!(
        index.search("instalacao sistema", 3).unwrap()[0].doc_name,
        "antigo.txt"
    );
}

#[test]
fn a_pdf_is_indexed_page_by_page_and_the_hit_says_which_page() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(&index, &dir, "manual.pdf", &long_pdf());
    let mut seen = Vec::new();
    let outcome = index.index_document(id, &mut |done, total| {
        seen.push((done, total));
        true
    });
    assert_eq!(outcome.unwrap(), Outcome::Done(State::Ready));
    let total = seen.last().unwrap().1;
    assert!(total > 1);
    assert_eq!(
        seen.iter().map(|pair| pair.0).collect::<Vec<_>>(),
        (1..=total).collect::<Vec<_>>()
    );
    let hits = index.search("palavraultima", 3).unwrap();
    assert_eq!(hits[0].page, Some(total));
    assert_eq!(index.document(id).unwrap().unwrap().pages, Some(total));
}

#[test]
fn cancelling_keeps_the_pages_done_and_resuming_finishes_without_duplicates() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(&index, &dir, "manual.pdf", &long_pdf());
    let outcome = index.index_document(id, &mut |done, _| done < 2).unwrap();
    assert_eq!(outcome, Outcome::Cancelled);
    let paused = index.document(id).unwrap().unwrap();
    assert_eq!((paused.state, paused.indexed_pages), (State::Pending, 2));
    // Enquanto não termina, nada do documento aparece na busca.
    assert!(index.search("palavraultima", 3).unwrap().is_empty());

    let mut resumed_from = Vec::new();
    index
        .index_document(id, &mut |done, _| {
            resumed_from.push(done);
            true
        })
        .unwrap();
    assert_eq!(resumed_from.first(), Some(&3));
    let resumed = index.document(id).unwrap().unwrap();

    let mut whole = Index::in_memory().unwrap();
    let again = add(&whole, &dir, "outro.pdf", &long_pdf());
    run(&mut whole, again);
    assert_eq!(
        resumed.chunks,
        whole.document(again).unwrap().unwrap().chunks
    );
    assert_eq!(resumed.state, State::Ready);
}

#[test]
fn documents_without_text_or_with_problems_get_their_own_state() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let empty = add(&index, &dir, "vazio.txt", b"  \n\n ");
    let scanned = add(
        &index,
        &dir,
        "scan.pdf",
        &chamados_app::pdf::text_to_pdf(None, ""),
    );
    let fake = add(&index, &dir, "falso.pdf", b"isto nao e um pdf");
    let missing = index
        .add_document(
            "sumiu.txt",
            &dir.path().join("sumiu.txt").display().to_string(),
            1,
            1,
        )
        .unwrap();
    assert_eq!(run(&mut index, empty), Outcome::Done(State::NoText));
    assert_eq!(run(&mut index, scanned), Outcome::Done(State::NoText));
    assert_eq!(run(&mut index, fake), Outcome::Done(State::Corrupted));
    assert_eq!(run(&mut index, missing), Outcome::Done(State::Corrupted));
    assert_eq!(
        index.document(fake).unwrap().unwrap().state,
        State::Corrupted
    );
}

#[test]
fn big_files_and_big_pdfs_are_refused() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    index.set_limits(Limits {
        max_bytes: 100,
        max_pages: 2,
    });
    let big_text = add(&index, &dir, "grande.txt", &[b'a'; 200]);
    let big_pdf = add(&index, &dir, "grande.pdf", &long_pdf());
    assert_eq!(run(&mut index, big_text), Outcome::Done(State::TooLarge));
    assert_eq!(run(&mut index, big_pdf), Outcome::Done(State::TooLarge));
    index.set_limits(Limits {
        max_bytes: u64::MAX,
        max_pages: 2,
    });
    let many_pages = add(&index, &dir, "paginas.pdf", &long_pdf());
    assert_eq!(run(&mut index, many_pages), Outcome::Done(State::TooLarge));
}

#[test]
fn a_ready_document_is_not_indexed_again_but_reset_starts_over() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(&index, &dir, "manual.txt", MANUAL.as_bytes());
    run(&mut index, id);
    let chunks = index.document(id).unwrap().unwrap().chunks;
    let mut calls = 0;
    assert_eq!(
        index
            .index_document(id, &mut |_, _| {
                calls += 1;
                true
            })
            .unwrap(),
        Outcome::Done(State::Ready)
    );
    assert_eq!(calls, 0);

    index.reset(id).unwrap();
    let cleared = index.document(id).unwrap().unwrap();
    assert_eq!(
        (
            cleared.state,
            cleared.pages,
            cleared.indexed_pages,
            cleared.chunks
        ),
        (State::Pending, None, 0, 0)
    );
    assert!(index.search("impressora", 3).unwrap().is_empty());
    run(&mut index, id);
    assert_eq!(index.document(id).unwrap().unwrap().chunks, chunks);
}

#[test]
fn removing_a_document_clears_chunks_search_data_and_returns_the_file_path() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("docs.db");
    let mut index = Index::open(&db).unwrap();
    let keep = add(&index, &dir, "fica.txt", b"procedimento de backup semanal");
    let drop = add(
        &index,
        &dir,
        "sai.txt",
        b"procedimento de restauracao diaria",
    );
    run(&mut index, keep);
    run(&mut index, drop);
    assert_eq!(index.search("procedimento", 5).unwrap().len(), 2);

    let path = index.remove(drop).unwrap();
    assert!(path.ends_with("sai.txt"));
    assert!(index.document(drop).unwrap().is_none());
    let remaining: Vec<String> = index
        .search("procedimento", 5)
        .unwrap()
        .into_iter()
        .map(|hit| hit.doc_name)
        .collect();
    assert_eq!(remaining, ["fica.txt"]);
    assert!(index.search("restauracao", 5).unwrap().is_empty());
    // Nada ficou no arquivo: nem os trechos, nem o que a busca guardou deles.
    assert_eq!(
        count(
            &db,
            &format!("SELECT count(*) FROM chunks WHERE doc_id = {drop}")
        ),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM chunk_fts WHERE chunk_fts MATCH 'restauracao'"
        ),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM chunk_fts WHERE chunk_fts MATCH 'backup'"
        ),
        1
    );
}

#[test]
fn unknown_documents_are_errors() {
    let mut index = Index::in_memory().unwrap();
    assert!(matches!(
        index.remove(7).err().unwrap(),
        RagError::NoSuchDocument(7)
    ));
    assert!(matches!(
        index.reset(7).err().unwrap(),
        RagError::NoSuchDocument(7)
    ));
    assert!(matches!(
        index.index_document(7, &mut |_, _| true).err().unwrap(),
        RagError::NoSuchDocument(7)
    ));
    assert!(index.remove(7).unwrap_err().to_string().contains('7'));
}

#[test]
fn search_needs_half_of_the_words_and_ranks_the_best_chunk_first() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(
        &index,
        &dir,
        "redes.txt",
        b"A rede do campus usa VPN para acesso remoto.\n\nA impressora fica na sala dois.\n\nAcesso remoto exige VPN e senha forte da rede.",
    );
    run(&mut index, id);
    // Três palavras úteis: um trecho com só uma delas não basta.
    let hits = index.search("vpn acesso remoto impressora", 5).unwrap();
    assert!(hits.iter().all(|hit| !hit.text.starts_with("A impressora")));
    assert!(!hits.is_empty());
    assert!(hits.windows(2).all(|pair| pair[0].score >= pair[1].score));
    assert_eq!(Kind::of("redes.txt"), Some(Kind::Text));
}

#[test]
fn an_encrypted_pdf_is_protected() {
    let dir = TempDir::new().unwrap();
    let mut index = Index::in_memory().unwrap();
    let id = add(&index, &dir, "senha.pdf", &encrypted_pdf());
    assert_eq!(run(&mut index, id), Outcome::Done(State::Protected));
}

/// Um PDF com o dicionário /Encrypt no trailer (o que um PDF protegido por senha tem).
fn encrypted_pdf() -> Vec<u8> {
    use lopdf::{dictionary, Document, Object};
    let mut document = Document::with_version("1.5");
    let pages = document.add_object(
        dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    let encrypt = document.add_object(dictionary! {
        "Filter" => "Standard", "V" => 1, "R" => 2,
        "O" => Object::string_literal(vec![b'a'; 32]), "U" => Object::string_literal(vec![b'b'; 32]), "P" => -4,
    });
    document.trailer.set("Root", catalog);
    document.trailer.set("Encrypt", encrypt);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}
