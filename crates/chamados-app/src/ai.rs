//! IA nos comandos (`--AI`): escolhe o provedor do perfil, monta o pedido a partir do chamado e fala
//! com a API do provedor (Gemini, Anthropic ou compatível com a da OpenAI).
//!
//! As regras do texto (privacidade, orçamento, limpeza) são as do `chamados-assist`, as mesmas do
//! aplicativo. Aqui ficam só o contexto vindo do `TicketDetails`, a configuração do perfil e a rede.
//!
//! A chave da API **nunca** é gravada: vem de uma variável de ambiente (a padrão do provedor ou a que
//! o perfil indicar). Nenhuma mensagem de erro leva texto de chamado.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use chamados_assist as assist;
use chamados_core::{EntryKind, TicketDetails};
use serde_json::{json, Value};
use suap_core::DEFAULT_PROFILE;

use crate::AppError;

pub use assist::{DraftKind, Prompt};

/// O arquivo com o provedor de IA do perfil `default`; os outros perfis usam `ia-<perfil>.json`.
pub const SETTINGS_FILE: &str = "ia.json";

/// Onde fica a configuração de IA de um perfil (a pasta de configuração é a mesma para todos).
pub fn settings_file(dir: &Path, profile: &str) -> PathBuf {
    if profile == DEFAULT_PROFILE {
        return dir.join(SETTINGS_FILE);
    }
    dir.join(format!("ia-{profile}.json"))
}

const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TOKENS: u32 = 1024;
const ERROR_CHARS: usize = 200;

/// Os provedores de IA aceitos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gemini,
    Anthropic,
    /// A API de `chat/completions` da OpenAI, que muitos servidores também falam (Ollama, OpenRouter…).
    OpenAi,
}

impl Provider {
    /// Os nomes aceitos na linha de comando.
    pub const NAMES: [&'static str; 3] = ["gemini", "anthropic", "openai"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "gemini" => Some(Self::Gemini),
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
        }
    }

    fn default_model(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-2.5-flash",
            Self::Anthropic => "claude-haiku-5-5",
            Self::OpenAi => "gpt-4o-mini",
        }
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAi => "https://api.openai.com/v1",
        }
    }

    /// A variável de ambiente que guarda a chave, se o perfil não indicar outra.
    pub fn default_key_env(self) -> &'static str {
        match self {
            Self::Gemini => "GEMINI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
        }
    }
}

/// O provedor de IA de um perfil. Não guarda a chave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSettings {
    pub provider: Provider,
    /// O modelo; sem ele, o padrão do provedor.
    pub model: Option<String>,
    /// O endereço da API; sem ele, o do provedor.
    pub base_url: Option<String>,
    /// O nome da variável de ambiente com a chave; sem ele, a padrão do provedor.
    pub key_env: Option<String>,
}

fn text_field(value: &Value, name: &str) -> Option<String> {
    value.get(name)?.as_str().map(str::to_owned)
}

impl AiSettings {
    pub fn model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or_else(|| self.provider.default_model())
    }

    pub fn base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or_else(|| self.provider.default_base_url())
    }

    pub fn key_env(&self) -> &str {
        self.key_env
            .as_deref()
            .unwrap_or_else(|| self.provider.default_key_env())
    }

    /// Lê a configuração de IA do perfil; `None` se o perfil não tem.
    pub fn load(file: &Path) -> Result<Option<Self>, AppError> {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let invalid = || {
            let name = file.display();
            AppError::Message(format!("{name} inválido: refaça com `profile ia`"))
        };
        let value: Value = serde_json::from_str(&text).map_err(|_| invalid())?;
        let provider = text_field(&value, "provider")
            .and_then(|name| Provider::parse(&name))
            .ok_or_else(invalid)?;
        Ok(Some(Self {
            provider,
            model: text_field(&value, "model"),
            base_url: text_field(&value, "base_url"),
            key_env: text_field(&value, "key_env"),
        }))
    }

    pub fn save(&self, file: &Path) -> Result<(), AppError> {
        let value = json!({
            "provider": self.provider.name(),
            "model": self.model,
            "base_url": self.base_url,
            "key_env": self.key_env,
        });
        let dir = file.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        fs::write(file, format!("{value:#}\n"))?;
        Ok(())
    }

    /// Apaga a configuração de IA do perfil; diz se havia.
    pub fn remove(file: &Path) -> Result<bool, AppError> {
        match fs::remove_file(file) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// A chave da API, lida da variável de ambiente por `lookup`.
    pub fn api_key(&self, lookup: &dyn Fn(&str) -> Option<String>) -> Result<String, AppError> {
        let name = self.key_env();
        match lookup(name).filter(|key| !key.trim().is_empty()) {
            Some(key) => Ok(key.trim().to_owned()),
            None => Err(AppError::Message(format!(
                "chave da IA ({}) não encontrada: defina a variável de ambiente {name}",
                self.provider.name()
            ))),
        }
    }
}

