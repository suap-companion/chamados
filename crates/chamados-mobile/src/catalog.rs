//! O catálogo de serviços no aplicativo: o passo "Serviço" de abrir um chamado busca por nome no que
//! ficou guardado no aparelho (uma leitura de todas as áreas do SUAP só acontece quando a pessoa pede
//! para atualizar) e lista os campus e centros de atendimento do serviço escolhido.

use chamados_core::catalog::{
    catalog_file, fetch_campuses, fetch_catalog, fetch_centers, fetch_service_fields, Catalog,
    Place, ServiceEntry,
};
use chamados_core::manage::search_people;

use crate::{failed, sync::unix_now, Chamados, ChamadosError};

/// Um serviço do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServiceItem {
    /// O número do serviço no SUAP.
    pub id: u64,
    pub name: String,
    pub area: String,
    pub category: String,
    pub group: String,
    /// Um incidente (algo parou de funcionar); senão, uma solicitação.
    pub incident: bool,
    /// Só a equipe de atendimento abre.
    pub internal: bool,
}

impl From<&ServiceEntry> for ServiceItem {
    fn from(service: &ServiceEntry) -> Self {
        Self {
            id: service.id,
            name: service.name.clone(),
            area: service.area.clone(),
            category: service.category.clone(),
            group: service.group.clone(),
            incident: service.incident,
            internal: service.internal,
        }
    }
}

/// O que há guardado do catálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct CatalogInfo {
    /// Quantos serviços.
    pub count: u32,
    /// Quando foi lido do SUAP (Unix ms); `None` se nunca foi lido.
    pub fetched_at: Option<u64>,
}

/// Um campus ou centro de atendimento.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlaceItem {
    pub id: String,
    pub name: String,
    /// O campus da pessoa, ou um centro local a ele (o que vem escolhido).
    pub preferred: bool,
}

impl From<Place> for PlaceItem {
    fn from(place: Place) -> Self {
        Self {
            id: place.id,
            name: place.name,
            preferred: place.preferred,
        }
    }
}

/// Uma pessoa achada na busca do SUAP.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PersonItem {
    /// O número do vínculo (o que se informa como interessado).
    pub id: String,
    pub name: String,
}

/// Um campo a mais que o formulário do serviço pede (ex.: patrimônio).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServiceFieldItem {
    pub name: String,
    pub label: String,
    pub required: bool,
    pub value: String,
}

