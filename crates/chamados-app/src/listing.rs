//! Filtros da listagem que valem só no aparelho (os títulos, o arquivo e os adiamentos nunca saem
//! dele em claro).

use chamados_core::{watch::WatchEvent, ArchiveStore, RemoteTicket, SnoozeStore, TitleStore};

/// Which tickets a list shows, as for what the user archived or snoozed on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListView {
    /// The usual lists: no archived ticket and no ticket still snoozed; the ones that came back from
    /// a snooze come first.
    Normal,
    /// Only the archived tickets.
    Archived,
    /// Only the tickets still snoozed (not archived), soonest to come back first.
    Snoozed,
}

/// The marks the user keeps on tickets on this machine, as of `now` (Unix ms).
#[derive(Debug, Clone, Copy)]
pub struct Marks<'a> {
    pub archive: &'a ArchiveStore,
    pub snooze: &'a SnoozeStore,
    pub now: u64,
}

impl Marks<'_> {
    /// Whether the ticket stays out of the usual lists and out of the watch.
    pub fn hidden(&self, id: &str) -> bool {
        self.archive.is_archived(id) || self.snooze.is_pending(id, self.now)
    }

    /// Whether the ticket came back from a snooze and nobody has seen it since.
    pub fn returned(&self, id: &str) -> bool {
        !self.archive.is_archived(id)
            && self.snooze.until(id).is_some_and(|until| until <= self.now)
    }
}

/// Keeps the tickets of `view` whose local title contains `needle` (ignoring case; every ticket when
/// it is `None`) and then at most `limit` of them.
pub fn narrow(
    tickets: Vec<RemoteTicket>,
    titles: &TitleStore,
    marks: &Marks<'_>,
    view: ListView,
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
    let shown = |id: &str| match view {
        ListView::Normal => !marks.hidden(id),
        ListView::Archived => marks.archive.is_archived(id),
        ListView::Snoozed => {
            !marks.archive.is_archived(id) && marks.snooze.is_pending(id, marks.now)
        }
    };
    let mut kept: Vec<RemoteTicket> = tickets
        .into_iter()
        .filter(|ticket| shown(&ticket.id) && matches(&ticket.id))
        .collect();
    match view {
        // Back on top, like a new arrival (the sort is stable: the rest keeps SUAP's order).
        ListView::Normal => kept.sort_by_key(|ticket| !marks.returned(&ticket.id)),
        ListView::Snoozed => kept.sort_by_key(|ticket| marks.snooze.until(&ticket.id)),
        ListView::Archived => {}
    }
    kept.truncate(limit.unwrap_or(usize::MAX));
    kept
}

/// The watch events of tickets that are not archived or snoozed: they are not announced (their state
/// is still recorded, so bringing them back does not bring a flood).
pub fn without_hidden(events: Vec<WatchEvent>, marks: &Marks<'_>) -> Vec<WatchEvent> {
    let kept = events.into_iter();
    kept.filter(|event| !marks.hidden(event.id())).collect()
}
