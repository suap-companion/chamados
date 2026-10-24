//! The catalogue of services a person can open tickets for.
//!
//! SUAP has no list of services: it is reached by walking the service areas
//! (`/centralservicos/listar_area_servico/`) and, inside each, its categories and service groups
//! (`/centralservicos/selecionar_servico_abertura/<area>/`). Reading it takes one request per area,
//! so the result is kept in a JSON file per profile and read again only when asked for.

use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
};

use scraper::Html;
use serde::{Deserialize, Serialize};
use suap_core::{AppPaths, SuapClient, DEFAULT_PROFILE};

use crate::{
    flat_text, selector, CampusReply, CentersReply, TicketError, CAMPUS_PATH_PREFIX,
    CENTERS_PATH_PREFIX,
};

const FORMAT_VERSION: u32 = 1;
const AREAS_PATH: &str = "/centralservicos/listar_area_servico/";
const AREA_PATH_PREFIX: &str = "/centralservicos/selecionar_servico_abertura/";
const SOLUTIONS_MARK: &str = "visualizar_solucoes/";
/// Most areas read in one go (a guard against a page that links itself over and over).
const MAX_AREAS: usize = 100;

/// A service area (a box on SUAP's "open a ticket" page).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    pub slug: String,
    pub name: String,
}

/// A service a ticket can be opened for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// The id used in `/centralservicos/abrir_chamado/<id>/`.
    pub id: u64,
    pub name: String,
    pub area: String,
    pub category: String,
    pub group: String,
    /// An incident (SUAP draws a flame); otherwise a request.
    pub incident: bool,
    /// Offered to the service team only.
    pub internal: bool,
}

/// A campus or a service center a service is offered at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub id: String,
    pub name: String,
    /// The user's own campus, or a center that is local to it.
    pub preferred: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CatalogFile {
    version: u32,
    fetched_at: u64,
    services: Vec<ServiceEntry>,
}

/// The services read from SUAP and when (Unix time, ms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub fetched_at: u64,
    pub services: Vec<ServiceEntry>,
}

/// The catalogue file of the selected profile (`services.json` for `default`, else `services-<profile>.json`).
pub fn catalog_file(paths: &AppPaths) -> PathBuf {
    if paths.profile() == DEFAULT_PROFILE {
        return paths.data_dir().join("services.json");
    }
    paths
        .data_dir()
        .join(format!("services-{}.json", paths.profile()))
}

fn file_error(path: &Path, error: impl Display) -> TicketError {
    TicketError::Source(format!("catalog file {}: {error}", path.display()))
}

impl Catalog {
    /// Loads the kept catalogue; `None` when it was never read. A damaged file is an error.
    pub fn load(path: &Path) -> Result<Option<Self>, TicketError> {
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path).map_err(|error| file_error(path, error))?;
        let file: CatalogFile =
            serde_json::from_str(&content).map_err(|error| file_error(path, error))?;
        if file.version != FORMAT_VERSION {
            let message = format!("unsupported format version {}", file.version);
            return Err(file_error(path, message));
        }
        Ok(Some(Self {
            fetched_at: file.fetched_at,
            services: file.services,
        }))
    }

    /// Keeps the catalogue, replacing the file in one step.
    pub fn save(&self, path: &Path) -> Result<(), TicketError> {
        let file = CatalogFile {
            version: FORMAT_VERSION,
            fetched_at: self.fetched_at,
            services: self.services.clone(),
        };
        let content = serde_json::to_string(&file).expect("the catalog always serializes");
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, content).map_err(|error| file_error(path, error))?;
        fs::rename(&temporary, path).map_err(|error| file_error(path, error))
    }

    /// The service with this id.
    pub fn find(&self, id: u64) -> Option<&ServiceEntry> {
        self.services.iter().find(|service| service.id == id)
    }

    /// Services whose name, group, category or area has every word of `query` (ignoring case and
    /// accents), by name. A word that is only digits also matches the service number.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&ServiceEntry> {
        let words: Vec<String> = query.split_whitespace().map(fold).collect();
        let mut found: Vec<&ServiceEntry> = self
            .services
            .iter()
            .filter(|service| {
                let text = fold(&format!(
                    "{} {} {} {}",
                    service.name, service.group, service.category, service.area
                ));
                let number = service.id.to_string();
                words
                    .iter()
                    .all(|word| text.contains(word.as_str()) || number == *word)
            })
            .collect();
        found.sort_by(|a, b| fold(&a.name).cmp(&fold(&b.name)).then(a.id.cmp(&b.id)));
        found.truncate(limit);
        found
    }
}

