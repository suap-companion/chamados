//! Fachada do núcleo `chamados` para aplicativos (Kotlin no Android; Swift no iOS, no futuro).
//!
//! Spike M0: duas funções só para provar a cadeia (Rust → NDK → UniFFI → Kotlin → emulador).

uniffi::setup_scaffolding!();

use std::path::PathBuf;

use chamados_core::{parse_ticket_list, TicketFilter};
use suap_core::{AppPaths, SuapClient, SuapConfig};
use url::Url;

/// Erros da fachada, como o aplicativo os vê.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ChamadosError {
    // O campo não se chama `message`: em Kotlin isso colidiria com `Throwable.message`.
    #[error("{detail}")]
    Failed { detail: String },
}

fn failed(error: impl std::fmt::Display) -> ChamadosError {
    ChamadosError::Failed {
        detail: error.to_string(),
    }
}

/// Um chamado da lista.
#[derive(Debug, uniffi::Record)]
pub struct TicketSummary {
    pub id: String,
    pub status: Option<String>,
    pub subject: Option<String>,
}

/// Versão do núcleo (prova de que a biblioteca carregou).
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Lê uma página de listagem do SUAP (HTML) e devolve os chamados, sem rede: prova que o parser
/// (`chamados-core`) roda no aparelho.
#[uniffi::export]
pub fn parse_listing(html: String, base_url: String) -> Result<Vec<TicketSummary>, ChamadosError> {
    let base = Url::parse(&base_url).map_err(failed)?;
    Ok(parse_ticket_list(&html, &base)
        .into_iter()
        .map(|ticket| TicketSummary {
            id: ticket.id,
            status: ticket.status,
            subject: ticket.subject,
        })
        .collect())
}

/// O caminho que a listagem do SUAP usa para um filtro vazio (prova do `chamados-core`).
#[uniffi::export]
pub fn default_listing_path() -> Result<String, ChamadosError> {
    TicketFilter::default()
        .path(chamados_core::TicketQueue::Mine, 1)
        .map_err(failed)
}

/// Pergunta ao SUAP se a sessão (vazia) é válida: prova que HTTP + TLS + tokio rodam no aparelho.
#[uniffi::export(async_runtime = "tokio")]
pub async fn probe_suap(data_dir: String, base_url: String) -> Result<bool, ChamadosError> {
    let dir = PathBuf::from(data_dir);
    let paths = AppPaths::from_dirs(dir.join("config"), dir.join("data"));
    paths.ensure_dirs().map_err(failed)?;
    let config = SuapConfig {
        base_url: Url::parse(&base_url).map_err(failed)?,
        ..SuapConfig::default()
    };
    let client = SuapClient::open(&paths, &config).map_err(failed)?;
    client.is_authenticated().await.map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_runs_through_the_facade() {
        assert_eq!(core_version(), env!("CARGO_PKG_VERSION"));
        let html = r#"<div class="general-box"><span class="status">Aberto</span>
            <h4><a href="/centralservicos/chamado/7/">REQ #7 <strong>Assunto</strong></a></h4></div>"#;
        let tickets = parse_listing(html.to_owned(), "https://suap.example/".to_owned()).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].id, "7");
        assert!(parse_listing(String::new(), "isto não é url".to_owned()).is_err());
        assert!(default_listing_path().unwrap().contains("meus_chamados"));
        let error = failed("deu errado");
        assert_eq!(error.to_string(), "deu errado");
    }

    #[tokio::test]
    async fn probing_suap_tells_a_valid_session_from_an_invalid_one() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        let (dir, base) = (tempfile::tempdir().unwrap(), format!("{}/", server.uri()));
        let data = dir.path().to_str().unwrap().to_owned();

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        assert!(probe_suap(data.clone(), base.clone()).await.unwrap());

        server.reset().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/accounts/login/"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/accounts/login/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        assert!(!probe_suap(data.clone(), base).await.unwrap());

        // Mistakes come back as errors the app can show, never as panics.
        assert!(probe_suap(data.clone(), "isto não é url".to_owned())
            .await
            .is_err());
        let blocked = dir.path().join("arquivo");
        std::fs::write(&blocked, "x").unwrap();
        let under_a_file = blocked.join("dentro").to_str().unwrap().to_owned();
        assert!(probe_suap(under_a_file, "http://127.0.0.1:1/".to_owned())
            .await
            .is_err());
        assert!(probe_suap(data, "http://127.0.0.1:1/".to_owned())
            .await
            .is_err());
    }
}
