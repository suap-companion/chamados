//! Adiados e arquivados: a lista deles vem dos ids marcados no aparelho, sem depender da paginação da
//! fila do SUAP (a fila de suporte com todas as situações tem centenas de páginas).

use chamados_app::listing::{ListView, Marks};
use chamados_core::{ArchiveStore, RemoteTicket, SnoozeStore, SuapTicketSource, TicketSource};

use crate::{
    failed, marked_ids, sync::unix_now, Chamados, ChamadosError, ListQuery, LOOKUP_TTL_MS,
    MAX_MARKED_LOOKUPS,
};

impl Chamados {
    /// Os chamados adiados (ainda esperando) ou arquivados, de uma vez: os que a lista da fila traz (só
    /// com as situações de sempre, poucas páginas) e, para os que ela não traz, o chamado lido um a um
    /// (até [`MAX_MARKED_LOOKUPS`] por vez; os que não puderem ser lidos ficam de fora).
    pub(crate) async fn marked_tickets(
        &self,
        source: &SuapTicketSource<'_>,
        query: &ListQuery,
        view: ListView,
    ) -> Result<Vec<RemoteTicket>, ChamadosError> {
        // Tudo veio de uma vez: as páginas seguintes não têm nada.
        if query.page.is_some_and(|page| page > 1) {
            return Ok(Vec::new());
        }
        let now = unix_now();
        let archive = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let snooze = SnoozeStore::open(self.paths.snooze_file()).map_err(failed)?;
        let marks = Marks {
            archive: &archive,
            snooze: &snooze,
            now,
        };
        let (snoozed, archived) = marked_ids(&marks);
        let ids = if view == ListView::Snoozed {
            snoozed
        } else {
            archived
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut filter = query.clone().core();
        filter.page = None;
        filter.all_pages = true;
        filter.all_statuses = false;
        let listed = source.list_filtered(&filter).await.map_err(failed)?;
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        for id in ids {
            match listed.iter().find(|ticket| ticket.id == id) {
                Some(ticket) => found.push(ticket.clone()),
                None => missing.push(id),
            }
        }
        for id in missing.into_iter().take(MAX_MARKED_LOOKUPS) {
            found.extend(self.look_up(source, &id, now).await);
        }
        Ok(found)
    }

    /// Lê um chamado pela página dele (com uma memória de alguns minutos); `None` se não der.
    async fn look_up(
        &self,
        source: &SuapTicketSource<'_>,
        id: &str,
        now: u64,
    ) -> Option<RemoteTicket> {
        if let Some((read_at, ticket)) = self.looked_up.lock().ok()?.get(id) {
            if now.saturating_sub(*read_at) < LOOKUP_TTL_MS {
                return Some(ticket.clone());
            }
        }
        let details = source.get_ticket(id).await.ok()?;
        // A lista mostra só o assunto: a última parte do cabeçalho ("Serviço | Assunto").
        let heading = details.heading.as_deref();
        let subject = heading.and_then(|text| text.rsplit(" | ").next());
        let ticket = RemoteTicket {
            id: details.id,
            subject: Some(subject.unwrap_or(&details.title).to_owned()),
            status: details.statuses.first().cloned(),
            details_url: details.details_url,
            photo: details.interested_photo,
        };
        self.looked_up
            .lock()
            .ok()?
            .insert(id.to_owned(), (now, ticket.clone()));
        Some(ticket)
    }
}
