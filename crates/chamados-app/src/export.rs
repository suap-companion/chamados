//! Exportar um chamado como ZIP: texto legível, dados estruturados e os anexos, para guardar o
//! histórico ou entregar o conteúdo a uma análise (por exemplo, por IA).
//!
//! Nada de sessão, cookies ou dados de outros chamados entra no arquivo: só o que a página do chamado
//! mostra, o título local e a marca de arquivado.

use chamados_core::{SuapTicketSource, TicketDetails, TicketSource};
use serde_json::json;

use crate::{
    files::{safe_file_name, unique_name, with_stored_extension},
    zip::{iso_utc, ZipWriter},
    AppError,
};

/// O que se sabe do chamado fora da página do SUAP.
#[derive(Debug, Clone, Copy)]
pub struct ExportInfo<'a> {
    /// O título local, se o chamado tiver um.
    pub local_title: Option<&'a str>,
    pub archived: bool,
    /// Quando o ZIP é feito (segundos desde 1970).
    pub exported_at: u64,
}

/// O nome do arquivo: `chamado-<número>.zip`.
pub fn export_file_name(details: &TicketDetails) -> String {
    format!("chamado-{}.zip", safe_file_name(&details.id, "0"))
}

/// Um anexo já baixado e o nome que ele tem dentro do ZIP.
struct Stored {
    original: String,
    path: String,
    bytes: Vec<u8>,
}

fn markdown(details: &TicketDetails, info: &ExportInfo<'_>, files: &[Stored]) -> String {
    let heading = details.heading.as_deref().unwrap_or(&details.title);
    let mut text = format!("# Chamado #{} — {heading}\n\n", details.id);
    if let Some(title) = info.local_title {
        text.push_str(&format!("- **Título local:** {title}\n"));
    }
    if !details.statuses.is_empty() {
        text.push_str(&format!(
            "- **Situação:** {}\n",
            details.statuses.join(" · ")
        ));
    }
    if info.archived {
        text.push_str("- **Arquivado:** sim (marca local, não existe no SUAP)\n");
    }
    text.push_str(&format!("- **Endereço:** {}\n", details.details_url));
    text.push_str(&format!(
        "- **Exportado em:** {}\n",
        iso_utc(info.exported_at)
    ));
    if !details.fields.is_empty() {
        text.push_str("\n## Dados\n\n");
        for (label, value) in &details.fields {
            text.push_str(&format!("### {label}\n\n{value}\n\n"));
        }
    }
    if !details.timeline.is_empty() {
        text.push_str("## Linha do tempo\n\n");
        for entry in &details.timeline {
            text.push_str(&format!("### {}\n\n{}\n\n", entry.date, entry.text));
        }
    }
    if !files.is_empty() {
        text.push_str("## Anexos\n\n");
        for file in files {
            text.push_str(&format!("- [{}](<{}>)\n", file.original, file.path));
        }
    }
    text
}

fn data(details: &TicketDetails, info: &ExportInfo<'_>, files: &[Stored]) -> String {
    let fields: Vec<_> = (details.fields.iter())
        .map(|(label, value)| json!({"label": label, "value": value}))
        .collect();
    let timeline: Vec<_> = (details.timeline.iter())
        .map(|entry| json!({"date": entry.date, "text": entry.text}))
        .collect();
    let attachments: Vec<_> = (details.attachments.iter().enumerate())
        .map(|(index, link)| {
            let file = files.get(index).map(|file| file.path.clone());
            json!({"number": index + 1, "name": link.name, "file": file})
        })
        .collect();
    let document = json!({
        "id": details.id,
        "heading": details.heading,
        "title": details.title,
        "local_title": info.local_title,
        "archived": info.archived,
        "statuses": details.statuses,
        "url": details.details_url,
        "exported_at": iso_utc(info.exported_at),
        "fields": fields,
        "timeline": timeline,
        "attachments": attachments,
    });
    serde_json::to_string_pretty(&document).expect("the document is serializable")
}

