//! Fachada do núcleo `chamados` para aplicativos (Kotlin no Android; Swift no iOS, no futuro).
//!
//! Spike M0 (`core_version`, `parse_listing`, `probe_suap`) e M2: o objeto [`Chamados`] com sessão,
//! leitura e o executor de compartilhamentos, mais `classify_shared` e `text_to_pdf`; M7: títulos
//! locais e sincronização cifrada (módulo `sync`).

uniffi::setup_scaffolding!();

mod account;
mod assisting;
mod cloud;
mod docs;
mod exporting;
mod marked;
mod sync;
mod watching;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chamados_app::{
    apply::{apply_share, ShareAction, ShareRequest},
    download::download_attachments,
    listing::{narrow, Marks},
    share::SharedFile,
};
use chamados_core::{
    parse_ticket_list, ArchiveStore, Attachment, Direction, Reclassification, RemoteTicket,
    SnoozeStore, SuapTicketSource, TicketFilter, TicketQueue, TicketSource, TitleStore,
};
use suap_core::{AppPaths, SuapClient, SuapConfig};

use crate::sync::unix_now;
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
    /// O caminho da foto de quem pediu no SUAP (para `ticket_photo`); `None` sem foto.
    pub photo: Option<String>,
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
            photo: ticket.photo,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
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

/// Quais chamados a lista mostra, quanto ao que a pessoa arquivou ou adiou neste aparelho.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Enum)]
pub enum ListView {
    /// As listas de sempre: sem os arquivados e sem os adiados que ainda não voltaram; os que já
    /// voltaram vêm primeiro.
    #[default]
    Normal,
    /// Só os arquivados.
    Archived,
    /// Só os adiados que ainda não voltaram, do que volta antes ao que volta depois.
    Snoozed,
}

impl ListView {
    fn app(self) -> chamados_app::listing::ListView {
        match self {
            Self::Normal => chamados_app::listing::ListView::Normal,
            Self::Archived => chamados_app::listing::ListView::Archived,
            Self::Snoozed => chamados_app::listing::ListView::Snoozed,
        }
    }
}

/// Filtros da listagem; tudo é opcional.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct ListQuery {
    /// Só os chamados cujo título local contém este texto (sem diferenciar maiúsculas).
    pub title: Option<String>,
    pub view: ListView,
    pub text: Option<String>,
    pub statuses: Vec<String>,
    pub all_statuses: bool,
    /// `AAAA-MM-DD`.
    pub since: Option<String>,
    /// `AAAA-MM-DD`.
    pub until: Option<String>,
    pub page: Option<u32>,
    /// A quem o chamado está atribuído, só na fila de suporte (`mim`, `outros`, `ninguem`…).
    pub assignment: Option<String>,
}

/// Quantos chamados marcados se buscam um a um, no máximo, a cada vez que a lista é lida.
const MAX_MARKED_LOOKUPS: usize = 60;
/// Quanto tempo um chamado buscado um a um vale sem ser lido de novo (ms).
const LOOKUP_TTL_MS: u64 = 10 * 60 * 1000;

/// Os ids dos chamados adiados (ainda esperando, do que volta antes ao que volta depois, sem os
/// arquivados) e dos arquivados.
fn marked_ids(marks: &Marks<'_>) -> (Vec<String>, Vec<String>) {
    let snoozed = marks
        .snooze
        .pending(marks.now)
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !marks.archive.is_archived(id))
        .collect();
    (snoozed, marks.archive.archived_ids())
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
            assignment: self.assignment,
            ..TicketFilter::default()
        }
    }
}

/// Quantos chamados há em cada situação de uma aba (para os selos das abas Ativos, Adiados e Arquivados).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ListCounts {
    pub active: u32,
    pub snoozed: u32,
    pub archived: u32,
}

/// Um campo (rótulo e valor) da página do chamado.
#[derive(Debug, uniffi::Record)]
pub struct TicketField {
    pub label: String,
    pub value: String,
}

