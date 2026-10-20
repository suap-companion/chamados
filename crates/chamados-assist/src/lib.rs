//! Assistente de redação ("Sugerir com IA"): monta o pedido ao modelo e limpa o que ele devolve.
//!
//! O modelo roda no aparelho (fora deste crate: AICore ou um modelo local). Aqui ficam as regras que
//! precisam ser testáveis e iguais para qualquer modelo:
//! - **privacidade**: notas internas só entram no contexto de textos internos, nunca num texto público;
//! - **orçamento**: o contexto cabe no limite de entrada do modelo (a Prompt API aceita menos de 4000
//!   tokens), com as entradas mais recentes tendo prioridade;
//! - **fidelidade**: o prompt manda usar só os fatos do chamado e do que a pessoa disse, e marcar
//!   [`PLACEHOLDER`] onde faltar dado;
//! - **limpeza**: tira cercas de código, marcação, rótulos e aspas da resposta, e respeita o limite do
//!   título.
//!
//! Nada aqui fala com o SUAP, com a rede ou com o disco.

use chamados_core::MAX_TITLE_CHARS;

/// Onde faltar informação, o modelo (e os modelos prontos) escrevem isto para a pessoa preencher.
pub const PLACEHOLDER: &str = "[preencher]";
/// O maior texto aceito de volta do modelo (comentários, notas e mensagens).
pub const MAX_TEXT_CHARS: usize = 4000;
/// O tamanho padrão do pedido, em caracteres: cerca de 3000 tokens, abaixo do limite de 4000 do modelo.
pub const DEFAULT_BUDGET_CHARS: usize = 9000;

const DESCRIPTION_CHARS: usize = 1500;
const ENTRY_CHARS: usize = 600;
const MAX_ENTRIES: usize = 12;
const HINT_CHARS: usize = 1000;
const REFERENCE_CHARS: usize = 700;
const MAX_REFERENCES: usize = 3;
const HISTORY_HEADER: &str = "\nHistórico (do mais antigo ao mais novo):\n";
const PREVIOUS_CHARS: usize = 2000;

/// O texto que a pessoa quer escrever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl DraftKind {
    /// Se o texto é interno (só a equipe de atendimento lê): só estes veem as notas internas.
    pub fn is_internal(self) -> bool {
        matches!(self, Self::InternalNote | Self::Escalate | Self::Return)
    }

    fn guideline(self) -> &'static str {
        match self {
            Self::Title => "Crie um título curto, em uma única linha, que identifique o problema ou o pedido, sem o número do chamado.",
            Self::Comment => "Escreva um comentário público para o interessado, claro e cordial.",
            Self::InternalNote => "Escreva uma nota interna técnica para a equipe de atendimento: objetiva, com o que foi verificado e os próximos passos.",
            Self::Suspend => "Escreva a mensagem de suspensão: o motivo e o que é preciso para o atendimento ser retomado.",
            Self::Cancel => "Escreva o motivo do cancelamento, de forma educada e direta.",
            Self::Resolve => "Escreva a resolução: o que foi feito para resolver e, se couber, como confirmar que funcionou.",
            Self::Close => "Escreva o comentário de fechamento, sobre o atendimento recebido, de forma cordial.",
            Self::Reopen => "Escreva o motivo da reabertura: o que ainda não está resolvido.",
            Self::Escalate => "Escreva a nota interna ao escalar o chamado para o nível superior: por que escalar e o que já foi tentado.",
            Self::Return => "Escreva a nota interna ao devolver o chamado ao nível anterior: por que devolver e o que falta.",
        }
    }

    fn template(self, hint: &str) -> String {
        match self {
            Self::Title => hint.to_owned(),
            Self::Comment => format!("Olá! {hint}\n\nQualquer dúvida, estamos à disposição."),
            Self::InternalNote => format!("Verificado: {hint}\nPróximos passos: {PLACEHOLDER}"),
            Self::Suspend => format!(
                "Chamado suspenso porque {hint}. Para retomar, é necessário: {PLACEHOLDER}."
            ),
            Self::Cancel => format!("Chamado cancelado: {hint}."),
            Self::Resolve => {
                format!("Solução aplicada: {hint}.\n\nQualquer dúvida, estamos à disposição.")
            }
            Self::Close => format!("Atendimento: {hint}. Obrigado(a)."),
            Self::Reopen => format!("Reabrindo o chamado porque {hint}."),
            Self::Escalate => format!("Escalando para o nível superior: {hint}."),
            Self::Return => format!("Devolvendo ao nível anterior: {hint}."),
        }
    }
}