/// O que o modelo pode saber do chamado: tipo, situação, descrição e o histórico (comentários,
/// acontecimentos, anexos e notas) do mais antigo ao mais novo. Quem decide se as notas entram é o
/// `chamados-assist`, pelo tipo do texto.
fn context(details: &TicketDetails, local_title: Option<&str>) -> assist::TicketContext {
    let kind = |kind: EntryKind| match kind {
        EntryKind::Comment => assist::ContextKind::Comment,
        EntryKind::Note => assist::ContextKind::Note,
        EntryKind::Event => assist::ContextKind::Event,
        EntryKind::Attachment => assist::ContextKind::Attachment,
    };
    // A linha do tempo do SUAP vem do mais novo ao mais antigo; as notas, juntas, ordenadas pela data.
    let notes = details.notes.iter().flatten();
    let mut items: Vec<_> = details.thread.iter().rev().chain(notes).collect();
    items.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    let description = details
        .fields
        .iter()
        .find(|(label, _)| label.trim().trim_end_matches(':').to_lowercase() == "descrição")
        .map(|(_, value)| value.clone());
    assist::TicketContext {
        id: details.id.clone(),
        title: local_title.map(str::to_owned),
        kind: details
            .heading
            .clone()
            .unwrap_or_else(|| details.title.clone()),
        statuses: details.statuses.clone(),
        description,
        entries: items
            .into_iter()
            .map(|item| assist::ContextEntry {
                kind: kind(item.kind),
                author: item.author.clone(),
                date: item.date.clone(),
                body: item.body.clone(),
            })
            .collect(),
    }
}

/// O pedido ao modelo para escrever `kind` sobre o chamado; `hint` é a instrução da pessoa.
pub fn draft_prompt(
    kind: DraftKind,
    details: &TicketDetails,
    local_title: Option<&str>,
    hint: Option<&str>,
) -> Prompt {
    let request = assist::DraftRequest {
        kind,
        ticket: context(details, local_title),
        hint: hint.map(str::to_owned),
        previous: None,
        refine: None,
        references: vec![],
    };
    assist::build_prompt(&request, assist::DEFAULT_BUDGET_CHARS)
}

/// O pedido para descrever um chamado que ainda não existe: só a instrução da pessoa.
pub fn describe_prompt(hint: Option<&str>) -> Prompt {
    let ticket = assist::TicketContext {
        id: String::new(),
        title: None,
        kind: String::new(),
        statuses: vec![],
        description: None,
        entries: vec![],
    };
    let request = assist::DraftRequest {
        kind: DraftKind::Describe,
        ticket,
        hint: hint.map(str::to_owned),
        previous: None,
        refine: None,
        references: vec![],
    };
    assist::build_prompt(&request, assist::DEFAULT_BUDGET_CHARS)
}

/// Limpa a resposta do modelo para o texto que vai ao SUAP; erro se não sobrou nada.
pub fn clean(kind: DraftKind, raw: &str) -> Result<String, AppError> {
    let text = assist::clean_output(kind, raw);
    if text.is_empty() {
        return Err(AppError::Message("a IA não devolveu texto".to_owned()));
    }
    Ok(text)
}

/// A requisição HTTP de cada provedor: endereço, cabeçalhos e corpo.
struct Call {
    url: String,
    headers: Vec<(&'static str, String)>,
    body: String,
}

fn call(settings: &AiSettings, key: &str, prompt: &str) -> Call {
    let base = settings.base_url().trim_end_matches('/');
    let model = settings.model();
    match settings.provider {
        Provider::Gemini => Call {
            url: format!("{base}/models/{model}:generateContent"),
            headers: vec![("x-goog-api-key", key.to_owned())],
            body: json!({"contents": [{"parts": [{"text": prompt}]}]}).to_string(),
        },
        Provider::Anthropic => Call {
            url: format!("{base}/v1/messages"),
            headers: vec![
                ("x-api-key", key.to_owned()),
                ("anthropic-version", "2023-06-01".to_owned()),
            ],
            body: json!({
                "model": model,
                "max_tokens": MAX_TOKENS,
                "messages": [{"role": "user", "content": prompt}],
            })
            .to_string(),
        },
        Provider::OpenAi => Call {
            url: format!("{base}/chat/completions"),
            headers: vec![("authorization", format!("Bearer {key}"))],
            body: json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
            })
            .to_string(),
        },
    }
}

