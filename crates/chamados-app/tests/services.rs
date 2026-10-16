//! Os serviços de aplicação contra um SUAP simulado: acompanhamento, lote e rodada de sincronização.

use chamados_app::{
    batch::{send_messages, BatchReport},
    download::download_attachments,
    listing::{narrow, without_hidden, ListView, Marks},
    syncing::sync_round,
    watching::watch_once,
    AppError,
};
use chamados_core::{
    watch::WatchEvent, ArchiveStore, Message, RemoteTicket, SnoozeStore, SuapTicketSource,
    TicketDetails, TicketFilter, TicketQueue, TitleStore,
};
use chamados_sync::{backend_from, Key, SyncOptions};
use suap_core::{AppPaths, SuapClient, SuapConfig, SyncSettings};
use tempfile::{tempdir, TempDir};
use url::Url;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn client_for(server: &MockServer) -> (TempDir, AppPaths, SuapClient) {
    let directory = tempdir().unwrap();
    let paths = AppPaths::from_dirs(directory.path().join("c"), directory.path().join("d"));
    let config = SuapConfig {
        base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
        ..SuapConfig::default()
    };
    let client = SuapClient::open(&paths, &config).unwrap();
    (directory, paths, client)
}

async fn page(server: &MockServer, verb: &str, at: &str, status: u16, body: &str) {
    Mock::given(method(verb))
        .and(path(at.to_owned()))
        .respond_with(ResponseTemplate::new(status).set_body_string(body.to_owned()))
        .mount(server)
        .await;
}

fn listing(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| {
            format!(
                r#"<div class="general-box"><span class="status">Aberto</span>
                <h4><a href="/centralservicos/chamado/{id}/">REQ #{id}</a></h4></div>"#
            )
        })
        .collect()
}

#[tokio::test]
async fn one_watch_round_remembers_what_it_saw_for_the_next() {
    let server = MockServer::start().await;
    let (dir, _, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Mine);
    let state = dir.path().join("estado").join("watch.json");
    let filter = TicketFilter::default();
    let list = "/centralservicos/meus_chamados/";

    page(&server, "GET", list, 200, &listing(&[1])).await;
    let first = watch_once(&source, &filter, "mine:active", false, &state)
        .await
        .unwrap();
    assert!(first.is_empty(), "the first look is only a baseline");
    assert!(state.exists());

    server.reset().await;
    page(&server, "GET", list, 200, &listing(&[1, 2])).await;
    let second = watch_once(&source, &filter, "mine:active", false, &state)
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert!(matches!(&second[0], WatchEvent::New { id, .. } if id == "2"));

    // A list that cannot be read is an error and leaves the saved state as it was.
    server.reset().await;
    page(&server, "GET", list, 500, "").await;
    assert!(watch_once(&source, &filter, "mine:active", false, &state)
        .await
        .is_err());
    std::fs::write(&state, "isto nao e json").unwrap();
    assert!(watch_once(&source, &filter, "mine:active", false, &state)
        .await
        .is_err());
}

