//! A IA na nuvem com a chave da própria pessoa (#144): o app guarda a chave (no cofre do Android) e manda o
//! pedido; o Rust fala com o provedor (Gemini, Anthropic ou uma API compatível com a da OpenAI), com o mesmo
//! código do CLI (`chamados_app::ai`). Nada disso roda sem a pessoa ligar, ler o aviso e informar a chave; a chave
//! nunca é gravada aqui nem entra em mensagem de erro.

use chamados_app::ai::{generate, AiSettings, Provider};
use url::Url;

use crate::{failed, ChamadosError};

/// Um provedor que o app oferece, com os padrões que a tela mostra como dica.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CloudProviderInfo {
    /// O nome interno (`gemini`, `anthropic` ou `openai`), o que `cloud_generate` pede.
    pub name: String,
    /// O nome para a pessoa.
    pub label: String,
    pub default_model: String,
    pub default_base_url: String,
    /// Só a API compatível com a da OpenAI aceita outro endereço (Ollama, OpenRouter…).
    pub custom_url: bool,
}

/// Os provedores aceitos, com o modelo e o endereço padrão de cada um.
#[uniffi::export]
pub fn cloud_providers() -> Vec<CloudProviderInfo> {
    let labels = [
        "Gemini (Google)",
        "Claude (Anthropic)",
        "OpenAI e compatíveis",
    ];
    Provider::NAMES
        .iter()
        .zip(labels)
        .map(|(name, label)| {
            let defaults = settings(name, None, None);
            CloudProviderInfo {
                name: (*name).to_owned(),
                label: label.to_owned(),
                default_model: defaults.model().to_owned(),
                default_base_url: defaults.base_url().to_owned(),
                custom_url: *name == "openai",
            }
        })
        .collect()
}

/// A configuração de um provedor conhecido (`name` já validado), sem a chave.
fn settings(name: &str, model: Option<String>, base_url: Option<String>) -> AiSettings {
    AiSettings {
        provider: Provider::parse(name).unwrap_or(Provider::Gemini),
        model,
        base_url,
        key_env: None,
    }
}

/// Um texto vazio (ou só espaços) vale como "não informado".
fn filled(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// O endereço da API tem que ser `https`; `http` só vale para a própria máquina (um Ollama local), porque a
/// chave viaja nele.
fn check_base_url(address: &str) -> Result<(), ChamadosError> {
    let url =
        Url::parse(address).map_err(|_| failed("o endereço da IA não é um endereço válido"))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() == "https" || (url.scheme() == "http" && local) {
        return Ok(());
    }
    Err(failed(
        "o endereço da IA precisa ser https (http só vale para a própria máquina)",
    ))
}

/// Pede o texto ao provedor e devolve a resposta crua (o app a limpa com `assist_clean`). A chave só vive
/// nesta chamada. Falha com uma mensagem em português, sem a chave nem o texto do chamado.
#[uniffi::export(async_runtime = "tokio")]
pub async fn cloud_generate(
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    key: String,
    prompt: String,
) -> Result<String, ChamadosError> {
    if Provider::parse(&provider).is_none() {
        return Err(failed(format!("provedor de IA desconhecido: {provider}")));
    }
    let (model, base_url) = (filled(model), filled(base_url));
    if let Some(address) = &base_url {
        check_base_url(address)?;
    }
    let key = key.trim();
    if key.is_empty() {
        return Err(failed("falta a chave da IA"));
    }
    let settings = settings(&provider, model, base_url);
    generate(&settings, key, &prompt).await.map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const OPENAI_ANSWER: &str = r#"{"choices":[{"message":{"content":"Olá! Estou analisando."}}]}"#;

    async fn ask(server: &MockServer, key: &str) -> Result<String, ChamadosError> {
        let address = Some(server.uri());
        cloud_generate(
            "openai".to_owned(),
            None,
            address,
            key.to_owned(),
            "escreva".to_owned(),
        )
        .await
    }

    #[test]
    fn the_providers_come_with_their_defaults_and_only_openai_takes_another_address() {
        let providers = cloud_providers();
        let names: Vec<&str> = providers.iter().map(|info| info.name.as_str()).collect();
        assert_eq!(names, ["gemini", "anthropic", "openai"]);
        assert!(providers.iter().all(|info| !info.default_model.is_empty()));
        assert!(providers[0]
            .default_base_url
            .starts_with("https://generativelanguage"));
        assert_eq!(
            providers
                .iter()
                .map(|info| info.custom_url)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(providers[1].label, "Claude (Anthropic)");
    }

    #[test]
    fn the_address_must_be_https_or_the_own_machine() {
        for good in [
            "https://api.exemplo.com/v1",
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080",
            "http://[::1]:1/x",
        ] {
            assert!(check_base_url(good).is_ok(), "{good}");
        }
        for bad in [
            "http://api.exemplo.com/v1",
            "ftp://localhost",
            "não é um endereço",
            "https:",
        ] {
            assert!(check_base_url(bad).is_err(), "{bad}");
        }
        assert_eq!(filled(Some("  ".to_owned())), None);
        assert_eq!(filled(Some(" gpt ".to_owned())), Some("gpt".to_owned()));
        assert_eq!(filled(None), None);
    }

    #[tokio::test]
    async fn the_text_comes_from_the_provider_with_the_key_in_the_header_only() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer chave-secreta"))
            .respond_with(ResponseTemplate::new(200).set_body_string(OPENAI_ANSWER))
            .mount(&server)
            .await;
        assert_eq!(
            ask(&server, "  chave-secreta ").await.unwrap(),
            "Olá! Estou analisando."
        );
    }

    #[tokio::test]
    async fn bad_requests_are_refused_without_leaking_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(r#"{"error":{"message":"chave inválida"}}"#),
            )
            .mount(&server)
            .await;
        let denied = ask(&server, "chave-secreta").await.unwrap_err().to_string();
        assert!(
            denied.contains("HTTP 401") && !denied.contains("chave-secreta"),
            "{denied}"
        );

        let unknown = cloud_generate(
            "outro".to_owned(),
            None,
            None,
            "k".to_owned(),
            "p".to_owned(),
        )
        .await;
        assert!(unknown.unwrap_err().to_string().contains("desconhecido"));
        let insecure = cloud_generate(
            "openai".to_owned(),
            None,
            Some("http://api.exemplo.com".to_owned()),
            "k".to_owned(),
            "p".to_owned(),
        )
        .await;
        assert!(insecure.unwrap_err().to_string().contains("https"));
        let no_key = cloud_generate(
            "gemini".to_owned(),
            None,
            None,
            "  ".to_owned(),
            "p".to_owned(),
        )
        .await;
        assert!(no_key.unwrap_err().to_string().contains("falta a chave"));
    }
}
