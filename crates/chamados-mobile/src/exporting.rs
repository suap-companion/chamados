//! Exportar um chamado como ZIP (texto, dados e anexos), para o app entregar por "Compartilhar".

use std::{fs, path::Path};

use chamados_app::export::{export_file_name, export_ticket, ExportInfo};
use chamados_core::{ArchiveStore, SuapTicketSource, TicketQueue, TicketSource, TitleStore};

use crate::{failed, sync::unix_now, Chamados, ChamadosError};

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// Grava em `dest_dir` o ZIP `chamado-<número>.zip` do chamado (`chamado.md`, `chamado.json` e,
    /// com `attachments`, a pasta `anexos/`) e devolve o caminho do arquivo. Só lê o SUAP.
    pub async fn export_ticket(
        &self,
        id: String,
        attachments: bool,
        dest_dir: String,
    ) -> Result<String, ChamadosError> {
        let source = SuapTicketSource::new(&self.client, TicketQueue::Support);
        let details = source.get_ticket(&id).await.map_err(failed)?;
        let titles = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let archive = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let info = ExportInfo {
            local_title: titles.get(&details.id),
            archived: archive.is_archived(&details.id),
            exported_at: unix_now() / 1_000,
        };
        let zip = export_ticket(&source, &details, &info, attachments).await;
        let zip = zip.map_err(failed)?;
        fs::create_dir_all(&dest_dir).map_err(failed)?;
        let target = Path::new(&dest_dir).join(export_file_name(&details));
        fs::write(&target, zip).map_err(failed)?;
        Ok(target.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn the_zip_is_written_where_the_app_asks() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("dados").to_str().unwrap().to_owned();
        let chamados = Chamados::new(data, format!("{}/", server.uri())).unwrap();
        let folder = dir.path().join("exportar").to_str().unwrap().to_owned();
        assert!(chamados
            .export_ticket("7".to_owned(), false, folder.clone())
            .await
            .is_err());

        let page =
            r#"<main id="content"><div class="title-container"><h2>Chamado 7</h2></div></main>"#;
        Mock::given(method("GET"))
            .and(path("/centralservicos/chamado/7/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(page))
            .mount(&server)
            .await;
        chamados
            .set_title("7".to_owned(), "Meu título".to_owned())
            .unwrap();
        chamados.set_archived("7".to_owned(), true).unwrap();
        let written = chamados
            .export_ticket("7".to_owned(), true, folder.clone())
            .await
            .unwrap();
        assert!(written.ends_with("chamado-7.zip"), "{written}");
        let bytes = std::fs::read(&written).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(bytes.starts_with(b"PK\x03\x04") && text.contains("Meu título"));

        // The destination must be a folder it can create.
        let blocked = dir.path().join("arquivo");
        std::fs::write(&blocked, "x").unwrap();
        let under_a_file = blocked.join("dentro").to_str().unwrap().to_owned();
        assert!(chamados
            .export_ticket("7".to_owned(), false, under_a_file)
            .await
            .is_err());
        // A damaged titles or archive file stops the export instead of hiding user data.
        std::fs::write(chamados.paths.archive_file(), "{").unwrap();
        assert!(chamados
            .export_ticket("7".to_owned(), false, folder.clone())
            .await
            .is_err());
        std::fs::remove_file(chamados.paths.archive_file()).unwrap();
        std::fs::write(chamados.paths.titles_file(), "{").unwrap();
        assert!(chamados
            .export_ticket("7".to_owned(), false, folder)
            .await
            .is_err());
    }
}
