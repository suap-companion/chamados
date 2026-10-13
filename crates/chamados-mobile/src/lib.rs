//! Fachada do núcleo `chamados` para aplicativos (Kotlin no Android; Swift no iOS, no futuro).
//!
//! Spike M0 (`core_version`, `parse_listing`, `probe_suap`) e M2: o objeto [`Chamados`] com sessão,
//! leitura e o executor de compartilhamentos, mais `classify_shared` e `text_to_pdf`; M7: títulos
//! locais e sincronização cifrada (módulo `sync`).

uniffi::setup_scaffolding!();

mod sync;
mod watching;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chamados_app::{
    apply::{apply_share, ShareAction, ShareRequest},
    download::download_attachments,
    listing::narrow,
    share::SharedFile,
};
use chamados_core::{
    parse_ticket_list, ArchiveStore, Attachment, Direction, Reclassification, SuapTicketSource,
    TicketFilter, TicketQueue, TicketSource, TitleStore,
};
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

/// Qual lista do SUAP ler.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum Queue {
    /// Fila de suporte (para atendentes).
    Support,
    /// Meus chamados ativos.
    Mine,
    /// Resolvidos que posso fechar.
    ToClose,
}

impl Queue {
    fn core(self) -> TicketQueue {
        match self {
            Self::Support => TicketQueue::Support,
            Self::Mine => TicketQueue::Mine,
            Self::ToClose => TicketQueue::ToClose,
        }
    }
}

/// Quais chamados a lista mostra, quanto ao que a pessoa arquivou neste aparelho.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Enum)]
pub enum ArchiveView {
    /// Tudo menos os arquivados (as listas de sempre).
    #[default]
    Hide,
    /// Só os arquivados.
    Only,
}

impl ArchiveView {
    fn app(self) -> chamados_app::listing::ArchiveView {
        match self {
            Self::Hide => chamados_app::listing::ArchiveView::Hide,
            Self::Only => chamados_app::listing::ArchiveView::Only,
        }
    }
}

/// Filtros da listagem; tudo é opcional.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct ListQuery {
    /// Só os chamados cujo título local contém este texto (sem diferenciar maiúsculas).
    pub title: Option<String>,
    pub archive: ArchiveView,
    pub text: Option<String>,
    pub statuses: Vec<String>,
    pub all_statuses: bool,
    /// `AAAA-MM-DD`.
    pub since: Option<String>,
    /// `AAAA-MM-DD`.
    pub until: Option<String>,
    pub page: Option<u32>,
}

impl ListQuery {
    fn core(self) -> TicketFilter {
        TicketFilter {
            text: self.text,
            statuses: self.statuses,
            all_statuses: self.all_statuses,
            since: self.since,
            until: self.until,
            page: self.page,
            ..TicketFilter::default()
        }
    }
}

/// Um campo (rótulo e valor) da página do chamado.
#[derive(Debug, uniffi::Record)]
pub struct TicketField {
    pub label: String,
    pub value: String,
}

/// Uma entrada da linha do tempo.
#[derive(Debug, uniffi::Record)]
pub struct TimelineItem {
    pub date: String,
    pub text: String,
}

/// Um anexo do chamado; `number` (a partir de 1) é o que `download_attachment` pede.
#[derive(Debug, uniffi::Record)]
pub struct AttachmentItem {
    pub number: u32,
    pub name: String,
}

/// O chamado por inteiro, como a página dele mostra.
#[derive(Debug, uniffi::Record)]
pub struct TicketView {
    pub id: String,
    pub title: String,
    pub heading: Option<String>,
    pub statuses: Vec<String>,
    pub fields: Vec<TicketField>,
    pub timeline: Vec<TimelineItem>,
    pub attachments: Vec<AttachmentItem>,
    pub url: String,
}

/// O que fazer com o que foi compartilhado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ShareKind {
    /// Abrir um chamado (pede `service_id`).
    Create,
    Attach,
    Comment,
    InternalNote,
    Resolve,
    Cancel,
    Reopen,
    Suspend,
}