/// O que uma entrada da linha do tempo é.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EntryKind {
    /// Um comentário público.
    Comment,
    /// Uma nota interna (só a equipe de atendimento vê).
    Note,
    /// Algo que aconteceu: mudança de situação, atribuição, mensagem automática, avaliação.
    Event,
    /// Um arquivo anexado ao chamado.
    Attachment,
}

impl From<chamados_core::EntryKind> for EntryKind {
    fn from(kind: chamados_core::EntryKind) -> Self {
        match kind {
            chamados_core::EntryKind::Comment => Self::Comment,
            chamados_core::EntryKind::Note => Self::Note,
            chamados_core::EntryKind::Event => Self::Event,
            chamados_core::EntryKind::Attachment => Self::Attachment,
        }
    }
}

/// Uma entrada da linha do tempo ou das notas internas, com quem escreveu e o tipo.
#[derive(Debug, uniffi::Record)]
pub struct ThreadItem {
    pub kind: EntryKind,
    /// A data como o SUAP mostra (`dd/mm/aaaa hh:mm:ss`).
    pub date: String,
    /// A data como `aaaa-mm-dd hh:mm:ss`, para ordenar listas juntas; vazia se o formato for outro.
    pub sort_key: String,
    pub author: Option<String>,
    /// O caminho da foto de quem escreveu no SUAP (para `ticket_photo`).
    pub photo: Option<String>,
    /// O texto sem o título ("fulano comentou:").
    pub body: String,
}

impl From<chamados_core::ThreadEntry> for ThreadItem {
    fn from(entry: chamados_core::ThreadEntry) -> Self {
        Self {
            kind: entry.kind.into(),
            date: entry.date,
            sort_key: entry.sort_key,
            author: entry.author,
            photo: entry.photo,
            body: entry.body,
        }
    }
}

/// Um anexo do chamado; `number` (a partir de 1) é o que `download_attachment` pede.
#[derive(Debug, uniffi::Record)]
pub struct AttachmentItem {
    pub number: u32,
    pub name: String,
    /// Quem anexou e quando ("Ana em 06/10/2026 19:14"), como a aba de anexos diz.
    pub by: Option<String>,
}

