//! `apply_share` contra um SUAP simulado: a ordem dos passos, as recusas antes de enviar e a
//! retomada depois de uma falha.

use chamados_app::apply::{apply_share, AfterOpen, ShareAction, ShareReport, ShareRequest};
use chamados_core::{Attachment, SuapTicketSource, TicketQueue};
use suap_core::{AppPaths, SuapClient, SuapConfig};
use tempfile::{tempdir, TempDir};
use url::Url;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn client_for(server: &MockServer) -> (TempDir, SuapClient) {
    let directory = tempdir().unwrap();
    let paths = AppPaths::from_dirs(directory.path().join("c"), directory.path().join("d"));
    let config = SuapConfig {
        base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
        ..SuapConfig::default()
    };
    let client = SuapClient::open(&paths, &config).unwrap();
    (directory, client)
}

async fn page(server: &MockServer, verb: &str, at: &str, status: u16, body: &str) {
    Mock::given(method(verb))
        .and(path(at.to_owned()))
        .respond_with(ResponseTemplate::new(status).set_body_string(body.to_owned()))
        .mount(server)
        .await;
}

/// A status-change form (`field` is the text box SUAP asks the message in) that accepts the post.
async fn form(server: &MockServer, prefix: &str, id: u32, field: &str) {
    let at = format!("{prefix}{id}/");
    let body = format!(
        r#"<form action="{at}" method="post"><input type="hidden" name="csrfmiddlewaretoken" value="t">
        <textarea name="{field}"></textarea><input name="descricao"><input type="file" name="anexo"></form>"#
    );
    page(server, "GET", &at, 200, &body).await;
    page(server, "POST", &at, 200, "ok").await;
}

/// The ticket page, with the comment and the internal note forms, and where they are posted.
async fn ticket_page(server: &MockServer, id: u32) {
    let forms: String = ["adicionar_comentario", "adicionar_nota_interna"]
        .iter()
        .map(|name| {
            format!(
                r#"<form action="/centralservicos/{name}/{id}/" method="post"><input type="hidden" name="csrfmiddlewaretoken" value="t">
                <textarea name="texto"></textarea></form>"#
            )
        })
        .collect();
    page(
        server,
        "GET",
        &format!("/centralservicos/chamado/{id}/"),
        200,
        &forms,
    )
    .await;
    for name in ["adicionar_comentario", "adicionar_nota_interna"] {
        page(
            server,
            "POST",
            &format!("/centralservicos/{name}/{id}/"),
            200,
            "ok",
        )
        .await;
    }
}

fn request(action: ShareAction, ticket: Option<&str>, text: Option<&str>) -> ShareRequest {
    ShareRequest {
        action,
        ticket: ticket.map(str::to_owned),
        text: text.map(str::to_owned),
        title: None,
        files: Vec::new(),
        attachment_description: None,
        resume_from: 0,
    }
}

fn file(name: &str, size: usize) -> Attachment {
    Attachment {
        file_name: name.to_owned(),
        bytes: vec![b'x'; size],
    }
}

fn labels(report: &ShareReport) -> Vec<&str> {
    report.steps.iter().map(|s| s.label.as_str()).collect()
}