fn info(catalog: Option<&Catalog>) -> CatalogInfo {
    CatalogInfo {
        count: catalog.map_or(0, |catalog| catalog.services.len() as u32),
        fetched_at: catalog.map(|catalog| catalog.fetched_at),
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// Quantos serviços há guardados e de quando.
    pub fn catalog_info(&self) -> Result<CatalogInfo, ChamadosError> {
        let catalog = Catalog::load(&catalog_file(&self.paths)).map_err(failed)?;
        Ok(info(catalog.as_ref()))
    }

    /// Os serviços guardados que têm todas as palavras de `query` (sem diferenciar maiúsculas e
    /// acentos; um número também vale), pelo nome. Sem catálogo guardado, a lista vem vazia.
    pub fn catalog_search(
        &self,
        query: String,
        limit: u32,
    ) -> Result<Vec<ServiceItem>, ChamadosError> {
        let catalog = Catalog::load(&catalog_file(&self.paths)).map_err(failed)?;
        Ok(catalog.map_or_else(Vec::new, |catalog| {
            catalog
                .search(&query, limit as usize)
                .into_iter()
                .map(ServiceItem::from)
                .collect()
        }))
    }

    /// O serviço guardado com este número.
    pub fn catalog_find(&self, id: u64) -> Result<Option<ServiceItem>, ChamadosError> {
        let catalog = Catalog::load(&catalog_file(&self.paths)).map_err(failed)?;
        Ok(catalog.and_then(|catalog| catalog.find(id).map(ServiceItem::from)))
    }

    /// Lê de novo o catálogo no SUAP (percorre as áreas de serviço) e guarda o resultado.
    pub async fn catalog_refresh(&self) -> Result<CatalogInfo, ChamadosError> {
        let catalog = fetch_catalog(&self.client, unix_now())
            .await
            .map_err(failed)?;
        catalog.save(&catalog_file(&self.paths)).map_err(failed)?;
        Ok(info(Some(&catalog)))
    }

    /// Os campus em que o serviço é oferecido (o da pessoa vem como preferido).
    pub async fn service_campuses(&self, service_id: u64) -> Result<Vec<PlaceItem>, ChamadosError> {
        let places = fetch_campuses(&self.client, service_id)
            .await
            .map_err(failed)?;
        Ok(places.into_iter().map(PlaceItem::from).collect())
    }

    /// Busca pessoas por matrícula ou nome, com a busca do formulário do serviço (a mesma de escolher o
    /// interessado no site). O `id` de cada uma é o que vai como interessado ao abrir o chamado.
    pub async fn search_people(
        &self,
        service_id: u64,
        query: String,
    ) -> Result<Vec<PersonItem>, ChamadosError> {
        let found = search_people(&self.client, service_id, &query)
            .await
            .map_err(failed)?;
        Ok(found
            .into_iter()
            .map(|person| PersonItem {
                id: person.value,
                name: person.label,
            })
            .collect())
    }

    /// Os campos a mais que o formulário do serviço pede, além dos de sempre.
    pub async fn service_fields(
        &self,
        service_id: u64,
    ) -> Result<Vec<ServiceFieldItem>, ChamadosError> {
        let fields = fetch_service_fields(&self.client, service_id)
            .await
            .map_err(failed)?;
        Ok(fields
            .into_iter()
            .map(|field| ServiceFieldItem {
                name: field.name,
                label: field.label,
                required: field.required,
                value: field.value,
            })
            .collect())
    }

    /// Os centros de atendimento do serviço no campus.
    pub async fn service_centers(
        &self,
        service_id: u64,
        campus: String,
    ) -> Result<Vec<PlaceItem>, ChamadosError> {
        let places = fetch_centers(&self.client, service_id, &campus)
            .await
            .map_err(failed)?;
        Ok(places.into_iter().map(PlaceItem::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::*;

    async fn page(server: &MockServer, at: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(at.to_owned()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_owned()))
            .mount(server)
            .await;
    }

    fn app(server: &MockServer) -> (tempfile::TempDir, Arc<Chamados>) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_str().unwrap().to_owned();
        let chamados = Chamados::new(data, format!("{}/", server.uri())).unwrap();
        (dir, chamados)
    }

    const AREAS: &str = r#"<div class="boxes links"><a href="/centralservicos/selecionar_servico_abertura/ti/" class="box-item"><h4>TI</h4></a></div>"#;
    const AREA: &str = r#"<div class="tab-pane" data-title="Sistemas"><ul><li class="box-item"><h4>Moodle</h4><ol class="numbered-list">
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/400/"><span class="fa-fire"></span> Infraestrutura para o Moodle</a></li>
        <li class="list-item warning"><a href="/centralservicos/visualizar_solucoes/401/">Backup do Moodle (Interno)</a></li>
    </ol></li></ul></div>"#;

    #[tokio::test]
    async fn the_catalog_is_read_on_request_and_searched_from_the_device() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        // Nothing kept yet: empty answers, without touching the network.
        assert_eq!(
            chamados.catalog_info().unwrap(),
            CatalogInfo {
                count: 0,
                fetched_at: None
            }
        );
        assert!(chamados
            .catalog_search("moodle".to_owned(), 10)
            .unwrap()
            .is_empty());
        assert_eq!(chamados.catalog_find(400).unwrap(), None);

        page(&server, "/centralservicos/listar_area_servico/", AREAS).await;
        page(
            &server,
            "/centralservicos/selecionar_servico_abertura/ti/",
            AREA,
        )
        .await;
        let read = chamados.catalog_refresh().await.unwrap();
        assert_eq!(read.count, 2);
        assert!(read.fetched_at.is_some());
        assert_eq!(chamados.catalog_info().unwrap(), read);

        let found = chamados
            .catalog_search("moodle infra".to_owned(), 10)
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].id, found[0].incident, found[0].internal),
            (400, true, false)
        );
        assert_eq!(found[0].group, "Moodle");
        let internal = chamados.catalog_find(401).unwrap().unwrap();
        assert!(internal.internal);
        assert_eq!(internal.name, "Backup do Moodle");
        assert_eq!(chamados.catalog_search("".to_owned(), 1).unwrap().len(), 1);

        // A damaged file is reported, not ignored.
        std::fs::write(catalog_file(&chamados.paths), "{").unwrap();
        assert!(chamados.catalog_info().is_err());
        assert!(chamados.catalog_search("a".to_owned(), 1).is_err());
        assert!(chamados.catalog_find(1).is_err());
    }

    #[tokio::test]
    async fn the_refresh_fails_when_the_areas_cannot_be_read() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        assert!(chamados.catalog_refresh().await.is_err());
        // The folder of the file is gone: the result cannot be kept.
        page(&server, "/centralservicos/listar_area_servico/", AREAS).await;
        page(
            &server,
            "/centralservicos/selecionar_servico_abertura/ti/",
            AREA,
        )
        .await;
        std::fs::remove_dir_all(chamados.paths.data_dir()).unwrap();
        assert!(chamados.catalog_refresh().await.is_err());
    }

    #[tokio::test]
    async fn campuses_and_centers_of_a_service_come_from_suap() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        page(
            &server,
            "/centralservicos/get_campus_com_centros_atendimento/400/0/",
            r#"{"campus":[[1,"RE",false],[2,"ZL",true]]}"#,
        )
        .await;
        page(
            &server,
            "/centralservicos/get_centros_atendimento_por_servico_e_campus/400/2/",
            r#"{"centros":[[5,"Central DEAD",true]]}"#,
        )
        .await;
        let campuses = chamados.service_campuses(400).await.unwrap();
        assert_eq!(campuses.len(), 2);
        assert!(campuses[1].preferred && campuses[1].id == "2");
        let centers = chamados.service_centers(400, "2".to_owned()).await.unwrap();
        assert_eq!(centers[0].name, "Central DEAD");
        assert!(chamados.service_centers(400, "9".to_owned()).await.is_err());
        assert!(chamados.service_campuses(1).await.is_err());
    }

    #[tokio::test]
    async fn the_extra_fields_of_a_service_come_from_its_form() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        page(
            &server,
            "/centralservicos/abrir_chamado/400/",
            r#"<form><textarea name="descricao"></textarea><label for="p">Patrimônio *</label><input id="p" name="patrimonio" required></form>"#,
        )
        .await;
        let fields = chamados.service_fields(400).await.unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(
            (
                fields[0].name.as_str(),
                fields[0].label.as_str(),
                fields[0].required
            ),
            ("patrimonio", "Patrimônio", true)
        );
        assert!(chamados.service_fields(9).await.is_err());
    }

    #[tokio::test]
    async fn people_are_searched_with_the_form_of_the_service() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        page(
            &server,
            "/centralservicos/abrir_chamado/400/",
            r#"<form><script>data: { q: params.term, control: 'abc' }</script></form>"#,
        )
        .await;
        page(
            &server,
            "/json/comum/vinculo/",
            r#"{"items": [{"id": 38349, "html": "<dd class=\"title\">Wagner (Mat. 1723011)</dd>"}]}"#,
        )
        .await;
        let found = chamados
            .search_people(400, "wagner".to_owned())
            .await
            .unwrap();
        assert_eq!(
            found,
            vec![PersonItem {
                id: "38349".to_owned(),
                name: "Wagner (Mat. 1723011)".to_owned()
            }]
        );
        assert!(chamados.search_people(9, "x".to_owned()).await.is_err());
    }
}
