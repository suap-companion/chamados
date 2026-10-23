//! Executar o que foi decidido na tela "O que fazer com isto?" do compartilhamento: os passos, na
//! ordem certa, com o resultado de cada um e a possibilidade de retomar só o que falhou.

use chamados_core::{
    validate_attachments, Attachment, Message, NewTicket, Resolution, SuapTicketSource,
    TicketSource, MAX_ATTACHMENTS,
};

use crate::{pdf::text_to_pdf, AppError};

/// O que fazer com o que foi compartilhado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareAction {
    /// Abre um chamado: o texto é a descrição e os arquivos (até 3) são os anexos.
    Create {
        service_id: u64,
        campus: Option<String>,
        center: Option<String>,
        /// Interessado (id de vínculo do SUAP); alguns serviços o exigem.
        interested: Option<String>,
    },
    /// Anexa ao chamado os arquivos e, se houver texto, o texto como PDF.
    Attach,
    Comment,
    InternalNote,
    Resolve,
    Cancel,
    Reopen,
    Suspend,
}

/// O pedido completo: tudo o que o app decidiu, para executar de uma vez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRequest {
    pub action: ShareAction,
    /// O chamado de destino (todas as ações, menos criar).
    pub ticket: Option<String>,
    /// O texto: descrição, comentário, nota, motivo ou conteúdo do PDF.
    pub text: Option<String>,
    /// Título do PDF gerado a partir do texto (só em `Attach`).
    pub title: Option<String>,
    pub files: Vec<Attachment>,
    /// A descrição dos anexos (até 80 caracteres; sem ela, o nome do arquivo), como `attach --descricao`.
    pub attachment_description: Option<String>,
    /// Quantos passos do começo já foram feitos numa tentativa anterior (`completed` do relatório).
    pub resume_from: usize,
}

/// O resultado de um passo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareStep {
    pub label: String,
    /// `None` quando o passo foi feito.
    pub error: Option<String>,
}

/// O que aconteceu: os passos tentados (o primeiro que falhou é o último da lista).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareReport {
    pub steps: Vec<ShareStep>,
    /// Quantos passos do começo estão feitos: é o `resume_from` para tentar de novo o que falhou.
    pub completed: usize,
    /// O chamado de destino (o número do chamado criado, em `Create`).
    pub ticket: Option<String>,
    /// Quantos passos o pedido tem no total.
    pub total: usize,
}

impl ShareReport {
    pub fn is_done(&self) -> bool {
        self.completed == self.total
    }
}

enum Step {
    Open(Box<NewTicket>),
    Attach(Attachment, Option<String>),
    Send(Message, String),
    Suspend(String),
    Cancel(String),
    Reopen(String),
    Resolve(String),
}

impl Step {
    fn label(&self) -> String {
        match self {
            Self::Open(_) => "Abrir o chamado".to_owned(),
            Self::Attach(file, _) => format!("Anexar {}", file.file_name),
            Self::Send(Message::Comment, _) => "Adicionar o comentário".to_owned(),
            Self::Send(Message::InternalNote, _) => "Adicionar a nota interna".to_owned(),
            Self::Suspend(_) => "Suspender o chamado".to_owned(),
            Self::Cancel(_) => "Cancelar o chamado".to_owned(),
            Self::Reopen(_) => "Reabrir o chamado".to_owned(),
            Self::Resolve(_) => "Resolver o chamado".to_owned(),
        }
    }
}

fn refuse(text: &str) -> AppError {
    AppError::Message(text.to_owned())
}