/// Como ajustar um rascunho que a pessoa já recebeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refine {
    Shorter,
    MoreFormal,
    Redo,
}

impl Refine {
    fn instruction(self) -> &'static str {
        match self {
            Self::Shorter => "Deixe o rascunho anterior mais curto, mantendo os fatos.",
            Self::MoreFormal => {
                "Reescreva o rascunho anterior em tom mais formal, mantendo os fatos."
            }
            Self::Redo => "Escreva de novo, com outras palavras.",
        }
    }
}

/// O tipo de uma entrada do histórico do chamado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    Comment,
    Note,
    Event,
    Attachment,
}

impl ContextKind {
    fn label(self) -> &'static str {
        match self {
            Self::Comment => "comentário",
            Self::Note => "nota interna",
            Self::Event => "acontecimento",
            Self::Attachment => "anexo",
        }
    }
}

/// Uma entrada do histórico do chamado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    pub kind: ContextKind,
    pub author: Option<String>,
    pub date: String,
    pub body: String,
}

/// O que o modelo pode saber do chamado aberto (e de mais nada).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketContext {
    pub id: String,
    /// O título que a pessoa deu ao chamado, se houver.
    pub title: Option<String>,
    /// O tipo (serviço) do chamado.
    pub kind: String,
    pub statuses: Vec<String>,
    pub description: Option<String>,
    /// O histórico, do mais antigo ao mais novo.
    pub entries: Vec<ContextEntry>,
}

/// Um trecho de um documento de apoio que a pessoa importou (procedimento, modelo, FAQ).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// O nome do arquivo de onde veio.
    pub source: String,
    /// A página (PDF), se houver.
    pub page: Option<u32>,
    pub text: String,
}

impl Reference {
    /// Como a fonte aparece no texto: "Manual.pdf, p. 3" ou só "notas.txt".
    pub fn label(&self) -> String {
        match self.page {
            Some(page) => format!("{}, p. {page}", self.source),
            None => self.source.clone(),
        }
    }
}

/// O pedido de um rascunho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRequest {
    pub kind: DraftKind,
    pub ticket: TicketContext,
    /// O que a pessoa quer dizer, em poucas palavras.
    pub hint: Option<String>,
    /// O rascunho anterior, ao refinar.
    pub previous: Option<String>,
    pub refine: Option<Refine>,
    /// Trechos de documentos de apoio achados para este chamado (os mais relevantes primeiro).
    pub references: Vec<Reference>,
}

/// O pedido pronto para o modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    /// Alguma parte do chamado ficou de fora (texto cortado ou entradas antigas descartadas).
    pub truncated: bool,
}

/// Corta `text` em `max` caracteres, com reticências; devolve também se cortou.
fn shorten(text: &str, max: usize) -> (String, bool) {
    let text = text.trim();
    if text.chars().count() <= max {
        return (text.to_owned(), false);
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    (format!("{}…", cut.trim_end()), true)
}

/// Uma linha só: quebras de linha e espaços repetidos viram um espaço.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_entry(entry: &ContextEntry) -> (String, bool) {
    let (body, cut) = shorten(&one_line(&entry.body), ENTRY_CHARS);
    let who = entry.author.as_deref().unwrap_or("sistema");
    (
        format!("- [{}] {who}, {}: {body}\n", entry.kind.label(), entry.date),
        cut,
    )
}

