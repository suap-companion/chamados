//! O assistente de redação ("Sugerir com IA") para o aplicativo: monta o pedido ao modelo a partir do
//! chamado aberto, limpa a resposta e dá o modelo pronto quando não há IA no aparelho. Os modelos
//! (AICore ou um modelo local) ficam no Kotlin; as regras (privacidade, orçamento, limpeza) ficam em
//! `chamados-assist`, iguais para qualquer modelo.

use chamados_assist as assist;

use crate::{EntryKind, ThreadItem, TicketView};

/// O texto que a pessoa quer escrever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DraftKind {
    Title,
    Comment,
    InternalNote,
    Suspend,
    Cancel,
    Resolve,
    Close,
    Reopen,
    Escalate,
    Return,
}

impl From<DraftKind> for assist::DraftKind {
    fn from(kind: DraftKind) -> Self {
        match kind {
            DraftKind::Title => Self::Title,
            DraftKind::Comment => Self::Comment,
            DraftKind::InternalNote => Self::InternalNote,
            DraftKind::Suspend => Self::Suspend,
            DraftKind::Cancel => Self::Cancel,
            DraftKind::Resolve => Self::Resolve,
            DraftKind::Close => Self::Close,
            DraftKind::Reopen => Self::Reopen,
            DraftKind::Escalate => Self::Escalate,
            DraftKind::Return => Self::Return,
        }
    }
}

/// Como ajustar um rascunho que a pessoa já recebeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DraftRefine {
    Shorter,
    MoreFormal,
    Redo,
}

impl From<DraftRefine> for assist::Refine {
    fn from(refine: DraftRefine) -> Self {
        match refine {
            DraftRefine::Shorter => Self::Shorter,
            DraftRefine::MoreFormal => Self::MoreFormal,
            DraftRefine::Redo => Self::Redo,
        }
    }
}

/// O pedido de um rascunho: o que escrever, o chamado aberto e o que a pessoa quer dizer.
#[derive(Debug, uniffi::Record)]
pub struct DraftInput {
    pub kind: DraftKind,
    /// O chamado aberto (o único contexto que o modelo recebe).
    pub ticket: TicketView,
    /// O título que a pessoa deu ao chamado, se houver.
    pub local_title: Option<String>,
    /// O que a pessoa quer dizer, em poucas palavras.
    pub hint: Option<String>,
    /// O rascunho anterior, ao refinar.
    pub previous: Option<String>,
    pub refine: Option<DraftRefine>,
    /// Trechos dos documentos de apoio achados para este chamado (os mais relevantes primeiro).
    pub references: Vec<DraftReference>,
}

/// Um trecho de documento de apoio (de `docs_search`) para o rascunho.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DraftReference {
    /// O nome do arquivo.
    pub source: String,
    pub page: Option<u32>,
    pub text: String,
}

/// O pedido pronto para o modelo.
#[derive(Debug, uniffi::Record)]
pub struct DraftPrompt {
    pub text: String,
    /// Alguma parte do chamado ficou de fora (texto cortado ou entradas antigas descartadas).
    pub truncated: bool,
}

fn reference(item: DraftReference) -> assist::Reference {
    assist::Reference {
        source: item.source,
        page: item.page,
        text: item.text,
    }
}

fn context_kind(kind: EntryKind) -> assist::ContextKind {
    match kind {
        EntryKind::Comment => assist::ContextKind::Comment,
        EntryKind::Note => assist::ContextKind::Note,
        EntryKind::Event => assist::ContextKind::Event,
        EntryKind::Attachment => assist::ContextKind::Attachment,
    }
}

fn entry(item: &ThreadItem) -> assist::ContextEntry {
    assist::ContextEntry {
        kind: context_kind(item.kind),
        author: item.author.clone(),
        date: item.date.clone(),
        body: item.body.clone(),
    }
}