#[tokio::test]
async fn each_action_runs_its_steps_in_order() {
    let server = MockServer::start().await;
    let (_dir, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    form(&server, "/centralservicos/adicionar_anexo/", 5, "x").await;
    ticket_page(&server, 5).await;
    form(
        &server,
        "/centralservicos/suspender_chamado/",
        5,
        "observacao",
    )
    .await;
    form(
        &server,
        "/centralservicos/cancelar_chamado/",
        5,
        "observacao",
    )
    .await;
    form(
        &server,
        "/centralservicos/reabrir_chamado/",
        5,
        "observacao",
    )
    .await;
    form(
        &server,
        "/centralservicos/resolver_chamado/",
        5,
        "comentario",
    )
    .await;

    // Text alone, attached: it becomes a PDF.
    let mut attach = request(ShareAction::Attach, Some("5"), Some("conteúdo"));
    attach.title = Some("Título".to_owned());
    let report = apply_share(&source, &attach).await.unwrap();
    assert_eq!(labels(&report), ["Anexar texto.pdf"]);
    assert!(report.is_done(), "{:?}", report.steps);
    assert_eq!((report.completed, report.total), (1, 1));
    assert_eq!(report.ticket.as_deref(), Some("5"));

    // Files go before the message.
    let mut comment = request(ShareAction::Comment, Some("5"), Some("oi"));
    comment.files = vec![file("a.png", 3), file("b.pdf", 3)];
    let report = apply_share(&source, &comment).await.unwrap();
    assert_eq!(
        labels(&report),
        ["Anexar a.png", "Anexar b.pdf", "Adicionar o comentário"]
    );
    assert!(report.is_done(), "{:?}", report.steps);

    let note = request(ShareAction::InternalNote, Some("5"), Some("nota"));
    let report = apply_share(&source, &note).await.unwrap();
    assert_eq!(labels(&report), ["Adicionar a nota interna"]);
    for (action, label) in [
        (ShareAction::Suspend, "Suspender o chamado"),
        (ShareAction::Cancel, "Cancelar o chamado"),
        (ShareAction::Reopen, "Reabrir o chamado"),
        (ShareAction::Resolve, "Resolver o chamado"),
    ] {
        let report = apply_share(&source, &request(action, Some("5"), Some("motivo")))
            .await
            .unwrap();
        assert_eq!(labels(&report), [label]);
        assert!(report.is_done(), "{label}: {:?}", report.steps);
    }
}

#[tokio::test]
async fn a_request_that_makes_no_sense_is_refused_before_anything_is_sent() {
    let server = MockServer::start().await;
    let (_dir, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    let create = |text: Option<&str>| {
        request(
            ShareAction::Create {
                service_id: 1,
                campus: None,
                center: None,
                interested: None,
                extra_fields: Vec::new(),
                copy_email: false,
                then: AfterOpen::Nothing,
            },
            None,
            text,
        )
    };
    let refusal = |request: ShareRequest| {
        let source = &source;
        async move { apply_share(source, &request).await.unwrap_err().to_string() }
    };

    assert!(refusal(create(Some("  "))).await.contains("descrição"));
    let mut many = create(Some("d"));
    many.files = (0..4).map(|n| file(&format!("{n}.png"), 1)).collect();
    assert!(refusal(many).await.contains("até 3 anexos"));
    let mut bad = create(Some("d"));
    bad.files = vec![file("a.exe", 1)];
    assert!(refusal(bad).await.contains("a.exe"));

    let no_ticket = request(ShareAction::Comment, None, Some("x"));
    assert!(refusal(no_ticket).await.contains("chamado de destino"));
    let blank_ticket = request(ShareAction::Comment, Some(" "), Some("x"));
    assert!(refusal(blank_ticket).await.contains("chamado de destino"));
    let nothing = request(ShareAction::Attach, Some("5"), None);
    assert!(refusal(nothing).await.contains("nada para anexar"));
    let no_reason = request(ShareAction::Cancel, Some("5"), Some(" "));
    assert!(refusal(no_reason).await.contains("falta o texto"));
    let mut huge = request(ShareAction::Attach, Some("5"), None);
    huge.files = vec![file("grande.pdf", 10 * 1024 * 1024 + 1)];
    assert!(refusal(huge).await.contains("10 MB"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_failure_stops_the_steps_and_can_be_resumed_from_where_it_stopped() {
    let server = MockServer::start().await;
    let (_dir, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    form(&server, "/centralservicos/adicionar_anexo/", 5, "x").await;
    page(
        &server,
        "GET",
        "/centralservicos/chamado/5/",
        200,
        "<p>sem formulário</p>",
    )
    .await;

    let mut comment = request(ShareAction::Comment, Some("5"), Some("oi"));
    comment.files = vec![file("a.png", 3)];
    let report = apply_share(&source, &comment).await.unwrap();
    assert!(!report.is_done());
    assert_eq!((report.completed, report.total), (1, 2));
    assert_eq!(report.steps.len(), 2);
    assert!(report.steps[0].error.is_none());
    let error = report.steps[1].error.as_deref().unwrap();
    assert!(error.contains("no comment form"), "{error}");

    // Retrying with what is done: the attachment is not sent again.
    server.reset().await;
    ticket_page(&server, 5).await;
    comment.resume_from = report.completed;
    let retried = apply_share(&source, &comment).await.unwrap();
    assert!(retried.is_done(), "{:?}", retried.steps);
    assert_eq!(retried.steps.len(), 2);
    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| !request.url.path().contains("adicionar_anexo")));
}

#[tokio::test]
async fn opening_a_ticket_reports_what_suap_refuses() {
    let server = MockServer::start().await;
    let (_dir, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    page(
        &server,
        "GET",
        "/centralservicos/abrir_chamado/9/",
        200,
        "<p>nada</p>",
    )
    .await;
    let open = request(
        ShareAction::Create {
            service_id: 9,
            campus: Some("1".to_owned()),
            center: Some("2".to_owned()),
            interested: Some("1".to_owned()),
            extra_fields: Vec::new(),
            copy_email: false,
            then: AfterOpen::Nothing,
        },
        None,
        Some("descrição"),
    );
    let report = apply_share(&source, &open).await.unwrap();
    assert!(!report.is_done());
    assert_eq!(labels(&report), ["Abrir o chamado"]);
    let error = report.steps[0].error.as_deref().unwrap();
    assert!(error.contains("no ticket form"), "{error}");
    assert_eq!(report.ticket, None);
}

/// The body of the last form posted to the fake SUAP.
async fn last_post_body(server: &MockServer) -> String {
    let requests = server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .rfind(|request| request.method.as_str() == "POST")
        .unwrap();
    String::from_utf8_lossy(&post.body).into_owned()
}

#[tokio::test]
async fn the_attachment_description_goes_to_suap_and_defaults_to_the_file_name() {
    let server = MockServer::start().await;
    form(&server, "/centralservicos/adicionar_anexo/", 5, "x").await;
    let (_directory, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);

    let mut with = request(ShareAction::Attach, Some("5"), None);
    with.files = vec![file("foto.jpg", 10)];
    with.attachment_description = Some("Tela do erro".to_owned());
    assert!(apply_share(&source, &with).await.unwrap().is_done());
    assert!(last_post_body(&server).await.contains("Tela do erro"));

    let mut without = request(ShareAction::Attach, Some("5"), None);
    without.files = vec![file("log-do-servidor.pdf", 10)];
    assert!(apply_share(&source, &without).await.unwrap().is_done());
    assert!(last_post_body(&server)
        .await
        .contains("log-do-servidor.pdf"));
}

#[tokio::test]
async fn a_new_ticket_carries_its_extra_fields_and_can_be_assumed_and_started() {
    let server = MockServer::start().await;
    let (_dir, client) = client_for(&server).await;
    let source = SuapTicketSource::new(&client, TicketQueue::Support);
    page(
        &server,
        "GET",
        "/centralservicos/abrir_chamado/7/",
        200,
        r#"<form method="post"><textarea name="descricao"></textarea><input name="patrimonio"></form>"#,
    )
    .await;
    page(
        &server,
        "POST",
        "/centralservicos/abrir_chamado/7/",
        200,
        "Número do chamado: 31",
    )
    .await;
    page(
        &server,
        "GET",
        "/centralservicos/auto_atribuir_chamado/31/",
        200,
        "ok",
    )
    .await;
    page(
        &server,
        "GET",
        "/centralservicos/colocar_em_atendimento/31/",
        200,
        "ok",
    )
    .await;

    let create = |then: AfterOpen| {
        request(
            ShareAction::Create {
                service_id: 7,
                campus: Some("1".to_owned()),
                center: Some("2".to_owned()),
                interested: None,
                extra_fields: vec![("patrimonio".to_owned(), "12345".to_owned())],
                copy_email: true,
                then,
            },
            None,
            Some("Computador não liga"),
        )
    };

    let report = apply_share(&source, &create(AfterOpen::AssumeAndStart))
        .await
        .unwrap();
    assert_eq!(
        labels(&report),
        [
            "Abrir o chamado",
            "Assumir o chamado",
            "Iniciar o atendimento"
        ]
    );
    assert!(report.is_done(), "{:?}", report.steps);
    assert_eq!(report.ticket.as_deref(), Some("31"));
    let body = last_post_body(&server).await;
    assert!(body.contains("patrimonio=12345"), "{body}");
    assert!(body.contains("enviar_copia_email=on"), "{body}");

    let assumed = apply_share(&source, &create(AfterOpen::Assume))
        .await
        .unwrap();
    assert_eq!(labels(&assumed), ["Abrir o chamado", "Assumir o chamado"]);
    let only = apply_share(&source, &create(AfterOpen::Nothing))
        .await
        .unwrap();
    assert_eq!((only.completed, only.total), (1, 1));

    // The ticket is open even when a later step fails: the report keeps its number and the step that failed.
    let broken = MockServer::start().await;
    let (_dir2, client2) = client_for(&broken).await;
    let source2 = SuapTicketSource::new(&client2, TicketQueue::Support);
    page(
        &broken,
        "GET",
        "/centralservicos/abrir_chamado/7/",
        200,
        r#"<form method="post"><textarea name="descricao"></textarea></form>"#,
    )
    .await;
    page(
        &broken,
        "POST",
        "/centralservicos/abrir_chamado/7/",
        200,
        "Número do chamado: 32",
    )
    .await;
    let report = apply_share(&source2, &create(AfterOpen::Assume))
        .await
        .unwrap();
    assert!(!report.is_done());
    assert_eq!(report.ticket.as_deref(), Some("32"));
    assert_eq!(report.completed, 1);
    assert!(report.steps[1].error.is_some());
}
