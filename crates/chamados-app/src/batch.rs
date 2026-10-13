//! O mesmo comentário ou nota em vários chamados, sem que um que recusa pare os outros.

use chamados_core::{Message, SuapTicketSource, TicketError, TicketSource};
use suap_core::SuapError;

/// What happened to each ticket of a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchReport {
    /// Tickets that got the message, in order.
    pub sent: Vec<String>,
    /// Tickets that refused it, with the reason, in order.
    pub failed: Vec<(String, String)>,
}

/// Sends `text` as `kind` to each ticket (repeats count once). A ticket that refuses does not stop
/// the others. Without a session nothing can work, so that stops at the first ticket as an error.
pub async fn send_messages(
    source: &SuapTicketSource<'_>,
    ids: &[u64],
    kind: Message,
    text: &str,
) -> Result<BatchReport, TicketError> {
    let mut seen = Vec::new();
    let mut report = BatchReport::default();
    for id in ids {
        if seen.contains(id) {
            continue;
        }
        seen.push(*id);
        let id = id.to_string();
        match source.add_message(&id, kind, text).await {
            Ok(()) => report.sent.push(id),
            Err(error @ TicketError::Suap(SuapError::NotAuthenticated)) => return Err(error),
            Err(error) => report.failed.push((id, error.to_string())),
        }
    }
    Ok(report)
}
