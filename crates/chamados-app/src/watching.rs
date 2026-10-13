//! Uma rodada do acompanhamento: consulta a lista, compara com a anterior, guarda o estado.

use std::path::Path;

use chamados_core::{
    watch::{poll, Snapshot, WatchEvent},
    SuapTicketSource, TicketError, TicketFilter,
};

/// Looks at the list `filter` asks for once, saving what it saw in `state_file` (it is read first, so
/// the next round compares with this one). The first look of a list only records its state.
pub async fn watch_once(
    source: &SuapTicketSource<'_>,
    filter: &TicketFilter,
    scope: &str,
    messages: bool,
    state_file: &Path,
) -> Result<Vec<WatchEvent>, TicketError> {
    let previous = Snapshot::load(state_file)?;
    let (snapshot, events) = poll(source, filter, scope, messages, previous.as_ref()).await?;
    snapshot.save(state_file)?;
    Ok(events)
}