/// O chamado por inteiro, como a página dele mostra.
#[derive(Debug, uniffi::Record)]
pub struct TicketView {
    pub id: String,
    pub title: String,
    pub heading: Option<String>,
    pub statuses: Vec<String>,
    pub fields: Vec<TicketField>,
    /// A linha do tempo: comentários, acontecimentos e anexos.
    pub thread: Vec<ThreadItem>,
    /// As notas internas (vazia se a pessoa não as vê).
    pub notes: Vec<ThreadItem>,
    /// A página tem a aba de notas internas (só a equipe de atendimento a vê).
    pub can_see_notes: bool,
    pub attachments: Vec<AttachmentItem>,
    /// O caminho da foto do interessado no SUAP (para `ticket_photo`); `None` sem foto.
    pub photo: Option<String>,
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
    /// A descrição dos anexos (até 80 caracteres); sem ela, o SUAP mostra o nome do arquivo.
    pub attachment_description: Option<String>,
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
    /// Os chamados adiados/arquivados buscados um a um (quando a lista da fila não os traz), com a hora
    /// em que foram lidos (Unix ms): evita buscá-los de novo a cada atualização da lista.
    looked_up: Mutex<HashMap<String, (u64, RemoteTicket)>>,
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
            looked_up: Mutex::new(HashMap::new()),
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
        self.forget_user()
    }

    /// Lê uma lista do SUAP.
    pub async fn list(
        &self,
        queue: Queue,
        query: ListQuery,
    ) -> Result<Vec<TicketSummary>, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, queue.core());
        let (needle, view) = (query.title.clone(), query.view.app());
        let tickets = if view != chamados_app::listing::ListView::Normal && queue != Queue::ToClose
        {
            // Adiados e arquivados vêm dos ids marcados, de uma vez (sem paginação).
            self.marked_tickets(&source, &query, view).await?
        } else {
            source.list_filtered(&query.core()).await.map_err(failed)?
        };
        let titles = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let archive = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let snooze = SnoozeStore::open(self.paths.snooze_file()).map_err(failed)?;
        let marks = Marks {
            archive: &archive,
            snooze: &snooze,
            now: unix_now(),
        };
        let tickets = narrow(tickets, &titles, &marks, view, needle.as_deref(), None);
        Ok(tickets
            .into_iter()
            .map(|ticket| TicketSummary {
                id: ticket.id,
                status: ticket.status,
                subject: ticket.subject,
                photo: ticket.photo,
            })
            .collect())
    }

    /// Quantos chamados a aba tem em cada situação (ativos, adiados e arquivados), com os mesmos filtros
    /// de `list` (menos o título e a situação). Os adiados e arquivados contam em todas as situações do
    /// SUAP, como a lista deles; a aba "A fechar" só pagina, então conta a mesma lista nas três.
    pub async fn list_counts(
        &self,
        queue: Queue,
        query: ListQuery,
    ) -> Result<ListCounts, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, queue.core());
        let usual = source.list_filtered(&query.core()).await.map_err(failed)?;
        let titles = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let archive = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let snooze = SnoozeStore::open(self.paths.snooze_file()).map_err(failed)?;
        let marks = Marks {
            archive: &archive,
            snooze: &snooze,
            now: unix_now(),
        };
        let count = |tickets: &[RemoteTicket], view| {
            let kept = narrow(tickets.to_vec(), &titles, &marks, view, None, None);
            u32::try_from(kept.len()).unwrap_or(u32::MAX)
        };
        let (snoozed, archived) = match queue {
            // "A fechar" só pagina: a mesma lista serve às três contas.
            Queue::ToClose => (
                count(&usual, chamados_app::listing::ListView::Snoozed),
                count(&usual, chamados_app::listing::ListView::Archived),
            ),
            // As outras abas contam os ids marcados, como a lista de adiados e de arquivados os traz.
            _ => {
                let (snoozed, archived) = marked_ids(&marks);
                (snoozed.len() as u32, archived.len() as u32)
            }
        };
        Ok(ListCounts {
            active: count(&usual, chamados_app::listing::ListView::Normal),
            snoozed,
            archived,
        })
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
            photo: details.interested_photo,
            fields: details
                .fields
                .into_iter()
                .map(|(label, value)| TicketField { label, value })
                .collect(),
            can_see_notes: details.notes.is_some(),
            thread: details.thread.into_iter().map(ThreadItem::from).collect(),
            notes: details
                .notes
                .unwrap_or_default()
                .into_iter()
                .map(ThreadItem::from)
                .collect(),
            attachments: details
                .attachments
                .into_iter()
                .enumerate()
                .map(|(index, link)| AttachmentItem {
                    number: index as u32 + 1,
                    name: link.name,
                    by: link.by,
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
            attachment_description: job.attachment_description,
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
            matchers::{method, path, query_param},
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
                attachment_description: None,
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

        #[test]
        fn every_kind_of_entry_has_its_mobile_twin() {
            use chamados_core::EntryKind as Core;
            let pairs = [
                (Core::Comment, EntryKind::Comment),
                (Core::Note, EntryKind::Note),
                (Core::Event, EntryKind::Event),
                (Core::Attachment, EntryKind::Attachment),
            ];
            for (core, mobile) in pairs {
                assert_eq!(EntryKind::from(core), mobile);
            }
        }

        #[tokio::test]
        async fn the_snoozed_and_archived_lists_come_from_the_marked_ids_not_from_one_page() {
            let server = MockServer::start().await;
            let (_dir, chamados) = app(&server);
            // The support queue (usual statuses) only brings 1 and 2. Ticket 99 is somewhere else: it
            // moved on (resolved) and is read from its own page; 98 has no page; 97 is archived.
            page(
                &server,
                "GET",
                "/centralservicos/listar_chamados_suporte/",
                r#"<div class="general-box"><span class="status">Aberto</span>
                <h4><a href="/centralservicos/chamado/1/">REQ #1 <strong>Um</strong></a></h4></div>
                <div class="general-box"><span class="status">Em atendimento</span>
                <h4><a href="/centralservicos/chamado/2/">REQ #2 <strong>Dois</strong></a></h4></div>"#,
            )
            .await;
            let detail = |status: &str, heading: &str| {
                format!(
                    r#"<main id="content"><div class="title-container"><h2>Titulo</h2></div>
                    <div class="object-status"><span class="status">{status}</span></div>
                    <button class="accordion-button">{heading}</button></main>"#
                )
            };
            page(
                &server,
                "GET",
                "/centralservicos/chamado/99/",
                &detail("Resolvido", "1.2 Gestão | Assunto 99"),
            )
            .await;
            page(
                &server,
                "GET",
                "/centralservicos/chamado/97/",
                &detail("Aberto", "Sem barra"),
            )
            .await;
            let later = unix_now() + 3_600_000;
            for (id, until) in [("2", later), ("99", later + 1), ("98", later + 2)] {
                chamados.snooze(id.to_owned(), until).unwrap();
            }
            chamados.set_archived("97".to_owned(), true).unwrap();

            let view = |view| ListQuery {
                view,
                ..ListQuery::default()
            };
            // Snoozed: the listed one, the read one (status and subject from its page), in the order
            // they come back; 98 cannot be read and is left out. Archived 97 is not in this list.
            let snoozed = chamados
                .list(Queue::Support, view(ListView::Snoozed))
                .await
                .unwrap();
            let shown: Vec<_> = snoozed
                .iter()
                .map(|ticket| {
                    (
                        ticket.id.as_str(),
                        ticket.status.as_deref(),
                        ticket.subject.as_deref(),
                    )
                })
                .collect();
            assert_eq!(
                shown,
                [
                    ("2", Some("Em atendimento"), Some("Dois")),
                    ("99", Some("Resolvido"), Some("Assunto 99")),
                ]
            );
            // A read ticket is remembered for a while; an expired memory is read again.
            chamados.looked_up.lock().unwrap().get_mut("99").unwrap().0 = 0;
            let again = chamados
                .list(Queue::Support, view(ListView::Snoozed))
                .await
                .unwrap();
            assert_eq!(again.len(), 2);
            assert!(chamados.looked_up.lock().unwrap()["99"].0 > 0);
            // Everything comes at once: the next page has nothing.
            let next = ListQuery {
                page: Some(2),
                ..view(ListView::Snoozed)
            };
            assert!(chamados
                .list(Queue::Support, next)
                .await
                .unwrap()
                .is_empty());
            // A heading without a bar keeps the whole text as the subject.
            let archived = chamados
                .list(Queue::Support, view(ListView::Archived))
                .await
                .unwrap();
            assert_eq!(archived[0].subject.as_deref(), Some("Sem barra"));
            // The title search narrows them too.
            chamados
                .set_title("99".to_owned(), "Rede".to_owned())
                .unwrap();
            let by_title = ListQuery {
                title: Some("red".to_owned()),
                ..view(ListView::Snoozed)
            };
            let found = chamados.list(Queue::Support, by_title).await.unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].id, "99");

            // The counts follow the marks (3 snoozed, counting the one that cannot be read; 1 archived).
            let counts = chamados
                .list_counts(Queue::Support, ListQuery::default())
                .await
                .unwrap();
            assert_eq!((counts.active, counts.snoozed, counts.archived), (1, 3, 1));
            // Nothing marked: no list is read at all.
            let (_dir, empty) = app(&server);
            let none = empty
                .list(Queue::Support, view(ListView::Snoozed))
                .await
                .unwrap();
            assert!(none.is_empty());
        }

        #[tokio::test]
        async fn the_support_queue_can_be_limited_to_the_tickets_assigned_to_me() {
            let server = MockServer::start().await;
            let (_dir, chamados) = app(&server);
            Mock::given(method("GET"))
                .and(path("/centralservicos/listar_chamados_suporte/"))
                .and(query_param("atribuicoes", "1"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"<div class="general-box"><span class="status">Em atendimento</span>
                    <h4><a href="/centralservicos/chamado/9/">REQ #9 <strong>Meu</strong></a></h4></div>"#,
                ))
                .mount(&server)
                .await;
            let mine = ListQuery {
                assignment: Some("mim".to_owned()),
                ..ListQuery::default()
            };
            let listed = chamados.list(Queue::Support, mine.clone()).await;
            assert_eq!(listed.unwrap()[0].id, "9");
            // Without the filter the fake SUAP serves nothing; "Meus chamados" has no such filter.
            let all = chamados.list(Queue::Support, ListQuery::default()).await;
            assert!(all.is_err());
            assert!(chamados.list(Queue::Mine, mine).await.is_err());
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
                view: ListView::Archived,
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
                view: ListView::Archived,
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

            // The tab's counts: one active ticket, then one archived, then one snoozed.
            let counts = |c: ListCounts| (c.active, c.snoozed, c.archived);
            let now = chamados
                .list_counts(Queue::Mine, ListQuery::default())
                .await;
            assert_eq!(counts(now.unwrap()), (1, 0, 0));
            assert!(chamados.set_archived("7".to_owned(), true).unwrap());
            let now = chamados
                .list_counts(Queue::Mine, ListQuery::default())
                .await;
            assert_eq!(counts(now.unwrap()), (0, 0, 1));
            // "A fechar" only pages: the one list serves the three counts.
            page(
                &server,
                "GET",
                "/centralservicos/listar_chamados_a_fechar/",
                r#"<div class="general-box"><span class="status">Resolvido</span>
                <h4><a href="/centralservicos/chamado/8/">REQ #8 <strong>Fechar</strong></a></h4></div>"#,
            )
            .await;
            let to_close = chamados.list_counts(Queue::ToClose, ListQuery::default());
            assert_eq!(counts(to_close.await.unwrap()), (1, 0, 0));
            assert!(chamados.set_archived("7".to_owned(), false).unwrap());

            // A snoozed ticket is out of the usual list until it comes back, and has its own list.
            chamados
                .snooze("7".to_owned(), unix_now() + 3_600_000)
                .unwrap();
            let waiting = ListQuery {
                view: ListView::Snoozed,
                ..ListQuery::default()
            };
            let snoozed = chamados.list(Queue::Mine, waiting).await.unwrap();
            assert_eq!(snoozed[0].id, "7");
            let now = chamados
                .list_counts(Queue::Mine, ListQuery::default())
                .await;
            assert_eq!(counts(now.unwrap()), (0, 1, 0));
            let usual = chamados.list(Queue::Mine, ListQuery::default()).await;
            assert!(usual.unwrap().is_empty());
            std::fs::write(chamados.paths.snooze_file(), "{").unwrap();
            let damaged = chamados.list(Queue::Mine, ListQuery::default()).await;
            assert!(damaged.is_err());
            let damaged = chamados
                .list_counts(Queue::Mine, ListQuery::default())
                .await;
            assert!(damaged.is_err());
            std::fs::remove_file(chamados.paths.snooze_file()).unwrap();

            page(
                &server,
                "GET",
                "/centralservicos/chamado/7/",
                r#"<main id="content"><div class="title-container"><h2>Um</h2></div>
                <div class="accordion"><div class="accordion-body"><dl class="definition-list"><div class="list-item"><dt>Interessado</dt><dd><h4>Wagner</h4></dd></div></dl></div></div>
                <div data-tab="linha_tempo"><ul class="timeline"><li><div class="timeline-date">1</div>
                <div class="timeline-content"><a href="/djtools/arquivo/centralservicos/chamadoanexo/1/anexo/">a.csv</a></div></li></ul></div>
                <div data-tab="notas_internas"><ul class="timeline"><li><div class="timeline-date">2</div>
                <div class="timeline-content"><h4>Bia <small>comentou:</small></h4><p>Nota.</p></div></li></ul></div></main>"#,
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
            assert_eq!(view.attachments[0].by, None);
            assert_eq!(view.thread.len(), 1);
            assert_eq!(view.thread[0].kind, EntryKind::Event);
            assert!(view.can_see_notes);
            assert_eq!(
                (view.notes[0].kind, view.notes[0].body.as_str()),
                (EntryKind::Note, "Nota.")
            );
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