/// Monta os passos do pedido, recusando antes de qualquer requisição o que não pode dar certo.
fn plan(request: &ShareRequest) -> Result<Vec<Step>, AppError> {
    let text = request
        .text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let ticket = request
        .ticket
        .as_deref()
        .map(str::trim)
        .filter(|ticket| !ticket.is_empty());
    let mut files = request.files.clone();

    if let ShareAction::Create {
        service_id,
        campus,
        center,
        interested,
    } = &request.action
    {
        let description = text.ok_or_else(|| refuse("falta a descrição do chamado"))?;
        if files.len() > MAX_ATTACHMENTS {
            return Err(AppError::Message(format!(
                "um chamado novo aceita até {MAX_ATTACHMENTS} anexos"
            )));
        }
        validate_attachments(&files)?;
        let new = NewTicket {
            service_id: *service_id,
            description: description.to_owned(),
            campus: campus.clone(),
            center: center.clone(),
            interested: interested.clone(),
            attachments: files,
            ..NewTicket::default()
        };
        return Ok(vec![Step::Open(Box::new(new))]);
    }

    if ticket.is_none() {
        return Err(refuse("falta o chamado de destino"));
    }
    if request.action == ShareAction::Attach {
        if let Some(text) = text {
            let pdf = text_to_pdf(request.title.as_deref(), text);
            files.push(Attachment {
                file_name: "texto.pdf".to_owned(),
                bytes: pdf,
            });
        }
        if files.is_empty() {
            return Err(refuse("não há nada para anexar"));
        }
    }
    validate_attachments(&files)?;
    let description = request.attachment_description.clone();
    let mut steps: Vec<Step> = files
        .into_iter()
        .map(|file| Step::Attach(file, description.clone()))
        .collect();

    if request.action != ShareAction::Attach {
        let text = text.ok_or_else(|| refuse("falta o texto: a mensagem ou o motivo"))?;
        let text = text.to_owned();
        steps.push(match request.action {
            ShareAction::Comment => Step::Send(Message::Comment, text),
            ShareAction::InternalNote => Step::Send(Message::InternalNote, text),
            ShareAction::Resolve => Step::Resolve(text),
            ShareAction::Cancel => Step::Cancel(text),
            ShareAction::Reopen => Step::Reopen(text),
            _ => Step::Suspend(text),
        });
    }
    Ok(steps)
}

async fn run(
    source: &SuapTicketSource<'_>,
    step: &Step,
    ticket: &str,
) -> Result<Option<String>, AppError> {
    match step {
        Step::Open(new) => return Ok(Some(source.open_ticket(new).await?)),
        Step::Attach(file, description) => {
            source
                .attach_file(ticket, file, description.as_deref())
                .await?
        }
        Step::Send(kind, text) => source.add_message(ticket, *kind, text).await?,
        Step::Suspend(text) => source.suspend_ticket(ticket, text).await?,
        Step::Cancel(text) => source.cancel_ticket(ticket, text).await?,
        Step::Reopen(text) => source.reopen_ticket(ticket, text).await?,
        Step::Resolve(text) => {
            let resolution = Resolution {
                text: text.clone(),
                ..Resolution::default()
            };
            source.resolve_ticket(ticket, &resolution).await?;
        }
    }
    Ok(None)
}

/// Executa o pedido, um passo por vez e parando no primeiro que falhar (os anexos vão antes da
/// mensagem, pois depois de cancelar ou resolver o SUAP não aceita mais anexos).
///
/// Erro (`Err`) só quando o pedido em si não faz sentido, e nesse caso nada foi enviado. Falhas do
/// SUAP vêm no relatório, com o que já foi feito, para tentar de novo só o resto.
pub async fn apply_share(
    source: &SuapTicketSource<'_>,
    request: &ShareRequest,
) -> Result<ShareReport, AppError> {
    let steps = plan(request)?;
    let mut report = ShareReport {
        total: steps.len(),
        ticket: request.ticket.clone(),
        ..ShareReport::default()
    };
    for (index, step) in steps.iter().enumerate() {
        let label = step.label();
        if index < request.resume_from {
            report.steps.push(ShareStep { label, error: None });
            report.completed += 1;
            continue;
        }
        let ticket = report.ticket.clone().unwrap_or_default();
        let outcome = run(source, step, &ticket).await;
        let error = match outcome {
            Ok(created) => {
                report.ticket = created.or(report.ticket.take());
                report.completed += 1;
                None
            }
            Err(error) => Some(error.to_string()),
        };
        let failed = error.is_some();
        report.steps.push(ShareStep { label, error });
        if failed {
            break;
        }
    }
    Ok(report)
}