/// Lowercase with the accents of Portuguese removed, so "informática" finds "INFORMATICA".
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// The service areas on SUAP's "open a ticket" page.
pub fn parse_areas(html: &str) -> Vec<Area> {
    let document = Html::parse_document(html);
    let mut areas: Vec<Area> = Vec::new();
    for link in document.select(&selector(".boxes.links a.box-item")) {
        let slug = link
            .value()
            .attr("href")
            .and_then(|href| href.trim_end_matches('/').rsplit('/').next())
            .unwrap_or_default();
        let name = link
            .select(&selector("h4"))
            .next()
            .map(flat_text)
            .unwrap_or_default();
        if !slug.is_empty() && !name.is_empty() && areas.iter().all(|area| area.slug != slug) {
            areas.push(Area {
                slug: slug.to_owned(),
                name,
            });
        }
    }
    areas
}

/// The services of one area page: each category tab, its service groups and their services.
pub fn parse_area_services(html: &str, area: &str) -> Vec<ServiceEntry> {
    let document = Html::parse_document(html);
    let mut services = Vec::new();
    for tab in document.select(&selector(".tab-pane")) {
        let category = tab.value().attr("data-title").unwrap_or_default();
        for group in tab.select(&selector("li.box-item")) {
            let group_name = group
                .select(&selector("h4"))
                .next()
                .map(flat_text)
                .unwrap_or_default();
            for item in group.select(&selector("ol.numbered-list li.list-item")) {
                let Some(link) = item.select(&selector("a")).next() else {
                    continue;
                };
                let id = link
                    .value()
                    .attr("href")
                    .and_then(|href| href.split(SOLUTIONS_MARK).nth(1))
                    .and_then(|rest| rest.trim_end_matches('/').parse::<u64>().ok());
                let Some(id) = id else {
                    continue;
                };
                let name = flat_text(link);
                services.push(ServiceEntry {
                    id,
                    name: name.trim_end_matches("(Interno)").trim().to_owned(),
                    area: area.to_owned(),
                    category: category.to_owned(),
                    group: group_name.clone(),
                    incident: link.select(&selector("span.fa-fire")).next().is_some(),
                    internal: item.value().classes().any(|class| class == "warning"),
                });
            }
        }
    }
    services
}

/// Walks the areas and their pages and builds the catalogue (a service listed in two areas is kept
/// once). `now` is the Unix time in ms to record.
pub async fn fetch_catalog(client: &SuapClient, now: u64) -> Result<Catalog, TicketError> {
    let areas = parse_areas(&client.fetch_page(AREAS_PATH).await?);
    if areas.is_empty() {
        return Err(TicketError::Source(
            "no service area found on the SUAP page".to_owned(),
        ));
    }
    let mut services: Vec<ServiceEntry> = Vec::new();
    for area in areas.iter().take(MAX_AREAS) {
        let path = format!("{AREA_PATH_PREFIX}{}/", area.slug);
        let html = client.fetch_page(&path).await?;
        for service in parse_area_services(&html, &area.name) {
            if services.iter().all(|known| known.id != service.id) {
                services.push(service);
            }
        }
    }
    Ok(Catalog {
        fetched_at: now,
        services,
    })
}

/// The campuses a service is offered at.
pub fn parse_campuses(json: &str) -> Result<Vec<Place>, TicketError> {
    let reply: CampusReply = serde_json::from_str(json)
        .map_err(|error| TicketError::Source(format!("campus list: {error}")))?;
    Ok(reply
        .campus
        .into_iter()
        .map(|(id, name, selected)| Place {
            id: id.to_string(),
            name,
            preferred: selected,
        })
        .collect())
}

/// The service centers a service has at a campus.
pub fn parse_centers(json: &str) -> Result<Vec<Place>, TicketError> {
    let reply: CentersReply = serde_json::from_str(json)
        .map_err(|error| TicketError::Source(format!("center list: {error}")))?;
    Ok(reply
        .centros
        .into_iter()
        .map(|(id, name, local)| Place {
            id: id.to_string(),
            name,
            preferred: local,
        })
        .collect())
}

/// Reads the campuses a service is offered at.
pub async fn fetch_campuses(client: &SuapClient, service: u64) -> Result<Vec<Place>, TicketError> {
    let json = client
        .fetch_page(&format!("{CAMPUS_PATH_PREFIX}{service}/0/"))
        .await?;
    parse_campuses(&json)
}