/// O que o modelo pode saber do chamado: tipo, situação, descrição e o histórico (comentários,
/// acontecimentos, anexos e notas) do mais antigo ao mais novo. Quem decide se as notas entram é o
/// `chamados-assist`, pelo tipo do texto.
fn context(input: &DraftInput) -> assist::TicketContext {
    let view = &input.ticket;
    // A linha do tempo do SUAP vem do mais novo ao mais antigo; as notas, juntas, ordenadas pela data.
    let mut items: Vec<&ThreadItem> = view.thread.iter().rev().chain(view.notes.iter()).collect();
    items.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    let description = view
        .fields
        .iter()
        .find(|field| field.label.trim().trim_end_matches(':').to_lowercase() == "descrição")
        .map(|field| field.value.clone());
    assist::TicketContext {
        id: view.id.clone(),
        title: input.local_title.clone(),
        kind: view.heading.clone().unwrap_or_else(|| view.title.clone()),
        statuses: view.statuses.clone(),
        description,
        entries: items.into_iter().map(entry).collect(),
    }
}

/// Monta o pedido ao modelo para o rascunho; `budget` é o tamanho máximo do pedido em caracteres
/// (sem ele, o padrão, que cabe no limite de entrada do Gemini Nano).
#[uniffi::export]
pub fn assist_prompt(input: DraftInput, budget: Option<u32>) -> DraftPrompt {
    let budget = budget.map_or(assist::DEFAULT_BUDGET_CHARS, |chars| chars as usize);
    let request = assist::DraftRequest {
        kind: input.kind.into(),
        ticket: context(&input),
        hint: input.hint,
        previous: input.previous,
        refine: input.refine.map(Into::into),
        references: input.references.into_iter().map(reference).collect(),
    };
    let prompt = assist::build_prompt(&request, budget);
    DraftPrompt {
        text: prompt.text,
        truncated: prompt.truncated,
    }
}

/// Limpa a resposta do modelo para o campo de texto (sem cercas, marcação, rótulos nem aspas; o título
/// em uma linha de até 120 caracteres).
#[uniffi::export]
pub fn assist_clean(kind: DraftKind, raw: String) -> String {
    assist::clean_output(kind.into(), &raw)
}

