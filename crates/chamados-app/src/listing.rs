//! Filtros da listagem que valem só no aparelho (os títulos locais nunca saem dele).

use chamados_core::{RemoteTicket, TitleStore};

/// Keeps the tickets whose local title contains `needle` (ignoring case; every ticket when it is
/// `None`) and then at most `limit` of them.
pub fn narrow(
    tickets: Vec<RemoteTicket>,
    titles: &TitleStore,
    needle: Option<&str>,
    limit: Option<usize>,
) -> Vec<RemoteTicket> {
    let needle = needle.map(str::to_lowercase);
    let matches = |id: &str| {
        needle.as_ref().is_none_or(|needle| {
            let title = titles.get(id).unwrap_or_default();
            title.to_lowercase().contains(needle.as_str())
        })
    };
    tickets
        .into_iter()
        .filter(|ticket| matches(&ticket.id))
        .take(limit.unwrap_or(usize::MAX))
        .collect()
}
