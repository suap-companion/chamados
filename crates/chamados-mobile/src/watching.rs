//! Acompanhamento em segundo plano: uma rodada do `watch`, com os eventos prontos para notificar.

use chamados_app::watching::watch_once;
use chamados_core::{watch::WatchEvent, SuapTicketSource, TicketFilter, TitleStore};

use crate::{failed, Chamados, ChamadosError, Queue};

/// Uma mudança que o app deve avisar. `kind`: `new`, `status`, `left` ou `message`.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct WatchItem {
    pub id: String,
    pub kind: String,
    /// O título local do chamado, se tiver.
    pub title: Option<String>,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub text: Option<String>,
}

fn item_of(event: WatchEvent, title: Option<String>) -> WatchItem {
    let mut item = WatchItem {
        id: event.id().to_owned(),
        kind: event.kind().to_owned(),
        title,
        subject: None,
        from: None,
        to: None,
        text: None,
    };
    match event {
        WatchEvent::New {
            status, subject, ..
        } => {
            item.to = status;
            item.subject = subject;
        }
        WatchEvent::Status { from, to, .. } => {
            item.from = from;
            item.to = to;
        }
        WatchEvent::Left { status, .. } => item.from = status,
        WatchEvent::Message { text, .. } => item.text = Some(text),
    }
    item
}

fn scope_of(queue: Queue, all: bool) -> String {
    let name = match queue {
        Queue::Support => "support",
        Queue::Mine => "mine",
        Queue::ToClose => "close",
    };
    format!("{name}:{}", if all { "all" } else { "active" })
}

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// Olha a lista uma vez e devolve o que mudou desde a última olhada (a primeira só registra o
    /// estado). Só lê: nada é escrito no SUAP. Com `messages`, também avisa de mensagens novas
    /// (lê a página de cada chamado, então é mais pesado).
    pub async fn watch_once(
        &self,
        queue: Queue,
        all: bool,
        messages: bool,
    ) -> Result<Vec<WatchItem>, ChamadosError> {
        let filter = TicketFilter {
            all_statuses: all,
            all_pages: true,
            ..TicketFilter::default()
        };
        let source = SuapTicketSource::new(&self.client, queue.core());
        let scope = scope_of(queue, all);
        let state_file = self.paths.watch_file();
        let events = watch_once(&source, &filter, &scope, messages, &state_file).await;
        let titles = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        Ok(events
            .map_err(failed)?
            .into_iter()
            .map(|event| {
                let title = titles.get(event.id()).map(str::to_owned);
                item_of(event, title)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn boxes(tickets: &[(u32, &str)]) -> String {
        tickets
            .iter()
            .map(|(id, status)| {
                format!(
                    r#"<div class="general-box"><span class="status">{status}</span>
                    <h4><a href="/centralservicos/chamado/{id}/">REQ #{id} <strong>Assunto {id}</strong></a></h4></div>"#
                )
            })
            .collect()
    }

    async fn serve(server: &MockServer, body: String) {
        server.reset().await;
        Mock::given(method("GET"))
            .and(path("/centralservicos/meus_chamados/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }

    #[test]
    fn every_kind_of_event_becomes_an_item() {
        let id = || "5".to_owned();
        let some = |text: &str| Some(text.to_owned());
        let item = |event| item_of(event, some("Meu título"));
        let new = item(WatchEvent::New {
            id: id(),
            status: some("Aberto"),
            subject: some("Rede"),
        });
        assert_eq!(
            (new.kind.as_str(), new.to, new.subject),
            ("new", some("Aberto"), some("Rede"))
        );
        assert_eq!(new.title, some("Meu título"));
        let status = item(WatchEvent::Status {
            id: id(),
            from: some("Aberto"),
            to: some("Resolvido"),
        });
        assert_eq!(
            (status.kind.as_str(), status.from, status.to),
            ("status", some("Aberto"), some("Resolvido"))
        );
        let left = item(WatchEvent::Left {
            id: id(),
            status: some("Fechado"),
        });
        assert_eq!((left.kind.as_str(), left.from), ("left", some("Fechado")));
        let message = item_of(
            WatchEvent::Message {
                id: id(),
                date: "hoje".to_owned(),
                text: "Olá".to_owned(),
            },
            None,
        );
        assert_eq!(
            (message.kind.as_str(), message.text, message.title),
            ("message", some("Olá"), None)
        );
    }

    #[test]
    fn each_list_is_a_baseline_of_its_own() {
        assert_eq!(scope_of(Queue::Mine, false), "mine:active");
        assert_eq!(scope_of(Queue::Support, true), "support:all");
        assert_eq!(scope_of(Queue::ToClose, false), "close:active");
    }

    #[tokio::test]
    async fn the_first_look_only_records_and_the_next_ones_report_what_changed() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_str().unwrap().to_owned();
        let chamados = Chamados::new(data, format!("{}/", server.uri())).unwrap();
        chamados
            .set_title("7".to_owned(), "Meu título".to_owned())
            .unwrap();

        serve(&server, boxes(&[(7, "Aberto"), (8, "Aberto")])).await;
        assert!(chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .unwrap()
            .is_empty());
        assert!(chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .unwrap()
            .is_empty());

        serve(&server, boxes(&[(7, "Resolvido"), (9, "Aberto")])).await;
        let items = chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .unwrap();
        let kinds: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.kind.as_str(), i.id.as_str()))
            .collect();
        assert_eq!(kinds, [("status", "7"), ("new", "9"), ("left", "8")]);
        let status = items.iter().find(|i| i.kind == "status").unwrap();
        assert_eq!(status.title.as_deref(), Some("Meu título"));
        assert_eq!(status.to.as_deref(), Some("Resolvido"));

        // A failed look (here: SUAP out) is an error the app can show, and keeps the last state.
        server.reset().await;
        assert!(chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .is_err());
        serve(&server, boxes(&[(7, "Resolvido"), (9, "Aberto")])).await;
        assert!(chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .unwrap()
            .is_empty());

        // The titles file is user data: if it is damaged the round fails instead of hiding it.
        std::fs::write(chamados.paths.titles_file(), "{").unwrap();
        assert!(chamados
            .watch_once(Queue::Mine, false, false)
            .await
            .is_err());
    }
}