/// Monta o pedido ao modelo, com no máximo `budget` caracteres no contexto do chamado.
///
/// As notas internas só entram quando o texto também é interno. O que não cabe no orçamento fica de
/// fora, começando pelas entradas mais antigas.
pub fn build_prompt(request: &DraftRequest, budget: usize) -> Prompt {
    let ticket = &request.ticket;
    let mut truncated = false;

    let mut head = String::from(
        "Você ajuda a redigir textos para a Central de Serviços do SUAP. Escreva em português do Brasil, \
de forma clara e objetiva.\n",
    );
    head.push_str(&format!("Tarefa: {}\n", request.kind.guideline()));
    head.push_str(&format!(
        "Regras: use somente os fatos do chamado e do que a pessoa informou; não invente causas, soluções, \
datas, nomes nem números; onde faltar uma informação necessária, escreva {PLACEHOLDER}; devolva apenas o \
texto final, sem aspas, sem markdown e sem explicações.\n\n"
    ));
    head.push_str(&format!("Chamado #{}\n", ticket.id));
    head.push_str(&format!("Tipo: {}\n", one_line(&ticket.kind)));
    if !ticket.statuses.is_empty() {
        head.push_str(&format!("Situação: {}\n", ticket.statuses.join(", ")));
    }
    if let Some(title) = &ticket.title {
        head.push_str(&format!("Título: {}\n", one_line(title)));
    }
    if let Some(description) = &ticket.description {
        let (text, cut) = shorten(&one_line(description), DESCRIPTION_CHARS);
        truncated |= cut;
        head.push_str(&format!("Descrição: {text}\n"));
    }
    if !request.references.is_empty() {
        head.push_str(
            "\nDocumentos de apoio (use só o que ajudar; se usar, cite a fonte entre parênteses, como (Manual.pdf, p. 3)):\n",
        );
        truncated |= request.references.len() > MAX_REFERENCES;
        for (number, reference) in request.references.iter().take(MAX_REFERENCES).enumerate() {
            let (text, cut) = shorten(&one_line(&reference.text), REFERENCE_CHARS);
            truncated |= cut;
            head.push_str(&format!("[{}] {}: {text}\n", number + 1, reference.label()));
        }
    }

    let mut tail = String::new();
    if let Some(hint) = &request.hint {
        let (text, cut) = shorten(&one_line(hint), HINT_CHARS);
        truncated |= cut;
        tail.push_str(&format!("\nO que a pessoa quer dizer: {text}\n"));
    }
    if let Some(previous) = &request.previous {
        let (text, cut) = shorten(previous, PREVIOUS_CHARS);
        truncated |= cut;
        tail.push_str(&format!("\nRascunho anterior:\n{text}\n"));
    }
    if let Some(refine) = request.refine {
        tail.push_str(&format!("\nAjuste: {}\n", refine.instruction()));
    }
    tail.push_str("\nTexto:");

    // O histórico: só o que a privacidade deixa, e o que cabe, das entradas mais novas para as antigas.
    let usable = ticket
        .entries
        .iter()
        .filter(|entry| request.kind.is_internal() || entry.kind != ContextKind::Note);
    let recent: Vec<&ContextEntry> = usable.rev().collect();
    truncated |= recent.len() > MAX_ENTRIES;
    let fixed = head.chars().count() + HISTORY_HEADER.chars().count() + tail.chars().count();
    let mut room = budget.saturating_sub(fixed);
    let mut kept: Vec<String> = Vec::new();
    for entry in recent.into_iter().take(MAX_ENTRIES) {
        let (line, cut) = render_entry(entry);
        let size = line.chars().count();
        if size > room {
            truncated = true;
            break;
        }
        room -= size;
        truncated |= cut;
        kept.push(line);
    }
    kept.reverse();

    let mut text = head;
    if !kept.is_empty() {
        text.push_str(HISTORY_HEADER);
        text.push_str(&kept.concat());
    }
    text.push_str(&tail);
    Prompt { text, truncated }
}