/// Reads the service centers a service has at a campus.
pub async fn fetch_centers(
    client: &SuapClient,
    service: u64,
    campus: &str,
) -> Result<Vec<Place>, TicketError> {
    let json = client
        .fetch_page(&format!("{CENTERS_PATH_PREFIX}{service}/{campus}/"))
        .await?;
    parse_centers(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const AREAS: &str = r#"<div class="boxes links flex-container">
        <a href="/centralservicos/selecionar_servico_abertura/ti/" class="box-item"><span class="fas fa-laptop"></span><h4>Tecnologia da Informação</h4></a>
        <a href="/centralservicos/selecionar_servico_abertura/ti/" class="box-item"><h4>Repetida</h4></a>
        <a href="/centralservicos/selecionar_servico_abertura/gestao/" class="box-item"><h4>Gestão</h4></a>
        <a href="/x/" class="box-item"><h4></h4></a>
        <a class="box-item"><h4>Sem endereço</h4></a>
        </div><a href="/fora/" class="box-item"><h4>Fora</h4></a>"#;

    const AREA_TI: &str = r#"
    <div class="tab-pane" data-title="Rede" data-tab="rede"><ul class="boxes services">
      <li class="box-item"><h4>Internet</h4><ol class="numbered-list">
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/12/"><span class="fas fa-fire"></span> Internet lenta </a></li>
        <li class="list-item warning"><a href="/centralservicos/visualizar_solucoes/13/"><span class="fas fa-comment"></span> Liberar porta (Interno) </a></li>
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/abc/">Sem número</a></li>
        <li class="list-item">Sem link</li>
      </ol></li>
    </ul></div>
    <div class="tab-pane" data-title="Sistemas"><ul><li class="box-item"><h4>Moodle</h4><ol class="numbered-list">
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/400/"><span class="fas fa-comment"></span> Infraestrutura para o Moodle</a></li>
    </ol></li></ul></div>"#;

    const AREA_GESTAO: &str = r#"<div class="tab-pane" data-title="Geral"><ul><li class="box-item"><h4>Outros</h4><ol class="numbered-list">
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/400/">Infraestrutura para o Moodle</a></li>
        <li class="list-item"><a href="/centralservicos/visualizar_solucoes/7/">Informática: Suporte Técnico</a></li>
    </ol></li></ul></div>"#;

    fn entry(id: u64, name: &str, group: &str) -> ServiceEntry {
        ServiceEntry {
            id,
            name: name.to_owned(),
            area: "TI".to_owned(),
            category: "Rede".to_owned(),
            group: group.to_owned(),
            incident: false,
            internal: false,
        }
    }

    fn catalog() -> Catalog {
        Catalog {
            fetched_at: 5,
            services: vec![
                entry(400, "Infraestrutura para o Moodle", "Moodle"),
                entry(12, "Internet lenta", "Internet"),
                entry(7, "Informática: Suporte Técnico", "Outros"),
            ],
        }
    }

    #[test]
    fn reads_the_areas_once_and_ignores_incomplete_boxes() {
        let names: Vec<_> = parse_areas(AREAS)
            .into_iter()
            .map(|area| (area.slug, area.name))
            .collect();
        assert_eq!(
            names,
            vec![
                ("ti".to_owned(), "Tecnologia da Informação".to_owned()),
                ("gestao".to_owned(), "Gestão".to_owned()),
            ]
        );
    }

    #[test]
    fn reads_categories_groups_and_services() {
        let services = parse_area_services(AREA_TI, "TI");
        let summary: Vec<_> = services
            .iter()
            .map(|s| (s.id, s.name.as_str(), s.category.as_str(), s.group.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (12, "Internet lenta", "Rede", "Internet"),
                (13, "Liberar porta", "Rede", "Internet"),
                (400, "Infraestrutura para o Moodle", "Sistemas", "Moodle"),
            ]
        );
        let flags: Vec<_> = services.iter().map(|s| (s.incident, s.internal)).collect();
        assert_eq!(flags, vec![(true, false), (false, true), (false, false)]);
        assert_eq!(services[0].area, "TI");
        assert!(parse_area_services("<p>nada</p>", "TI").is_empty());
    }

    #[test]
    fn searches_by_words_without_accents_or_by_number() {
        let catalog = catalog();
        let ids =
            |query: &str| -> Vec<u64> { catalog.search(query, 10).iter().map(|s| s.id).collect() };
        assert_eq!(ids("informatica"), vec![7]);
        assert_eq!(ids("MOODLE infra"), vec![400]);
        assert_eq!(ids("400"), vec![400]);
        assert_eq!(ids("lenta internet"), vec![12]);
        assert!(ids("inexistente").is_empty());
        // An empty query lists everything by name; the limit cuts the tail.
        assert_eq!(ids(""), vec![7, 400, 12]);
        assert_eq!(catalog.search("", 2).len(), 2);
        assert_eq!(catalog.find(12).unwrap().name, "Internet lenta");
        assert!(catalog.find(1).is_none());
    }

    #[test]
    fn folds_every_accent_of_portuguese() {
        assert_eq!(
            fold("ÁÀÂÃÄ ÉÈÊË ÍÌÎÏ ÓÒÔÕÖ ÚÙÛÜ Ç x"),
            "aaaaa eeee iiii ooooo uuuu c x"
        );
    }

    #[test]
    fn keeps_and_reloads_the_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("services.json");
        assert_eq!(Catalog::load(&file).unwrap(), None);
        catalog().save(&file).unwrap();
        assert_eq!(Catalog::load(&file).unwrap(), Some(catalog()));

        std::fs::write(&file, "{").unwrap();
        assert!(Catalog::load(&file).is_err());
        std::fs::write(&file, r#"{"version":9,"fetched_at":1,"services":[]}"#).unwrap();
        let message = Catalog::load(&file).unwrap_err().to_string();
        assert!(message.contains("version 9"));

        // A path that exists but cannot be read (here, a folder) is an error too.
        assert!(Catalog::load(dir.path()).is_err());

        // A file that cannot be written (its folder is missing) is an error.
        let missing = dir.path().join("falta").join("services.json");
        assert!(catalog().save(&missing).is_err());
        // The temporary file is written but the target is a directory: the rename fails.
        let target = dir.path().join("pasta");
        std::fs::create_dir(&target).unwrap();
        assert!(catalog().save(&target).is_err());
    }

    #[test]
    fn names_the_file_by_profile() {
        let paths = AppPaths::from_dirs("c".into(), "d".into());
        assert_eq!(
            catalog_file(&paths),
            PathBuf::from("d").join("services.json")
        );
        let other = paths.with_profile("local").unwrap();
        assert_eq!(
            catalog_file(&other),
            PathBuf::from("d").join("services-local.json")
        );
    }

    #[test]
    fn reads_campuses_and_centers() {
        let campuses = parse_campuses(r#"{"campus":[[1,"RE",false],[2,"ZL",true]]}"#).unwrap();
        assert_eq!(campuses.len(), 2);
        assert_eq!(campuses[1].id, "2");
        assert!(campuses[1].preferred && !campuses[0].preferred);
        let centers = parse_centers(r#"{"centros":[[5,"Central DEAD",true]]}"#).unwrap();
        assert_eq!(centers[0].name, "Central DEAD");
        assert!(parse_campuses("{").is_err());
        assert!(parse_centers("{").is_err());
    }

    async fn client_for(server: &MockServer) -> (tempfile::TempDir, SuapClient) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_dirs(dir.path().join("c"), dir.path().join("d"));
        let config = suap_core::SuapConfig {
            base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
            ..suap_core::SuapConfig::default()
        };
        (dir, SuapClient::open(&paths, &config).unwrap())
    }

    async fn page(server: &MockServer, at: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(at.to_owned()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_owned()))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn walks_the_areas_and_keeps_each_service_once() {
        let server = MockServer::start().await;
        page(&server, AREAS_PATH, AREAS).await;
        page(
            &server,
            "/centralservicos/selecionar_servico_abertura/ti/",
            AREA_TI,
        )
        .await;
        page(
            &server,
            "/centralservicos/selecionar_servico_abertura/gestao/",
            AREA_GESTAO,
        )
        .await;
        let campus = "/centralservicos/get_campus_com_centros_atendimento/400/0/";
        page(&server, campus, r#"{"campus":[[2,"ZL",true]]}"#).await;
        let centers = "/centralservicos/get_centros_atendimento_por_servico_e_campus/400/2/";
        page(&server, centers, r#"{"centros":[[5,"Central",true]]}"#).await;
        let (_dir, client) = client_for(&server).await;

        let catalog = fetch_catalog(&client, 99).await.unwrap();
        assert_eq!(catalog.fetched_at, 99);
        let ids: Vec<_> = catalog.services.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![12, 13, 400, 7]);
        assert_eq!(catalog.find(400).unwrap().area, "Tecnologia da Informação");

        assert_eq!(fetch_campuses(&client, 400).await.unwrap()[0].name, "ZL");
        assert_eq!(fetch_centers(&client, 400, "2").await.unwrap()[0].id, "5");
    }

    #[tokio::test]
    async fn fails_when_there_are_no_areas_or_a_page_is_missing() {
        let server = MockServer::start().await;
        page(&server, AREAS_PATH, "<p>vazio</p>").await;
        let (_dir, client) = client_for(&server).await;
        let message = fetch_catalog(&client, 1).await.unwrap_err().to_string();
        assert!(message.contains("no service area"));

        let broken = MockServer::start().await;
        page(&broken, AREAS_PATH, AREAS).await;
        let (_dir2, client2) = client_for(&broken).await;
        assert!(fetch_catalog(&client2, 1).await.is_err());
    }
}