/// Tudo o que o aplicativo decidiu sobre um compartilhamento.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ShareJob {
    pub kind: ShareKind,
    /// Serviço do SUAP, ao abrir um chamado.
    pub service_id: Option<u64>,
    pub campus: Option<String>,
    pub center: Option<String>,
    /// Interessado (id de vínculo do SUAP), ao abrir um chamado; alguns serviços o exigem.
    pub interested: Option<String>,
    /// O chamado de destino (todas as ações, menos abrir).
    pub ticket: Option<String>,
    /// Descrição, comentário, nota, motivo ou o conteúdo do PDF.
    pub text: Option<String>,
    /// Título do PDF gerado do texto (ao anexar).
    pub title: Option<String>,
    /// Caminhos dos arquivos já copiados para a pasta do aplicativo.
    pub file_paths: Vec<String>,
    /// `completed` de uma tentativa anterior, para refazer só o que falhou.
    pub resume_from: u32,
}

/// O resultado de um passo do compartilhamento.
#[derive(Debug, uniffi::Record)]
pub struct ShareStepResult {
    pub label: String,
    /// Vazio quando o passo foi feito.
    pub error: Option<String>,
}

/// O que aconteceu com o compartilhamento.
#[derive(Debug, uniffi::Record)]
pub struct ShareOutcome {
    pub steps: Vec<ShareStepResult>,
    /// Passos feitos desde o começo: use como `resume_from` para tentar de novo.
    pub completed: u32,
    pub total: u32,
    /// O chamado de destino (o número do novo, ao abrir).
    pub ticket: Option<String>,
    pub done: bool,
}

/// Um arquivo recebido por "Compartilhar", só o que é preciso para decidir se o SUAP o aceita.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SharedFileInfo {
    pub name: String,
    pub size: u64,
}

/// Um arquivo que o SUAP recusa, e o motivo (em português, pronto para mostrar).
#[derive(Debug, uniffi::Record)]
pub struct RejectedFile {
    pub index: u32,
    pub name: String,
    pub reason: String,
}

/// Os arquivos separados entre aceitos (posições na lista enviada) e recusados.
#[derive(Debug, uniffi::Record)]
pub struct SharedClassification {
    pub accepted: Vec<u32>,
    pub rejected: Vec<RejectedFile>,
}

/// Separa o que o SUAP aceita como anexo do que recusa. `limit`: máximo de anexos aceitos (3 ao
/// abrir um chamado; sem limite ao anexar a um que já existe).
#[uniffi::export]
pub fn classify_shared(files: Vec<SharedFileInfo>, limit: Option<u32>) -> SharedClassification {
    let files: Vec<SharedFile> = files
        .into_iter()
        .map(|file| SharedFile {
            name: file.name,
            size: file.size,
        })
        .collect();
    let result = chamados_app::share::classify_shared(&files, limit.map(|limit| limit as usize));
    SharedClassification {
        accepted: result.accepted.into_iter().map(|n| n as u32).collect(),
        rejected: result
            .rejected
            .into_iter()
            .map(|file| RejectedFile {
                index: file.index as u32,
                name: file.name,
                reason: file.reason,
            })
            .collect(),
    }
}

/// O texto como um PDF (o SUAP não aceita `.txt`), com título opcional.
#[uniffi::export]
pub fn text_to_pdf(title: Option<String>, text: String) -> Vec<u8> {
    chamados_app::pdf::text_to_pdf(title.as_deref(), &text)
}