/// O texto da resposta de cada provedor.
fn answer(provider: Provider, value: &Value) -> Option<String> {
    match provider {
        Provider::Gemini => {
            let parts = value.pointer("/candidates/0/content/parts")?.as_array()?;
            Some(
                parts
                    .iter()
                    .filter_map(|part| part.get("text")?.as_str())
                    .collect(),
            )
        }
        Provider::Anthropic => {
            let blocks = value.get("content")?.as_array()?;
            Some(
                blocks
                    .iter()
                    .filter_map(|block| block.get("text")?.as_str())
                    .collect(),
            )
        }
        Provider::OpenAi => {
            let text = value.pointer("/choices/0/message/content")?.as_str()?;
            Some(text.to_owned())
        }
    }
}

/// A explicação que o provedor deu a uma falha (`error.message`), curta e sem quebras de linha.
fn failure_reason(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let reason = value.pointer("/error/message").and_then(Value::as_str);
    let reason: String = reason
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let short: String = reason.chars().take(ERROR_CHARS).collect();
    if short.is_empty() {
        short
    } else {
        format!(": {short}")
    }
}

/// Pede o texto ao modelo e devolve a resposta crua (use [`clean`] antes de enviá-la ao SUAP).
pub async fn generate(settings: &AiSettings, key: &str, prompt: &str) -> Result<String, AppError> {
    let name = settings.provider.name();
    let call = call(settings, key, prompt);
    let client = reqwest::Client::builder().timeout(TIMEOUT).build();
    let client = client.expect("cliente HTTP com TLS embutido");
    let mut request = client
        .post(&call.url)
        .header("content-type", "application/json")
        .body(call.body);
    for (header, value) in call.headers {
        request = request.header(header, value);
    }
    let response = request.send().await;
    let response = response.map_err(|error| {
        let error = error.without_url();
        AppError::Message(format!("falha ao falar com a IA ({name}): {error}"))
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let reason = failure_reason(&body);
        return Err(AppError::Message(format!(
            "a IA ({name}) respondeu HTTP {}{reason}",
            status.as_u16()
        )));
    }
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    answer(settings.provider, &value)
        .ok_or_else(|| AppError::Message(format!("resposta da IA ({name}) em formato inesperado")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chamados_core::ThreadEntry;
    use wiremock::{
        matchers::{body_partial_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn thread(kind: EntryKind, sort_key: &str, author: Option<&str>, body: &str) -> ThreadEntry {
        ThreadEntry {
            kind,
            date: format!("data {sort_key}"),
            sort_key: sort_key.to_owned(),
            author: author.map(str::to_owned),
            photo: None,
            body: body.to_owned(),
        }
    }

    fn details() -> TicketDetails {
        TicketDetails {
            id: "5".to_owned(),
            title: "Sem tipo".to_owned(),
            heading: None,
            statuses: vec!["Em atendimento".to_owned()],
            fields: vec![
                ("Campus:".to_owned(), "Natal".to_owned()),
                ("Descrição:".to_owned(), "Moodle fora do ar".to_owned()),
            ],
            timeline: vec![],
            // O SUAP lista do mais novo ao mais antigo.
            thread: vec![
                thread(EntryKind::Comment, "2026-10-03", Some("Ana"), "Voltou?"),
                thread(
                    EntryKind::Attachment,
                    "2026-10-03",
                    Some("Ana"),
                    "print.png",
                ),
                thread(EntryKind::Event, "2026-10-01", None, "Chamado aberto"),
            ],
            notes: Some(vec![thread(
                EntryKind::Note,
                "2026-10-02",
                Some("Bia"),
                "SEGREDO interno",
            )]),
            attachments: vec![],
            interested_photo: None,
            details_url: String::new(),
        }
    }

    fn settings(provider: Provider, server: &MockServer) -> AiSettings {
        AiSettings {
            provider,
            model: Some("modelo-x".to_owned()),
            base_url: Some(format!("{}/", server.uri())),
            key_env: None,
        }
    }

    #[test]
    fn providers_have_names_and_defaults() {
        for name in Provider::NAMES {
            let provider = Provider::parse(name).unwrap();
            assert_eq!(provider.name(), name);
            assert!(provider.default_model().len() > 3);
            assert!(provider.default_base_url().starts_with("https://"));
            assert!(provider.default_key_env().ends_with("_API_KEY"));
        }
        assert_eq!(Provider::parse("outro"), None);
    }

    #[test]
    fn settings_use_the_provider_defaults_until_overridden() {
        let plain = AiSettings {
            provider: Provider::Gemini,
            model: None,
            base_url: None,
            key_env: None,
        };
        assert_eq!(plain.model(), "gemini-2.5-flash");
        assert!(plain.base_url().contains("generativelanguage"));
        assert_eq!(plain.key_env(), "GEMINI_API_KEY");
        let custom = AiSettings {
            model: Some("m".to_owned()),
            base_url: Some("http://x".to_owned()),
            key_env: Some("MINHA".to_owned()),
            ..plain
        };
        assert_eq!(
            (custom.model(), custom.base_url(), custom.key_env()),
            ("m", "http://x", "MINHA")
        );
    }

    fn saved() -> AiSettings {
        AiSettings {
            provider: Provider::OpenAi,
            model: Some("llama".to_owned()),
            base_url: None,
            key_env: Some("CHAVE".to_owned()),
        }
    }

    #[test]
    fn settings_are_saved_loaded_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let folder = settings_file(&dir.path().join("config"), DEFAULT_PROFILE);
        assert_eq!(AiSettings::load(&folder).unwrap(), None);
        assert!(!AiSettings::remove(&folder).unwrap());
        let saved = saved();
        saved.save(&folder).unwrap();
        assert_eq!(AiSettings::load(&folder).unwrap(), Some(saved));
        assert!(AiSettings::remove(&folder).unwrap());
        assert_eq!(AiSettings::load(&folder).unwrap(), None);
    }

    #[test]
    fn broken_settings_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let file = settings_file(dir.path(), "outro");
        assert_eq!(file.file_name().unwrap(), "ia-outro.json");
        for text in ["{", r#"{"provider":"nenhum"}"#, "{}"] {
            fs::write(&file, text).unwrap();
            let error = AiSettings::load(&file).unwrap_err().to_string();
            assert!(error.contains("ia-outro.json inválido"), "{error}");
        }
        // Um diretório no lugar do arquivo: nem ler, nem gravar, nem apagar.
        let busy = tempfile::tempdir().unwrap();
        let file = settings_file(busy.path(), DEFAULT_PROFILE);
        fs::create_dir(&file).unwrap();
        assert!(AiSettings::load(&file).is_err());
        assert!(AiSettings::remove(&file).is_err());
        assert!(saved().save(&file).is_err());
    }

    #[test]
    fn the_key_comes_from_the_environment_variable_of_the_profile() {
        let mut ai = AiSettings {
            provider: Provider::Anthropic,
            model: None,
            base_url: None,
            key_env: None,
        };
        let lookup = |name: &str| match name {
            "ANTHROPIC_API_KEY" => Some("  sk-1\n".to_owned()),
            "VAZIA" => Some("  ".to_owned()),
            _ => None,
        };
        assert_eq!(ai.api_key(&lookup).unwrap(), "sk-1");
        for name in ["VAZIA", "AUSENTE"] {
            ai.key_env = Some(name.to_owned());
            let error = ai.api_key(&lookup).unwrap_err().to_string();
            assert_eq!(
                error,
                format!(
                    "chave da IA (anthropic) não encontrada: defina a variável de ambiente {name}"
                )
            );
        }
    }

    #[test]
    fn the_prompt_has_the_history_in_order_and_keeps_notes_internal() {
        let internal = draft_prompt(
            DraftKind::InternalNote,
            &details(),
            Some("Moodle"),
            Some("reiniciei"),
        );
        let text = &internal.text;
        assert!(text.contains("Chamado #5") && text.contains("Tipo: Sem tipo"));
        assert!(text.contains("Título: Moodle") && text.contains("Descrição: Moodle fora do ar"));
        assert!(text.contains("O que a pessoa quer dizer: reiniciei"));
        let (open, note, comment) = (
            text.find("Chamado aberto").unwrap(),
            text.find("SEGREDO interno").unwrap(),
            text.find("Voltou?").unwrap(),
        );
        assert!(open < note && note < comment);
        let public = draft_prompt(DraftKind::Comment, &details(), None, None).text;
        assert!(!public.contains("SEGREDO") && !public.contains("Título:"));
        // Sem a aba de notas (quem não é da equipe), nada de nota.
        let no_notes = TicketDetails {
            notes: None,
            heading: Some("Moodle".to_owned()),
            ..details()
        };
        let text = draft_prompt(DraftKind::InternalNote, &no_notes, None, None).text;
        assert!(!text.contains("SEGREDO") && text.contains("Tipo: Moodle"));
    }

    #[test]
    fn a_new_ticket_is_described_from_the_instruction_alone() {
        let text = describe_prompt(Some("wifi caiu na sala 3")).text;
        assert!(text.contains("descrição de um chamado novo"));
        assert!(text.contains("O que a pessoa quer dizer: wifi caiu na sala 3"));
        assert!(!text.contains("Chamado #"));
        assert!(!describe_prompt(None).text.contains("quer dizer"));
    }

    #[test]
    fn the_answer_is_cleaned_and_must_not_be_empty() {
        assert_eq!(
            clean(DraftKind::Title, "\"Moodle fora\"\n").unwrap(),
            "Moodle fora"
        );
        let error = clean(DraftKind::Comment, " \n```\n")
            .unwrap_err()
            .to_string();
        assert_eq!(error, "a IA não devolveu texto");
    }

    #[tokio::test]
    async fn gemini_is_called_with_the_key_in_a_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/models/modelo-x:generateContent"))
            .and(header("x-goog-api-key", "k1"))
            .and(body_partial_json(
                json!({"contents": [{"parts": [{"text": "oi"}]}]}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{"content": {"parts": [{"text": "Olá, "}, {"text": "mundo"}]}}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let text = generate(&settings(Provider::Gemini, &server), "k1", "oi").await;
        assert_eq!(text.unwrap(), "Olá, mundo");
    }

    #[tokio::test]
    async fn anthropic_is_called_with_its_headers_and_messages() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "k2"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(body_partial_json(json!({
                "model": "modelo-x",
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": "oi"}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "content": [{"type": "text", "text": "Resposta"}, {"type": "tool_use"}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let text = generate(&settings(Provider::Anthropic, &server), "k2", "oi").await;
        assert_eq!(text.unwrap(), "Resposta");
    }

    #[tokio::test]
    async fn openai_compatible_servers_get_a_bearer_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer k3"))
            .and(body_partial_json(json!({"model": "modelo-x"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "Feito"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let text = generate(&settings(Provider::OpenAi, &server), "k3", "oi").await;
        assert_eq!(text.unwrap(), "Feito");
    }

    #[tokio::test]
    async fn failures_are_explained_without_the_prompt() {
        let server = MockServer::start().await;
        Mock::given(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "error": {"message": "cota\nexcedida"}
            })))
            .mount(&server)
            .await;
        Mock::given(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("<html>"))
            .mount(&server)
            .await;
        Mock::given(path("/models/modelo-x:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"candidates": []})))
            .mount(&server)
            .await;
        let secret = "TEXTO SECRETO DO CHAMADO";
        let ask = |provider| {
            let ai = settings(provider, &server);
            async move { generate(&ai, "k", secret).await.unwrap_err().to_string() }
        };
        assert_eq!(
            ask(Provider::OpenAi).await,
            "a IA (openai) respondeu HTTP 429: cota excedida"
        );
        assert_eq!(
            ask(Provider::Anthropic).await,
            "a IA (anthropic) respondeu HTTP 500"
        );
        assert_eq!(
            ask(Provider::Gemini).await,
            "resposta da IA (gemini) em formato inesperado"
        );
        // Nada responde: a falha de rede também não leva o texto.
        let down = AiSettings {
            base_url: Some("http://127.0.0.1:1".to_owned()),
            ..settings(Provider::OpenAi, &server)
        };
        let error = generate(&down, "k", secret).await.unwrap_err().to_string();
        assert!(
            error.starts_with("falha ao falar com a IA (openai): "),
            "{error}"
        );
        assert!(!error.contains(secret));
    }
}
