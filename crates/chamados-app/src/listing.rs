//! Filtros da listagem que valem só no aparelho (os títulos locais nunca saem dele).

use chamados_core::{watch::WatchEvent, ArchiveStore, RemoteTicket, TitleStore};

/// Which tickets a list shows, as for what the user archived on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveView {
    /// Everything but the archived tickets (the usual lists).
    Hide,
    /// Only the archived tickets.
    Only,
}

/// Keeps the tickets of `view` whose local title contains `needle` (ignoring case; every ticket when
/// it is `None`) and then at most `limit` of them.
pub fn narrow(
    tickets: Vec<RemoteTicket>,
    titles: &TitleStore,
    archive: &ArchiveStore,
    view: ArchiveView,
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
        .filter(|ticket| (view == ArchiveView::Only) == archive.is_archived(&ticket.id))
        .filter(|ticket| matches(&ticket.id))
        .take(limit.unwrap_or(usize::MAX))
        .collect()
}

/// The watch events of tickets that are not archived: an archived ticket is not announced (its state
/// is still recorded, so unarchiving it does not bring a flood).
pub fn without_archived(events: Vec<WatchEvent>, archive: &ArchiveStore) -> Vec<WatchEvent> {
    let kept = events.into_iter();
    kept.filter(|event| !archive.is_archived(event.id()))
        .collect()
}