/// Os rótulos que os modelos costumam pôr antes do texto.
const LABELS: [&str; 6] = [
    "texto:",
    "título:",
    "resposta:",
    "rascunho:",
    "comentário:",
    "nota interna:",
];

/// Limpa a resposta do modelo para o campo de texto: sem cercas de código, marcação, rótulo nem aspas
/// em volta; o título vira uma linha só, de até [`MAX_TITLE_CHARS`] caracteres; os demais textos, de até
/// [`MAX_TEXT_CHARS`].
pub fn clean_output(kind: DraftKind, raw: &str) -> String {
    let lines: Vec<&str> = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .map(|line| strip_heading(line.trim_end()))
        .collect();
    let mut text = lines.join("\n").replace("**", "");
    text = text.trim().to_owned();

    let lower = text.to_lowercase();
    for label in LABELS {
        if lower.starts_with(label) {
            text = text
                .chars()
                .skip(label.chars().count())
                .collect::<String>()
                .trim()
                .to_owned();
            break;
        }
    }

    if kind == DraftKind::Title {
        let first = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("");
        let single = one_line(first);
        return fit_title(&unquote(single.trim_end_matches('.')));
    }
    text = unquote(&text);

    // Linhas em branco demais viram uma só.
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.chars()
        .take(MAX_TEXT_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Tira o `#` de título de markdown ("## Olá" vira "Olá"); um `#` colado no texto ("#123") fica.
fn strip_heading(line: &str) -> &str {
    let rest = line.trim_start_matches('#');
    if rest.len() < line.len() && rest.starts_with(' ') {
        rest.trim_start()
    } else {
        line
    }
}

/// Tira as aspas que cercam o texto inteiro (retas, curvas ou simples); aspas só de um lado ficam.
fn unquote(text: &str) -> String {
    let text = text.trim();
    for (open, close) in [('"', '"'), ('“', '”'), ('\'', '\'')] {
        if text.chars().count() >= 2 && text.starts_with(open) && text.ends_with(close) {
            let inner: String = text.chars().skip(1).collect();
            return inner.trim_end_matches(close).trim().to_owned();
        }
    }
    text.to_owned()
}

/// O título cabe em [`MAX_TITLE_CHARS`]: se passa, corta na última palavra inteira.
fn fit_title(title: &str) -> String {
    if title.chars().count() <= MAX_TITLE_CHARS {
        return title.to_owned();
    }
    let cut: String = title.chars().take(MAX_TITLE_CHARS).collect();
    match cut.rfind(' ') {
        Some(space) => cut[..space].trim_end().to_owned(),
        None => cut,
    }
}

/// O modelo pronto do texto, para quando não há IA no aparelho: um esqueleto com o que a pessoa disse
/// (ou [`PLACEHOLDER`]) para ela completar. O título vem do que a pessoa disse ou do tipo do chamado.
pub fn template(kind: DraftKind, ticket: &TicketContext, hint: Option<&str>) -> String {
    let said = hint.map(one_line).filter(|text| !text.is_empty());
    if kind == DraftKind::Title {
        let base = said.unwrap_or_else(|| one_line(&ticket.kind));
        return fit_title(&kind.template(&base));
    }
    kind.template(&said.unwrap_or_else(|| PLACEHOLDER.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: ContextKind, who: Option<&str>, body: &str) -> ContextEntry {
        ContextEntry {
            kind,
            author: who.map(str::to_owned),
            date: "13/10/2026 10:00:00".to_owned(),
            body: body.to_owned(),
        }
    }

    fn ticket(entries: Vec<ContextEntry>) -> TicketContext {
        TicketContext {
            id: "559298".to_owned(),
            title: Some("Moodle 5.3".to_owned()),
            kind: "Atualização\nde versão".to_owned(),
            statuses: vec!["Em atendimento".to_owned()],
            description: Some("Atualizar   o Moodle\npara a 5.3.0".to_owned()),
            entries,
        }
    }

    fn request(kind: DraftKind, entries: Vec<ContextEntry>) -> DraftRequest {
        DraftRequest {
            kind,
            ticket: ticket(entries),
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
    fn only_internal_texts_are_internal() {
        let internal: Vec<bool> = ALL.iter().map(|kind| kind.is_internal()).collect();
        assert_eq!(
            internal,
            [false, false, true, false, false, false, false, false, true, true]
        );
    }

    #[test]
    fn every_kind_has_its_own_guideline_and_the_rules() {
        let mut seen = Vec::new();
        for kind in ALL {
            let prompt = build_prompt(&request(kind, vec![]), DEFAULT_BUDGET_CHARS).text;
            assert!(prompt.contains(kind.guideline()));
            assert!(prompt.contains("não invente"));
            assert!(prompt.contains(PLACEHOLDER));
            assert!(prompt.ends_with("Texto:"));
            seen.push(kind.guideline());
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ALL.len());
    }

    #[test]
    fn the_prompt_describes_the_ticket_on_single_lines() {
        let prompt = build_prompt(&request(DraftKind::Comment, vec![]), DEFAULT_BUDGET_CHARS);
        assert!(!prompt.truncated);
        assert!(prompt.text.contains("Chamado #559298\n"));
        assert!(prompt.text.contains("Tipo: Atualização de versão\n"));
        assert!(prompt.text.contains("Situação: Em atendimento\n"));
        assert!(prompt.text.contains("Título: Moodle 5.3\n"));
        assert!(prompt
            .text
            .contains("Descrição: Atualizar o Moodle para a 5.3.0\n"));
        assert!(!prompt.text.contains("Histórico"));
        // Sem título, sem situação e sem descrição, essas linhas não existem.
        let bare = TicketContext {
            title: None,
            statuses: vec![],
            description: None,
            ..ticket(vec![])
        };
        let text = build_prompt(
            &DraftRequest {
                ticket: bare,
                ..request(DraftKind::Comment, vec![])
            },
            9000,
        )
        .text;
        assert!(
            !text.contains("Título:")
                && !text.contains("Situação:")
                && !text.contains("Descrição:")
        );
    }

    #[test]
    fn internal_notes_never_reach_a_public_text() {
        let entries = vec![
            entry(ContextKind::Comment, Some("Ana"), "Preciso de ajuda."),
            entry(ContextKind::Note, Some("Bia"), "SEGREDO da equipe"),
            entry(ContextKind::Event, None, "Chamado aberto"),
            entry(ContextKind::Attachment, Some("Ana"), "print.png"),
        ];
        for kind in ALL {
            let text = build_prompt(&request(kind, entries.clone()), DEFAULT_BUDGET_CHARS).text;
            assert_eq!(
                text.contains("SEGREDO da equipe"),
                kind.is_internal(),
                "{kind:?}"
            );
            assert!(text.contains("[comentário] Ana, 13/10/2026 10:00:00: Preciso de ajuda."));
            assert!(text.contains("[acontecimento] sistema, "));
            assert!(text.contains("[anexo] Ana, "));
        }
    }

    #[test]
    fn the_history_keeps_the_newest_entries_in_chronological_order_within_the_budget() {
        let entries: Vec<ContextEntry> = (1..=5)
            .map(|n| {
                entry(
                    ContextKind::Comment,
                    Some("Ana"),
                    &format!("mensagem {n} {}", "x".repeat(300)),
                )
            })
            .collect();
        let big = build_prompt(
            &request(DraftKind::Comment, entries.clone()),
            DEFAULT_BUDGET_CHARS,
        );
        assert!(!big.truncated);
        let pos: Vec<usize> = (1..=5)
            .map(|n| big.text.find(&format!("mensagem {n} ")).unwrap())
            .collect();
        assert!(pos.windows(2).all(|pair| pair[0] < pair[1]));

        // Um orçamento curto descarta as mais antigas e avisa.
        let fixed = build_prompt(&request(DraftKind::Comment, vec![]), 0)
            .text
            .chars()
            .count();
        let small = build_prompt(&request(DraftKind::Comment, entries), fixed + 900);
        assert!(small.truncated);
        assert!(small.text.contains("mensagem 5 ") && small.text.contains("mensagem 4 "));
        assert!(!small.text.contains("mensagem 1 "));
        assert!(small.text.chars().count() <= fixed + 900);
    }

    #[test]
    fn long_texts_and_many_entries_are_cut_and_flagged() {
        let long = "palavra ".repeat(500);
        let mut req = request(
            DraftKind::Comment,
            vec![entry(ContextKind::Comment, None, &long)],
        );
        req.ticket.description = Some(long.clone());
        req.hint = Some(long.clone());
        req.previous = Some(long.clone());
        let prompt = build_prompt(&req, 100_000);
        assert!(prompt.truncated);
        assert!(prompt.text.matches('…').count() >= 4);

        let many: Vec<ContextEntry> = (0..MAX_ENTRIES + 3)
            .map(|n| entry(ContextKind::Comment, Some("Ana"), &format!("m{n}")))
            .collect();
        let prompt = build_prompt(&request(DraftKind::Comment, many), 100_000);
        assert!(prompt.truncated);
        assert!(
            !prompt.text.contains("m0\n")
                && prompt.text.contains(&format!("m{}\n", MAX_ENTRIES + 2))
        );
    }

    fn reference(source: &str, page: Option<u32>, text: &str) -> Reference {
        Reference {
            source: source.to_owned(),
            page,
            text: text.to_owned(),
        }
    }

    #[test]
    fn support_documents_are_numbered_cited_and_kept_short() {
        assert_eq!(
            reference("Manual.pdf", Some(3), "x").label(),
            "Manual.pdf, p. 3"
        );
        assert_eq!(reference("notas.txt", None, "x").label(), "notas.txt");

        let mut req = request(DraftKind::Resolve, vec![]);
        // Sem referências, a seção não existe.
        assert!(!build_prompt(&req, 9000)
            .text
            .contains("Documentos de apoio"));
        req.references = vec![
            reference(
                "Manual.pdf",
                Some(3),
                "Para reiniciar,\ndesligue da tomada.",
            ),
            reference("notas.txt", None, "Troque o toner original."),
        ];
        let prompt = build_prompt(&req, 9000);
        assert!(!prompt.truncated);
        assert!(prompt.text.contains("cite a fonte entre parênteses"));
        assert!(prompt
            .text
            .contains("[1] Manual.pdf, p. 3: Para reiniciar, desligue da tomada.\n"));
        assert!(prompt
            .text
            .contains("[2] notas.txt: Troque o toner original.\n"));
        // O que passa de 3 trechos ou de 700 caracteres fica de fora, e o pedido avisa.
        req.references = (0..4)
            .map(|n| reference("a.txt", None, &format!("trecho{n}")))
            .collect();
        let many = build_prompt(&req, 9000);
        assert!(many.truncated && many.text.contains("trecho2") && !many.text.contains("trecho3"));
        req.references = vec![reference("a.txt", None, &"palavra ".repeat(200))];
        assert!(build_prompt(&req, 9000).truncated);
    }

    #[test]
    fn the_hint_and_the_refinements_are_part_of_the_prompt() {
        let mut req = request(DraftKind::Resolve, vec![]);
        req.hint = Some("reiniciei o\nserviço".to_owned());
        let text = build_prompt(&req, 9000).text;
        assert!(text.contains("O que a pessoa quer dizer: reiniciei o serviço\n"));
        assert!(!text.contains("Rascunho anterior") && !text.contains("Ajuste:"));

        for (refine, expected) in [
            (Refine::Shorter, "mais curto"),
            (Refine::MoreFormal, "tom mais formal"),
            (Refine::Redo, "outras palavras"),
        ] {
            let mut req = request(DraftKind::Resolve, vec![]);
            req.previous = Some("Rascunho velho".to_owned());
            req.refine = Some(refine);
            let text = build_prompt(&req, 9000).text;
            assert!(text.contains("Rascunho anterior:\nRascunho velho\n"));
            assert!(text.contains("Ajuste: ") && text.contains(expected));
        }
    }

    #[test]
    fn shorten_and_one_line_are_safe_with_accents() {
        assert_eq!(shorten("  curto  ", 10), ("curto".to_owned(), false));
        assert_eq!(shorten("ação ação ação", 6), ("ação…".to_owned(), true));
        assert_eq!(shorten("abc", 0), ("…".to_owned(), true));
        assert_eq!(one_line(" a \n b\t c "), "a b c");
    }

    #[test]
    fn the_output_is_cleaned_of_fences_markup_labels_and_quotes() {
        let c = |raw: &str| clean_output(DraftKind::Comment, raw);
        assert_eq!(c("```\nOlá, tudo bem?\n```"), "Olá, tudo bem?");
        assert_eq!(c("## Olá\n**Tudo** bem?"), "Olá\nTudo bem?");
        assert_eq!(c("#559298 aberto"), "#559298 aberto");
        assert_eq!(c("Texto: Resolvido."), "Resolvido.");
        assert_eq!(c("NOTA INTERNA:   verificado"), "verificado");
        assert_eq!(c("\"Entre aspas\""), "Entre aspas");
        assert_eq!(c("“Aspas curvas”"), "Aspas curvas");
        assert_eq!(c("'Simples'"), "Simples");
        assert_eq!(c("\"só abre"), "\"só abre");
        assert_eq!(c("\""), "\"");
        assert_eq!(c("Um\n\n\n\n\nDois"), "Um\n\nDois");
        assert_eq!(c("  \n "), "");
        assert_eq!(
            c(&"a".repeat(MAX_TEXT_CHARS + 50)).chars().count(),
            MAX_TEXT_CHARS
        );
    }

    #[test]
    fn a_title_is_one_short_line() {
        let t = |raw: &str| clean_output(DraftKind::Title, raw);
        assert_eq!(t("Título: \"Moodle 5.3.0\"."), "Moodle 5.3.0");
        assert_eq!(t("\n\n  Primeira linha  \nSegunda"), "Primeira linha");
        assert_eq!(t(""), "");
        let long = format!("{} fim", "palavra ".repeat(30));
        let cut = t(&long);
        assert!(cut.chars().count() <= MAX_TITLE_CHARS && cut.ends_with("palavra"));
        // Sem espaço para cortar, corta no limite.
        assert_eq!(
            t(&"x".repeat(MAX_TITLE_CHARS + 20)).chars().count(),
            MAX_TITLE_CHARS
        );
    }

    #[test]
    fn templates_give_each_text_a_skeleton_with_placeholders() {
        let tk = ticket(vec![]);
        for kind in ALL {
            let empty = template(kind, &tk, None);
            assert!(!empty.is_empty());
            if kind != DraftKind::Title {
                assert!(empty.contains(PLACEHOLDER), "{kind:?}");
            }
            let said = template(kind, &tk, Some("a  fila\nparou"));
            assert!(said.contains("a fila parou"), "{kind:?}");
        }
        // O título parte do tipo do chamado quando a pessoa não disse nada (ou só espaços).
        assert_eq!(
            template(DraftKind::Title, &tk, None),
            "Atualização de versão"
        );
        assert_eq!(
            template(DraftKind::Title, &tk, Some("  ")),
            "Atualização de versão"
        );
        let long = "p ".repeat(200);
        assert!(template(DraftKind::Title, &tk, Some(&long)).chars().count() <= MAX_TITLE_CHARS);
        assert_eq!(
            template(DraftKind::Cancel, &tk, Some("duplicado")),
            "Chamado cancelado: duplicado."
        );
    }

    // 4000 tokens, a 3 caracteres por token em português: o orçamento deixa folga para a resposta.
    const _: () = assert!(DEFAULT_BUDGET_CHARS < 4000 * 3);
}