/// Baixa os anexos (quando `attachments`) e monta o ZIP do chamado: `chamado.md`, `chamado.json` e,
/// em `anexos/`, os arquivos com nomes seguros. Se algum anexo falhar, nada é devolvido.
pub async fn export_ticket(
    source: &SuapTicketSource<'_>,
    details: &TicketDetails,
    info: &ExportInfo<'_>,
    attachments: bool,
) -> Result<Vec<u8>, AppError> {
    let mut files: Vec<Stored> = Vec::new();
    if attachments {
        let mut names: Vec<String> = Vec::new();
        for (index, link) in details.attachments.iter().enumerate() {
            let file = source.download_attachment(link).await?;
            let safe = safe_file_name(&link.name, &format!("anexo-{}", index + 1));
            let name = unique_name(&with_stored_extension(&safe, &file.stored_name), &names);
            names.push(name.clone());
            files.push(Stored {
                original: link.name.clone(),
                path: format!("anexos/{name}"),
                bytes: file.bytes,
            });
        }
    }
    let mut zip = ZipWriter::new(info.exported_at);
    zip.add("chamado.md", markdown(details, info, &files).as_bytes())?;
    zip.add("chamado.json", data(details, info, &files).as_bytes())?;
    for file in &files {
        zip.add(&file.path, &file.bytes)?;
    }
    zip.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::tests::read;
    use chamados_core::TicketQueue;
    use suap_core::{AppPaths, SuapClient, SuapConfig};
    use url::Url;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const PAGE: &str = r#"<main id="content"><div class="title-container"><h2>Moodle fora do ar</h2></div>
        <div class="object-status"><span class="status">Em atendimento</span></div>
        <div class="accordion"><div class="accordion-body"><dl class="definition-list">
          <div class="list-item"><dt>Interessado</dt><dd><h4>Ana</h4></dd></div>
          <div class="list-item"><dt>Descrição</dt><dd>O site "não" abre\ne dá erro 500.</dd></div>
        </dl></div></div>
        <div data-tab="linha_tempo"><ul class="timeline">
          <li><div class="timeline-date">13/10/2026 10:00</div><div class="timeline-content">Chamado aberto.</div></li>
          <li><div class="timeline-date">13/10/2026 11:00</div><div class="timeline-content">
            <a href="/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/">log</a>
            <a href="/djtools/arquivo/centralservicos/chamadoanexo/2/anexo/">log</a>
            <a href="/djtools/arquivo/centralservicos/chamadoanexo/3/..%2F..%2Fx.csv">../../CON</a></div></li>
        </ul></div></main>"#;

    async fn mount(server: &MockServer, at: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(at.to_owned()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_owned()))
            .mount(server)
            .await;
    }

    fn info(title: Option<&str>, archived: bool) -> ExportInfo<'_> {
        ExportInfo {
            local_title: title,
            archived,
            exported_at: 1_791_900_000,
        }
    }

    #[tokio::test]
    async fn a_ticket_becomes_a_zip_with_text_data_and_safe_attachment_names() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_dirs(dir.path().join("c"), dir.path().join("d"));
        let config = SuapConfig {
            base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
            ..SuapConfig::default()
        };
        let client = SuapClient::open(&paths, &config).unwrap();
        let source = SuapTicketSource::new(&client, TicketQueue::Support);
        mount(&server, "/centralservicos/chamado/7/", PAGE).await;
        let details = source.get_ticket("7").await.unwrap();
        assert_eq!(export_file_name(&details), "chamado-7.zip");

        // Without the attachments no file is fetched: only the text and the data.
        let light = export_ticket(&source, &details, &info(None, false), false)
            .await
            .unwrap();
        let names: Vec<String> = read(&light).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["chamado.md", "chamado.json"]);

        // With them, a failing download stops the export and nothing is returned.
        let broken = export_ticket(&source, &details, &info(None, false), true).await;
        assert!(matches!(broken, Err(AppError::Ticket(_))));

        for (number, content) in [(1, "um"), (2, "dois")] {
            let at = format!("/djtools/arquivo/centralservicos/chamadoanexo/{number}/anexo/");
            mount(&server, &at, content).await;
        }
        mount(
            &server,
            "/djtools/arquivo/centralservicos/chamadoanexo/3/..%2F..%2Fx.csv",
            "a,b",
        )
        .await;
        let zip = export_ticket(&source, &details, &info(Some("Moodle 5.3"), true), true)
            .await
            .unwrap();
        let files = read(&zip);
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names.len(), 5, "{names:?}");
        assert_eq!(&names[..2], ["chamado.md", "chamado.json"]);
        // Same name twice gets a suffix; a dangerous name stays inside anexos/.
        assert!(names.iter().all(|name| !name.contains("..")
            && (name.starts_with("anexos/") || name.starts_with("chamado."))));
        assert!(names.contains(&"anexos/log"));
        assert!(
            names.iter().any(|name| name.contains("log (2)")),
            "{names:?}"
        );

        let text = String::from_utf8(files[0].1.clone()).unwrap();
        assert!(text.starts_with("# Chamado #7 — "), "{text}");
        for wanted in [
            "**Título local:** Moodle 5.3",
            "**Situação:** Em atendimento",
            "**Arquivado:** sim",
            "2026-10-13T14:00:00Z",
            "### Interessado",
            "erro 500",
            "## Linha do tempo",
            "Chamado aberto.",
            "## Anexos",
            "(<anexos/",
        ] {
            assert!(text.contains(wanted), "falta {wanted:?} em:\n{text}");
        }
        let json: serde_json::Value = serde_json::from_slice(&files[1].1).unwrap();
        assert_eq!(json["id"], "7");
        assert_eq!(json["local_title"], "Moodle 5.3");
        assert_eq!(json["archived"], true);
        assert_eq!(json["attachments"].as_array().unwrap().len(), 3);
        assert_eq!(json["attachments"][1]["number"], 2);
        assert!(json["attachments"][0]["file"]
            .as_str()
            .unwrap()
            .starts_with("anexos/"));
        assert_eq!(json["timeline"].as_array().unwrap().len(), 2);
        // No session data ever goes into the archive.
        let everything = format!("{text}{}", String::from_utf8_lossy(&files[1].1));
        assert!(!everything.to_lowercase().contains("cookie") && !everything.contains("sessionid"));

        // A bare page (no fields, status, timeline or attachments) still exports, without empty sections.
        mount(
            &server,
            "/centralservicos/chamado/8/",
            r#"<main id="content"><div class="title-container"><h2>Vazio</h2></div></main>"#,
        )
        .await;
        let bare = source.get_ticket("8").await.unwrap();
        let zip = export_ticket(&source, &bare, &info(None, false), true)
            .await
            .unwrap();
        let text = String::from_utf8(read(&zip).remove(0).1).unwrap();
        assert!(
            !text.contains("## ") && !text.contains("Arquivado") && !text.contains("Título local"),
            "{text}"
        );
    }
}