#[tokio::test]
async fn a_batch_reports_each_ticket_and_stops_without_a_session() {
    let server = MockServer::start().await;
    let (_dir, _, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    let form = r#"<form action="/centralservicos/adicionar_comentario/5/" method="post">
        <input type="hidden" name="csrfmiddlewaretoken" value="tok"><textarea name="texto"></textarea></form>"#;
    page(&server, "GET", "/centralservicos/chamado/5/", 200, form).await;
    page(
        &server,
        "POST",
        "/centralservicos/adicionar_comentario/5/",
        200,
        "ok",
    )
    .await;
    page(
        &server,
        "GET",
        "/centralservicos/chamado/6/",
        200,
        "<p>sem formulário</p>",
    )
    .await;

    let report = send_messages(&source, &[5, 6, 5], Message::Comment, "oi")
        .await
        .unwrap();
    assert_eq!(report.sent, ["5"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, "6");
    assert!(
        report.failed[0].1.contains("no comment form"),
        "{:?}",
        report.failed
    );
    assert_eq!(BatchReport::default().sent.len(), 0);

    server.reset().await;
    page(&server, "GET", "/accounts/login/", 200, "").await;
    Mock::given(method("GET"))
        .and(path("/centralservicos/chamado/7/"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/accounts/login/"))
        .mount(&server)
        .await;
    assert!(send_messages(&source, &[7, 8], Message::InternalNote, "x")
        .await
        .is_err());
}

#[tokio::test]
async fn downloads_report_what_they_refuse() {
    let server = MockServer::start().await;
    let (dir, _, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    let details = |html: &str| -> TicketDetails {
        chamados_core::parse_ticket_details(
            html,
            &Url::parse("https://suap.example/").unwrap(),
            "9",
        )
        .unwrap()
    };
    let none =
        details(r#"<main id="content"><div class="title-container"><h2>Sem</h2></div></main>"#);
    let refused = download_attachments(&source, &none, &[], dir.path(), false)
        .await
        .unwrap_err();
    assert!(matches!(refused, AppError::Message(ref text) if text.contains("não tem anexos")));
    let one = details(
        r#"<main id="content"><div class="title-container"><h2>Um</h2></div>
        <div data-tab="linha_tempo"><ul class="timeline"><li><div class="timeline-date">1</div>
        <div class="timeline-content"><a href="/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/">a.csv</a></div></li></ul></div></main>"#,
    );
    let missing = download_attachments(&source, &one, &[2], dir.path(), false)
        .await
        .unwrap_err();
    assert!(
        missing.to_string().contains("o anexo 2 não existe"),
        "{missing}"
    );
    let unreachable = download_attachments(&source, &one, &[1], dir.path(), false)
        .await
        .unwrap_err();
    assert!(matches!(unreachable, AppError::Ticket(_)), "{unreachable}");
    // A folder that cannot be created is an I/O error.
    page(
        &server,
        "GET",
        "/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/",
        200,
        "a,b",
    )
    .await;
    let blocked = dir.path().join("arquivo");
    std::fs::write(&blocked, "x").unwrap();
    let io = download_attachments(&source, &one, &[1], &blocked.join("dentro"), false)
        .await
        .unwrap_err();
    assert!(matches!(io, AppError::Io(_)), "{io}");
}

#[test]
fn narrowing_by_local_title_and_limit() {
    let directory = tempdir().unwrap();
    let mut titles = TitleStore::open(directory.path().join("titles.json")).unwrap();
    titles.set("1", "Atualizar o Moodle", 1).unwrap();
    titles.set("2", "Rede do campus", 1).unwrap();
    let ticket = |id: &str| RemoteTicket {
        id: id.to_owned(),
        subject: None,
        status: None,
        details_url: String::new(),
        photo: None,
    };
    let all = || vec![ticket("1"), ticket("2"), ticket("3")];
    let ids =
        |tickets: Vec<RemoteTicket>| -> Vec<String> { tickets.into_iter().map(|t| t.id).collect() };
    let mut archive = ArchiveStore::open(directory.path().join("archive.json")).unwrap();
    let mut snooze = SnoozeStore::open(directory.path().join("snooze.json")).unwrap();
    let now = 1_000;
    let narrowed = |archive: &ArchiveStore, snooze: &SnoozeStore, view, needle, limit| {
        let marks = Marks {
            archive,
            snooze,
            now,
        };
        ids(narrow(all(), &titles, &marks, view, needle, limit))
    };
    let normal = ListView::Normal;
    assert_eq!(
        narrowed(&archive, &snooze, normal, None, None),
        ["1", "2", "3"]
    );
    assert_eq!(
        narrowed(&archive, &snooze, normal, Some("MOODLE"), None),
        ["1"]
    );
    assert_eq!(
        narrowed(&archive, &snooze, normal, Some("nada"), None),
        Vec::<String>::new()
    );
    assert_eq!(
        narrowed(&archive, &snooze, normal, None, Some(2)),
        ["1", "2"]
    );

    // Archived tickets leave the usual lists and are the only ones in the archive view.
    archive.set("2", true, 1).unwrap();
    assert_eq!(narrowed(&archive, &snooze, normal, None, None), ["1", "3"]);
    let only = ListView::Archived;
    assert_eq!(narrowed(&archive, &snooze, only, None, None), ["2"]);
    assert_eq!(narrowed(&archive, &snooze, only, Some("rede"), None), ["2"]);
    assert!(narrowed(&archive, &snooze, only, Some("moodle"), None).is_empty());

    // A snoozed ticket is out of the usual lists until its time; then it comes back on top, marked.
    snooze.set("1", Some(5_000), 1).unwrap();
    snooze.set("3", Some(2_000), 1).unwrap();
    assert_eq!(
        narrowed(&archive, &snooze, normal, None, None),
        Vec::<String>::new()
    );
    let waiting = ListView::Snoozed;
    assert_eq!(narrowed(&archive, &snooze, waiting, None, None), ["3", "1"]);
    snooze.set("2", Some(3_000), 1).unwrap();
    // An archived ticket is not in the snoozed view either.
    assert_eq!(narrowed(&archive, &snooze, waiting, None, None), ["3", "1"]);
    let marks = |now| Marks {
        archive: &archive,
        snooze: &snooze,
        now,
    };
    assert!(marks(1_500).hidden("1") && marks(1_500).hidden("2") && !marks(1_500).returned("1"));
    let back = ids(narrow(all(), &titles, &marks(2_500), normal, None, None));
    assert_eq!(back, ["3"]);
    let later = ids(narrow(all(), &titles, &marks(9_000), normal, None, None));
    assert_eq!(later, ["1", "3"]);
    assert!(marks(9_000).returned("1") && !marks(9_000).returned("2"));
    assert!(!marks(9_000).hidden("1") && marks(9_000).hidden("2"));
    // Their watch events are not announced while hidden.
    let event = |id: &str| WatchEvent::Left {
        id: id.to_owned(),
        status: None,
    };
    let kept = without_hidden(vec![event("1"), event("2"), event("3")], &marks(2_500));
    assert_eq!(kept, [event("3")]);

    // Seeing the ticket clears the snooze: it is just a normal ticket again, no longer on top.
    snooze.set("1", None, 2).unwrap();
    let marks = Marks {
        archive: &archive,
        snooze: &snooze,
        now: 9_000,
    };
    let again = ids(narrow(all(), &titles, &marks, normal, None, None));
    assert_eq!(again, ["3", "1"]);
    assert!(!marks.returned("1") && marks.returned("3"));
}

#[tokio::test]
async fn a_sync_round_takes_the_lock_and_gives_way_when_it_is_taken() {
    let directory = tempdir().unwrap();
    let cloud = directory.path().join("nuvem");
    let paths = AppPaths::from_dirs(directory.path().join("c"), directory.path().join("d"));
    paths.ensure_dirs().unwrap();
    let settings = SyncSettings {
        backend: Some("directory".to_owned()),
        path: Some(cloud.display().to_string()),
        ..SyncSettings::default()
    };
    let backend = backend_from(&settings, None).unwrap();
    let key = Key::generate();
    let options = SyncOptions {
        only: None,
        dry_run: false,
    };

    let done = sync_round(&paths, &backend, &key, &options).await.unwrap();
    assert!(done.is_some());
    std::fs::write(paths.sync_lock_file(), "").unwrap();
    assert!(sync_round(&paths, &backend, &key, &options)
        .await
        .unwrap()
        .is_none());
}