/// A sessão de um perfil do SUAP no aparelho, com tudo o que o aplicativo faz nele.
#[derive(uniffi::Object)]
pub struct Chamados {
    client: SuapClient,
    paths: AppPaths,
    base_url: Url,
}

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// Abre (ou cria) o armazenamento em `data_dir` para o SUAP em `base_url`.
    #[uniffi::constructor]
    pub fn new(data_dir: String, base_url: String) -> Result<Arc<Self>, ChamadosError> {
        let dir = PathBuf::from(data_dir);
        let paths = AppPaths::from_dirs(dir.join("config"), dir.join("data"));
        paths.ensure_dirs().map_err(failed)?;
        let config = SuapConfig {
            base_url: Url::parse(&base_url).map_err(failed)?,
            ..SuapConfig::default()
        };
        let client = SuapClient::open(&paths, &config).map_err(failed)?;
        Ok(Arc::new(Self {
            client,
            paths,
            base_url: config.base_url,
        }))
    }

    /// Recebe os cookies da WebView de login (o cabeçalho `Cookie`); devolve quantos guardou. O
    /// aplicativo nunca vê nem guarda a senha.
    pub async fn import_session_cookies(&self, header: String) -> Result<u32, ChamadosError> {
        let kept = self.client.import_cookie_header(&header).await;
        Ok(kept.map_err(failed)? as u32)
    }

    /// A sessão ainda vale no SUAP?
    pub async fn is_authenticated(&self) -> Result<bool, ChamadosError> {
        self.client.is_authenticated().await.map_err(failed)
    }

    /// Esquece a sessão (apaga os cookies guardados).
    pub async fn logout(&self) -> Result<(), ChamadosError> {
        self.client.import_cookie_header("").await.map_err(failed)?;
        Ok(())
    }

    /// Lê uma lista do SUAP.
    pub async fn list(
        &self,
        queue: Queue,
        query: ListQuery,
    ) -> Result<Vec<TicketSummary>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, queue.core());
        let (needle, view) = (query.title.clone(), query.archive.app());
        let tickets = source.list_filtered(&query.core()).await.map_err(failed)?;
        let titles = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let archive = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let tickets = narrow(tickets, &titles, &archive, view, needle.as_deref(), None);
        Ok(tickets
            .into_iter()
            .map(|ticket| TicketSummary {
                id: ticket.id,
                status: ticket.status,
                subject: ticket.subject,
            })
            .collect())
    }

    /// O chamado por inteiro.
    pub async fn show(&self, id: String) -> Result<TicketView, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let details = source.get_ticket(&id).await.map_err(failed)?;
        Ok(TicketView {
            id: details.id,
            title: details.title,
            heading: details.heading,
            statuses: details.statuses,
            fields: details
                .fields
                .into_iter()
                .map(|(label, value)| TicketField { label, value })
                .collect(),
            timeline: details
                .timeline
                .into_iter()
                .map(|entry| TimelineItem {
                    date: entry.date,
                    text: entry.text,
                })
                .collect(),
            attachments: details
                .attachments
                .into_iter()
                .enumerate()
                .map(|(index, link)| AttachmentItem {
                    number: index as u32 + 1,
                    name: link.name,
                })
                .collect(),
            url: details.details_url,
        })
    }

    /// Baixa o anexo `number` do chamado para `folder` e devolve o caminho do arquivo salvo.
    pub async fn download_attachment(
        &self,
        id: String,
        number: u32,
        folder: String,
    ) -> Result<String, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let details = source.get_ticket(&id).await.map_err(failed)?;
        let wanted = [number as usize];
        let saved = download_attachments(&source, &details, &wanted, Path::new(&folder), false)
            .await
            .map_err(failed)?;
        Ok(saved[0].path.display().to_string())
    }

    /// Executa o que foi decidido na tela de compartilhamento. Erro só quando o pedido não faz
    /// sentido (nada foi enviado); as falhas do SUAP vêm no resultado, para repetir só o resto.
    pub async fn apply_share(&self, job: ShareJob) -> Result<ShareOutcome, ChamadosError> {
        let mut files = Vec::new();
        for file_path in &job.file_paths {
            let path = Path::new(file_path);
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let bytes = std::fs::read(path).map_err(|error| failed(format!("{name}: {error}")))?;
            files.push(Attachment {
                file_name: name.into_owned(),
                bytes,
            });
        }
        let action = match job.kind {
            ShareKind::Create => ShareAction::Create {
                service_id: job
                    .service_id
                    .ok_or_else(|| failed("falta o serviço do novo chamado"))?,
                campus: job.campus,
                center: job.center,
                interested: job.interested,
            },
            ShareKind::Attach => ShareAction::Attach,
            ShareKind::Comment => ShareAction::Comment,
            ShareKind::InternalNote => ShareAction::InternalNote,
            ShareKind::Resolve => ShareAction::Resolve,
            ShareKind::Cancel => ShareAction::Cancel,
            ShareKind::Reopen => ShareAction::Reopen,
            ShareKind::Suspend => ShareAction::Suspend,
        };
        let request = ShareRequest {
            action,
            ticket: job.ticket,
            text: job.text,
            title: job.title,
            files,
            resume_from: job.resume_from as usize,
        };
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let report = apply_share(&source, &request).await.map_err(failed)?;
        Ok(ShareOutcome {
            done: report.is_done(),
            completed: report.completed as u32,
            total: report.total as u32,
            ticket: report.ticket,
            steps: report
                .steps
                .into_iter()
                .map(|step| ShareStepResult {
                    label: step.label,
                    error: step.error,
                })
                .collect(),
        })
    }

    /// Passa o chamado para outro atendente do mesmo grupo (nome ou matrícula); devolve quem recebeu.
    pub async fn assign(&self, id: String, to: String) -> Result<String, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.assign_ticket(&id, &to).await.map_err(failed)
    }

    /// Escala o chamado para o grupo acima, com a nota interna (e, se quiser, quem o recebe).
    pub async fn escalate(
        &self,
        id: String,
        text: String,
        to: Option<String>,
    ) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let moved = source.move_ticket(&id, Direction::Escalate, &text, to.as_deref());
        moved.await.map_err(failed)
    }

    /// Devolve o chamado ao grupo abaixo, com a nota interna (e, se quiser, quem o recebe).
    pub async fn return_ticket(
        &self,
        id: String,
        text: String,
        to: Option<String>,
    ) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let moved = source.move_ticket(&id, Direction::Return, &text, to.as_deref());
        moved.await.map_err(failed)
    }

    /// Muda o serviço, o campus e/ou o centro do chamado (o que ficar vazio não muda), com a
    /// justificativa.
    pub async fn reclassify(
        &self,
        id: String,
        service: Option<String>,
        campus: Option<String>,
        center: Option<String>,
        text: String,
    ) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let change = Reclassification {
            service,
            campus,
            center,
        };
        let done = source.reclassify_ticket(&id, &change, &text);
        done.await.map_err(failed)
    }

    /// Põe tags (pelo número ou nome) no chamado; devolve os nomes.
    pub async fn add_tags(
        &self,
        id: String,
        tags: Vec<String>,
    ) -> Result<Vec<String>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.add_tags(&id, &tags).await.map_err(failed)
    }

    /// Tira tags do chamado; devolve os nomes.
    pub async fn remove_tags(
        &self,
        id: String,
        tags: Vec<String>,
    ) -> Result<Vec<String>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.remove_tags(&id, &tags).await.map_err(failed)
    }

    /// Inclui outros interessados (por matrícula ou nome); devolve os nomes.
    pub async fn add_interested(
        &self,
        id: String,
        people: Vec<String>,
    ) -> Result<Vec<String>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.add_interested(&id, &people).await.map_err(failed)
    }

    /// Tira outros interessados do chamado; devolve os nomes.
    pub async fn remove_interested(
        &self,
        id: String,
        people: Vec<String>,
    ) -> Result<Vec<String>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.remove_interested(&id, &people).await.map_err(failed)
    }

    /// Assume o chamado ("assumir").
    pub async fn assume(&self, id: String) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.assume_ticket(&id).await.map_err(failed)
    }

    /// Coloca o chamado em atendimento (antes é preciso tê-lo assumido).
    pub async fn start_service(&self, id: String) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        source.start_service(&id).await.map_err(failed)
    }

    /// Fecha um chamado resolvido, com a avaliação (nota de 1 a 5) e o comentário de quem o abriu.
    pub async fn close_ticket(
        &self,
        id: String,
        rating: Option<u8>,
        comment: Option<String>,
    ) -> Result<(), ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let result = source.close_ticket(&id, rating, comment.as_deref()).await;
        result.map_err(failed)
    }
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

    mod facade {
        use super::*;
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        async fn page(server: &MockServer, verb: &str, at: &str, body: &str) {
            Mock::given(method(verb))
                .and(path(at.to_owned()))
                .respond_with(ResponseTemplate::new(200).set_body_string(body.to_owned()))
                .mount(server)
                .await;
        }

        fn app(server: &MockServer) -> (tempfile::TempDir, Arc<Chamados>) {
            let dir = tempfile::tempdir().unwrap();
            let data = dir.path().to_str().unwrap().to_owned();
            let chamados = Chamados::new(data, format!("{}/", server.uri())).unwrap();
            (dir, chamados)
        }

        fn job(kind: ShareKind) -> ShareJob {
            ShareJob {
                kind,
                service_id: None,
                campus: None,
                center: None,
                interested: None,
                ticket: Some("5".to_owned()),
                text: Some("texto".to_owned()),
                title: None,
                file_paths: Vec::new(),
                resume_from: 0,
            }
        }

        #[test]
        fn shared_files_are_classified_and_text_becomes_a_pdf() {
            let file = |name: &str| SharedFileInfo {
                name: name.to_owned(),
                size: 10,
            };
            let files = vec![file("foto.png"), file("nota.txt"), file("b.png")];
            let all = classify_shared(files.clone(), None);
            assert_eq!(all.accepted, [0, 2]);
            assert_eq!(all.rejected.len(), 1);
            assert_eq!(all.rejected[0].index, 1);
            assert_eq!(all.rejected[0].name, "nota.txt");
            assert!(all.rejected[0].reason.contains("não aceito"));
            assert_eq!(classify_shared(files, Some(1)).rejected.len(), 2);

            let pdf = text_to_pdf(Some("Título".to_owned()), "corpo".to_owned());
            assert!(pdf.starts_with(b"%PDF-"));
        }

        #[tokio::test]
        async fn the_session_comes_from_the_browser_cookies() {
            let server = MockServer::start().await;
            let (_dir, chamados) = app(&server);
            page(&server, "GET", "/", "").await;
            let header = "sessionid=abc; csrftoken=def".to_owned();
            assert_eq!(chamados.import_session_cookies(header).await.unwrap(), 2);
            assert!(chamados.is_authenticated().await.unwrap());
            chamados.logout().await.unwrap();

            assert!(Chamados::new("x".to_owned(), "isto não é url".to_owned()).is_err());
            let blocked = tempfile::tempdir().unwrap().keep().join("arquivo");
            std::fs::write(&blocked, "x").unwrap();
            let under_a_file = blocked.join("dentro").to_str().unwrap().to_owned();
            assert!(Chamados::new(under_a_file, "http://127.0.0.1:1/".to_owned()).is_err());

            let dir = tempfile::tempdir().unwrap();
            let data = dir.path().to_str().unwrap().to_owned();
            let offline = Chamados::new(data, "http://127.0.0.1:1/".to_owned()).unwrap();
            assert!(offline.is_authenticated().await.is_err());
        }

        #[tokio::test]
        async fn tickets_are_listed_shown_and_their_attachments_downloaded() {
            let server = MockServer::start().await;
            let (dir, chamados) = app(&server);
            page(
                &server,
                "GET",
                "/centralservicos/meus_chamados/",
                r#"<div class="general-box"><span class="status">Aberto</span>
                <h4><a href="/centralservicos/chamado/7/">REQ #7 <strong>Assunto</strong></a></h4></div>"#,
            )
            .await;
            let query = ListQuery {
                text: Some("rede".to_owned()),
                ..ListQuery::default()
            };
            // Only "Meus chamados" is served by this fake SUAP.
            for queue in [Queue::Support, Queue::ToClose] {
                assert!(chamados.list(queue, query.clone()).await.is_err());
            }
            let listed = chamados
                .list(Queue::Mine, ListQuery::default())
                .await
                .unwrap();
            assert_eq!(listed[0].id, "7");

            // An archived ticket leaves the usual list and shows up in the archive view.
            assert!(chamados.set_archived("7".to_owned(), true).unwrap());
            let only = ListQuery {
                archive: ArchiveView::Only,
                ..ListQuery::default()
            };
            let usual = chamados.list(Queue::Mine, ListQuery::default()).await;
            assert!(usual.unwrap().is_empty());
            let archived = chamados.list(Queue::Mine, only).await.unwrap();
            assert_eq!(archived[0].id, "7");
            chamados
                .set_title("7".to_owned(), "Rede".to_owned())
                .unwrap();
            let by_title = |title: &str| ListQuery {
                archive: ArchiveView::Only,
                title: Some(title.to_owned()),
                ..ListQuery::default()
            };
            assert_eq!(
                chamados
                    .list(Queue::Mine, by_title("red"))
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert!(chamados
                .list(Queue::Mine, by_title("moodle"))
                .await
                .unwrap()
                .is_empty());
            assert!(chamados.set_archived("7".to_owned(), false).unwrap());

            page(
                &server,
                "GET",
                "/centralservicos/chamado/7/",
                r#"<main id="content"><div class="title-container"><h2>Um</h2></div>
                <div class="accordion"><div class="accordion-body"><dl class="definition-list"><div class="list-item"><dt>Interessado</dt><dd><h4>Wagner</h4></dd></div></dl></div></div>
                <div data-tab="linha_tempo"><ul class="timeline"><li><div class="timeline-date">1</div>
                <div class="timeline-content"><a href="/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/">a.csv</a></div></li></ul></div></main>"#,
            )
            .await;
            page(
                &server,
                "GET",
                "/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/",
                "a,b",
            )
            .await;
            let view = chamados.show("7".to_owned()).await.unwrap();
            assert_eq!(view.id, "7");
            assert_eq!(view.attachments.len(), 1);
            assert_eq!(view.attachments[0].number, 1);
            assert_eq!(view.attachments[0].name, "a.csv");
            assert!(view.url.contains("/chamado/7/"));
            assert!(!view.title.is_empty());
            assert_eq!(view.fields[0].label, "Interessado");
            assert_eq!(view.fields[0].value, "Wagner");
            assert!(chamados.show("8".to_owned()).await.is_err());

            let folder = dir.path().join("baixados").to_str().unwrap().to_owned();
            let saved = chamados
                .download_attachment("7".to_owned(), 1, folder.clone())
                .await
                .unwrap();
            assert_eq!(std::fs::read(&saved).unwrap(), b"a,b");
            let second = chamados.download_attachment("7".to_owned(), 2, folder);
            assert!(second.await.is_err());
        }

        #[tokio::test]
        async fn a_share_is_applied_with_the_files_the_app_copied() {
            let server = MockServer::start().await;
            let (dir, chamados) = app(&server);
            let comment_form = r#"<form action="/centralservicos/adicionar_comentario/5/" method="post">
                <input type="hidden" name="csrfmiddlewaretoken" value="t"><textarea name="texto"></textarea></form>"#;
            page(&server, "GET", "/centralservicos/chamado/5/", comment_form).await;
            let comment_post = "/centralservicos/adicionar_comentario/5/";
            page(&server, "POST", comment_post, "ok").await;
            let attach_form = r#"<form action="/centralservicos/adicionar_anexo/5/" method="post">
                <input type="hidden" name="csrfmiddlewaretoken" value="t"><input name="descricao"><input type="file" name="anexo"></form>"#;
            let attach_path = "/centralservicos/adicionar_anexo/5/";
            page(&server, "GET", attach_path, attach_form).await;
            page(&server, "POST", attach_path, "ok").await;

            let photo = dir.path().join("foto.png");
            std::fs::write(&photo, b"png").unwrap();
            let mut with_file = job(ShareKind::Comment);
            with_file.file_paths = vec![photo.to_str().unwrap().to_owned()];
            let outcome = chamados.apply_share(with_file).await.unwrap();
            assert!(outcome.done, "{:?}", outcome.steps);
            assert_eq!((outcome.completed, outcome.total), (2, 2));
            assert_eq!(outcome.ticket.as_deref(), Some("5"));
            assert_eq!(outcome.steps[0].label, "Anexar foto.png");
            assert!(outcome.steps[1].error.is_none());

            // Every kind maps to its action; the ones this SUAP has no form for report the failure.
            for kind in [
                ShareKind::Attach,
                ShareKind::InternalNote,
                ShareKind::Resolve,
                ShareKind::Cancel,
                ShareKind::Reopen,
                ShareKind::Suspend,
            ] {
                let outcome = chamados.apply_share(job(kind)).await.unwrap();
                assert!(outcome.total > 0, "{kind:?}");
            }
            let missing_service = chamados.apply_share(job(ShareKind::Create)).await;
            assert!(missing_service.unwrap_err().to_string().contains("serviço"));
            let mut create = job(ShareKind::Create);
            create.service_id = Some(3);
            create.campus = Some("1".to_owned());
            create.center = Some("2".to_owned());
            create.interested = Some("1".to_owned());
            let refused = chamados.apply_share(create).await.unwrap();
            assert!(!refused.done);
            assert!(refused.steps[0].error.is_some());

            let mut gone = job(ShareKind::Attach);
            gone.file_paths = vec![dir.path().join("sumiu.png").to_str().unwrap().to_owned()];
            let error = chamados.apply_share(gone).await.unwrap_err().to_string();
            assert!(error.contains("sumiu.png"), "{error}");
            let mut empty = job(ShareKind::Comment);
            empty.text = None;
            assert!(chamados.apply_share(empty).await.is_err());
        }

        #[tokio::test]
        async fn a_ticket_is_assumed_started_and_closed_with_a_rating() {
            let server = MockServer::start().await;
            let (_dir, chamados) = app(&server);
            assert!(chamados.assume("5".to_owned()).await.is_err());
            assert!(chamados.start_service("5".to_owned()).await.is_err());

            page(
                &server,
                "GET",
                "/centralservicos/auto_atribuir_chamado/5/",
                "",
            )
            .await;
            page(
                &server,
                "GET",
                "/centralservicos/colocar_em_atendimento/5/",
                "",
            )
            .await;
            chamados.assume("5".to_owned()).await.unwrap();
            chamados.start_service("5".to_owned()).await.unwrap();

            let form = r#"<form action="/centralservicos/fechar_chamado/5/" method="post">
                <input type="hidden" name="csrfmiddlewaretoken" value="t"><input name="nota_avaliacao">
                <textarea name="comentario"></textarea></form>"#;
            page(&server, "GET", "/centralservicos/fechar_chamado/5/", form).await;
            page(&server, "POST", "/centralservicos/fechar_chamado/5/", "ok").await;
            let comment = Some("Atendimento ótimo".to_owned());
            let closing = chamados.close_ticket("5".to_owned(), Some(5), comment);
            closing.await.unwrap();
            assert!(chamados
                .close_ticket("5".to_owned(), Some(9), None)
                .await
                .is_err());
        }

        #[tokio::test]
        async fn the_management_actions_reach_suap_and_report_what_it_refuses() {
            let server = MockServer::start().await;
            let (_dir, chamados) = app(&server);
            let id = || "5".to_owned();
            let names = || vec!["Alguém".to_owned()];

            // Without the forms, SUAP refuses each action and the reason comes back as an error.
            assert!(chamados.assign(id(), "Ana".to_owned()).await.is_err());
            assert!(chamados
                .escalate(id(), "nota".to_owned(), None)
                .await
                .is_err());
            assert!(chamados
                .return_ticket(id(), "nota".to_owned(), None)
                .await
                .is_err());
            let (service, text) = (Some("2".to_owned()), "porque sim".to_owned());
            assert!(chamados
                .reclassify(id(), service, None, None, text)
                .await
                .is_err());
            assert!(chamados.add_tags(id(), names()).await.is_err());
            assert!(chamados.remove_tags(id(), names()).await.is_err());
            assert!(chamados.add_interested(id(), names()).await.is_err());
            assert!(chamados.remove_interested(id(), names()).await.is_err());

            // With the form, the attendant is found by name and the ticket is handed over.
            let form = r#"<form action="/centralservicos/atribuir_chamado/5/" method="post">
                <input type="hidden" name="csrfmiddlewaretoken" value="t">
                <select name="atribuido_para"><option value="">---</option>
                <option value="7">Ana Souza (111)</option><option value="8">Bruno Lima (222)</option></select></form>"#;
            page(&server, "GET", "/centralservicos/atribuir_chamado/5/", form).await;
            page(
                &server,
                "POST",
                "/centralservicos/atribuir_chamado/5/",
                "ok",
            )
            .await;
            assert_eq!(
                chamados.assign(id(), "bruno".to_owned()).await.unwrap(),
                "Bruno Lima (222)"
            );

            // And a ticket goes up to the group above with an internal note.
            let escalate = r#"<form action="/centralservicos/escalar_atendimento_chamado/5/" method="post">
                <input type="hidden" name="csrfmiddlewaretoken" value="t"><textarea name="texto"></textarea></form>"#;
            let path = "/centralservicos/escalar_atendimento_chamado/5/";
            page(&server, "GET", path, escalate).await;
            page(&server, "POST", path, "ok").await;
            chamados
                .escalate(id(), "Precisa do nível 2".to_owned(), None)
                .await
                .unwrap();
        }
    }
}