/// O modelo pronto do texto, para quando não há IA no aparelho: um esqueleto com o que a pessoa disse
/// (ou `[preencher]`) para ela completar.
#[uniffi::export]
pub fn assist_template(input: DraftInput) -> String {
    assist::template(input.kind.into(), &context(&input), input.hint.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TicketField;

    fn item(kind: EntryKind, key: &str, who: Option<&str>, body: &str) -> ThreadItem {
        ThreadItem {
            kind,
            date: format!("data {key}"),
            sort_key: key.to_owned(),
            author: who.map(str::to_owned),
            photo: None,
            body: body.to_owned(),
        }
    }

    fn view() -> TicketView {
        TicketView {
            id: "7".to_owned(),
            title: "Chamado Interno 7".to_owned(),
            heading: Some("Moodle | Atualização".to_owned()),
            statuses: vec!["Aberto".to_owned()],
            fields: vec![
                TicketField {
                    label: "Interessado".to_owned(),
                    value: "Ana".to_owned(),
                },
                TicketField {
                    label: " Descrição: ".to_owned(),
                    value: "Atualizar o Moodle".to_owned(),
                },
            ],
            // O SUAP entrega do mais novo ao mais antigo.
            thread: vec![
                item(
                    EntryKind::Comment,
                    "2026-10-13 10:00:00",
                    Some("Ana"),
                    "Já subiram?",
                ),
                item(
                    EntryKind::Event,
                    "2026-10-12 09:00:00",
                    None,
                    "Chamado aberto",
                ),
                item(
                    EntryKind::Attachment,
                    "2026-10-12 09:30:00",
                    Some("Ana"),
                    "log.txt",
                ),
            ],
            notes: vec![item(
                EntryKind::Note,
                "2026-10-12 11:00:00",
                Some("Bia"),
                "SEGREDO",
            )],
            can_see_notes: true,
            attachments: vec![],
            photo: None,
            url: String::new(),
        }
    }

    fn input(kind: DraftKind) -> DraftInput {
        DraftInput {
            kind,
            ticket: view(),
            local_title: Some("Moodle 5.3".to_owned()),
            hint: None,
            previous: None,
            refine: None,
            references: vec![],
        }
    }

    const ALL: [DraftKind; 10] = [
        DraftKind::Title,
        DraftKind::Comment,
        DraftKind::InternalNote,
        DraftKind::Suspend,
        DraftKind::Cancel,
        DraftKind::Resolve,
        DraftKind::Close,
        DraftKind::Reopen,
        DraftKind::Escalate,
        DraftKind::Return,
    ];

    #[test]
    fn the_context_is_the_open_ticket_oldest_first_and_notes_follow_the_text_kind() {
        let public = assist_prompt(input(DraftKind::Comment), None);
        assert!(!public.truncated);
        assert!(public.text.contains("Tipo: Moodle | Atualização"));
        assert!(public.text.contains("Título: Moodle 5.3"));
        assert!(public.text.contains("Descrição: Atualizar o Moodle"));
        assert!(!public.text.contains("SEGREDO"));
        let opened = public.text.find("Chamado aberto").unwrap();
        let attached = public.text.find("log.txt").unwrap();
        let asked = public.text.find("Já subiram?").unwrap();
        assert!(opened < attached && attached < asked);

        let internal = assist_prompt(input(DraftKind::InternalNote), Some(9000)).text;
        let note = internal.find("SEGREDO").unwrap();
        assert!(
            internal.find("log.txt").unwrap() < note
                && note < internal.find("Já subiram?").unwrap()
        );
    }

    #[test]
    fn a_ticket_without_heading_or_description_still_has_a_context() {
        let mut bare = input(DraftKind::Comment);
        bare.ticket.heading = None;
        bare.ticket.fields.clear();
        let text = assist_prompt(bare, None).text;
        assert!(text.contains("Tipo: Chamado Interno 7"));
        assert!(!text.contains("Descrição:"));
    }

    #[test]
    fn the_budget_and_the_refinements_reach_the_prompt() {
        let mut req = input(DraftKind::Resolve);
        req.hint = Some("reiniciei o serviço".to_owned());
        req.previous = Some("Rascunho velho".to_owned());
        req.refine = Some(DraftRefine::Shorter);
        let tiny = assist_prompt(req, Some(10));
        assert!(tiny.truncated);
        assert!(tiny.text.contains("reiniciei o serviço") && tiny.text.contains("mais curto"));
        for (refine, expected) in [
            (DraftRefine::MoreFormal, "tom mais formal"),
            (DraftRefine::Redo, "outras palavras"),
        ] {
            let mut req = input(DraftKind::Resolve);
            req.refine = Some(refine);
            assert!(assist_prompt(req, None).text.contains(expected));
        }
    }

    #[test]
    fn support_documents_reach_the_prompt_with_file_and_page() {
        let mut req = input(DraftKind::Resolve);
        req.references = vec![DraftReference {
            source: "Manual.pdf".to_owned(),
            page: Some(3),
            text: "Para reiniciar, desligue da tomada.".to_owned(),
        }];
        let text = assist_prompt(req, None).text;
        assert!(text.contains("[1] Manual.pdf, p. 3: Para reiniciar, desligue da tomada."));
        let without = assist_prompt(input(DraftKind::Resolve), None).text;
        assert!(!without.contains("Documentos de apoio"));
    }

    #[test]
    fn every_kind_maps_to_its_twin_and_cleans_and_templates() {
        for kind in ALL {
            let twin: assist::DraftKind = kind.into();
            assert_eq!(
                twin.is_internal(),
                matches!(
                    kind,
                    DraftKind::InternalNote | DraftKind::Escalate | DraftKind::Return
                )
            );
            assert_eq!(assist_clean(kind, "Texto: pronto".to_owned()), "pronto");
            assert!(!assist_template(input(kind)).is_empty());
        }
        let mut said = input(DraftKind::Cancel);
        said.hint = Some("duplicado".to_owned());
        assert_eq!(assist_template(said), "Chamado cancelado: duplicado.");
    }
}
