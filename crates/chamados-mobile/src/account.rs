//! A pessoa logada (nome usual, matrícula e foto, como o SUAP mostra no canto da página), guardada no
//! aparelho para o cabeçalho do app funcionar também sem rede.

use std::{fs, io::ErrorKind, path::PathBuf, time::Duration};

use chamados_core::{is_site_cdn, parse_user_profile};
use url::Url;

use crate::{failed, Chamados, ChamadosError};

const INFO_FILE: &str = "usuario.txt";
const PHOTO_FILE: &str = "foto.img";
/// Uma foto maior que isto não é foto de perfil: não é guardada.
const MAX_PHOTO_BYTES: usize = 2_000_000;
const PHOTO_TIMEOUT: Duration = Duration::from_secs(20);

/// Quem está logado: o nome usual, o vínculo (matrícula) e o arquivo da foto, se o SUAP tem uma.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UserInfo {
    pub name: String,
    pub enrollment: Option<String>,
    /// O caminho do arquivo da foto no aparelho.
    pub photo: Option<String>,
}

/// O nome do arquivo de uma foto. A de um endereço completo (o CDN) tem a assinatura na consulta, que
/// muda a cada listagem: o nome usa só o servidor e o caminho, para a mesma foto não ser baixada de novo.
fn photo_file_name(path: &str) -> String {
    let stable = match Url::parse(path) {
        Ok(url) => format!("{}{}", url.host_str().unwrap_or_default(), url.path()),
        Err(_) => path.to_owned(),
    };
    stable
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Baixa uma imagem de um endereço completo, sem a sessão do SUAP (o endereço do CDN já vem assinado).
/// `None` se não for uma imagem, se o servidor recusar ou se for grande demais.
async fn fetch_image(url: &Url) -> Option<Vec<u8>> {
    let builder = reqwest::Client::builder().timeout(PHOTO_TIMEOUT);
    let response = builder.build().ok()?.get(url.clone()).send().await.ok()?;
    let kind = response.headers().get(reqwest::header::CONTENT_TYPE);
    let is_image = kind
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("image/"));
    if !response.status().is_success() || !is_image {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    (bytes.len() <= MAX_PHOTO_BYTES).then(|| bytes.to_vec())
}

fn remove(path: &PathBuf) -> Result<(), ChamadosError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}

impl Chamados {
    /// Os bytes de uma foto do SUAP: um caminho do mesmo SUAP vai com a sessão; um endereço completo só
    /// vale se for do CDN de um subdomínio dele (https) e vai sem a sessão.
    async fn photo_bytes(&self, path: &str) -> Option<Vec<u8>> {
        let Ok(url) = Url::parse(path) else {
            return self
                .client
                .fetch_file(path)
                .await
                .ok()
                .map(|(bytes, _)| bytes);
        };
        if is_site_cdn(&url, &self.base_url) {
            fetch_image(&url).await
        } else {
            None
        }
    }

    fn info_file(&self) -> PathBuf {
        self.paths.data_dir().join(INFO_FILE)
    }

    fn photo_file(&self) -> PathBuf {
        self.paths.data_dir().join(PHOTO_FILE)
    }

    /// Esquece a pessoa guardada (ao sair da conta).
    pub(crate) fn forget_user(&self) -> Result<(), ChamadosError> {
        remove(&self.info_file())?;
        remove(&self.photo_file())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// A pessoa guardada na última vez que o SUAP foi consultado (sem rede), se houver.
    pub fn cached_user(&self) -> Result<Option<UserInfo>, ChamadosError> {
        let text = match fs::read_to_string(self.info_file()) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            read => read.map_err(failed)?,
        };
        let mut lines = text.lines();
        let name = lines.next().unwrap_or_default().to_owned();
        let enrollment = lines
            .next()
            .filter(|line| !line.is_empty())
            .map(str::to_owned);
        let photo = self.photo_file();
        Ok(Some(UserInfo {
            name,
            enrollment,
            photo: photo.exists().then(|| photo.display().to_string()),
        }))
    }

    /// A foto de quem pediu um chamado (o caminho vem de `TicketSummary.photo`): baixa no SUAP uma vez e
    /// guarda no aparelho; devolve o arquivo, ou `None` se não deu para baixar.
    pub async fn ticket_photo(&self, path: String) -> Result<Option<String>, ChamadosError> {
        let folder = self.paths.data_dir().join("fotos");
        let file = folder.join(photo_file_name(&path));
        if !file.exists() {
            let Some(bytes) = self.photo_bytes(&path).await else {
                return Ok(None);
            };
            fs::create_dir_all(&folder).map_err(failed)?;
            fs::write(&file, bytes).map_err(failed)?;
        }
        Ok(Some(file.display().to_string()))
    }

    /// Lê no SUAP o nome, o vínculo e a foto da pessoa logada e guarda no aparelho. `None` se a página
    /// não mostra a pessoa; falha se a sessão acabou ou o SUAP não responde (o que estava guardado fica).
    pub async fn refresh_user(&self) -> Result<Option<UserInfo>, ChamadosError> {
        let html = self.client.fetch_page("/").await.map_err(failed)?;
        let Some(profile) = parse_user_profile(&html, &self.base_url) else {
            return Ok(None);
        };
        // Sem foto (só a padrão do SUAP), ou se ela não baixar: o app mostra o ícone de pessoa.
        let photo = match &profile.photo_path {
            Some(path) => self.photo_bytes(path).await,
            None => None,
        };
        fs::create_dir_all(self.paths.data_dir()).map_err(failed)?;
        match photo {
            Some(bytes) => fs::write(self.photo_file(), bytes).map_err(failed)?,
            None => remove(&self.photo_file())?,
        }
        let enrollment = profile.enrollment.unwrap_or_default();
        let text = format!("{}\n{enrollment}\n", profile.name);
        fs::write(self.info_file(), text).map_err(failed)?;
        self.cached_user()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn app(server: &MockServer) -> (tempfile::TempDir, Arc<Chamados>) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_str().unwrap().to_owned();
        let chamados = Chamados::new(data, format!("{}/", server.uri())).unwrap();
        (dir, chamados)
    }

    async fn serve(server: &MockServer, at: &str, body: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(at.to_owned()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(server)
            .await;
    }

    fn home(photo: &str) -> Vec<u8> {
        format!(
            r#"<div id="user-tools"><a class="user-profile"><div class="photo-circle sm"><img src="{photo}"></div>
            <span title="Vínculo: 2080882">Pessoa Zero</span></a></div>"#
        )
        .into_bytes()
    }

    #[tokio::test]
    async fn the_person_and_the_photo_are_read_from_suap_and_kept() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        assert_eq!(chamados.cached_user().unwrap(), None);

        serve(&server, "/", home("/media/foto.jpg")).await;
        serve(&server, "/media/foto.jpg", vec![0xFF, 0xD8, 0xFF]).await;
        let user = chamados.refresh_user().await.unwrap().unwrap();
        assert_eq!(user.name, "Pessoa Zero");
        assert_eq!(user.enrollment.as_deref(), Some("2080882"));
        let photo = user.photo.clone().unwrap();
        assert_eq!(std::fs::read(&photo).unwrap(), [0xFF, 0xD8, 0xFF]);
        // Without the network the same person is still there.
        assert_eq!(chamados.cached_user().unwrap(), Some(user));

        // A photo that does not download leaves the person without one; the default picture too.
        server.reset().await;
        serve(&server, "/", home("/media/some.jpg")).await;
        let user = chamados.refresh_user().await.unwrap().unwrap();
        assert_eq!(user.photo, None);
        server.reset().await;
        serve(&server, "/", home("/static/comum/img/default.jpg")).await;
        assert_eq!(chamados.refresh_user().await.unwrap().unwrap().photo, None);

        // A page without the user's corner says nothing and keeps what was kept.
        server.reset().await;
        serve(&server, "/", b"<html>nada</html>".to_vec()).await;
        assert_eq!(chamados.refresh_user().await.unwrap(), None);
        assert_eq!(chamados.cached_user().unwrap().unwrap().name, "Pessoa Zero");

        // Signing out forgets the person (and signing out twice is fine).
        chamados.logout().await.unwrap();
        assert_eq!(chamados.cached_user().unwrap(), None);
        chamados.logout().await.unwrap();
    }

    #[tokio::test]
    async fn the_photo_of_a_requester_is_downloaded_once() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        // Not served yet: no photo, and nothing is kept.
        assert_eq!(
            chamados.ticket_photo("/m/x.jpg".to_owned()).await.unwrap(),
            None
        );
        serve(&server, "/m/x.jpg", vec![1, 2, 3]).await;
        let file = chamados
            .ticket_photo("/m/x.jpg".to_owned())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), [1, 2, 3]);
        // Kept: the second time does not need the SUAP.
        server.reset().await;
        let again = chamados.ticket_photo("/m/x.jpg".to_owned()).await.unwrap();
        assert_eq!(again, Some(file));
        // The folder cannot be made: an error.
        let blocked = chamados.paths.data_dir().join("fotos");
        std::fs::remove_dir_all(&blocked).unwrap();
        std::fs::write(&blocked, "x").unwrap();
        serve(&server, "/m/y.jpg", vec![4]).await;
        assert!(chamados.ticket_photo("/m/y.jpg".to_owned()).await.is_err());
    }

    #[test]
    fn the_file_of_a_photo_is_named_by_its_address_without_the_signature() {
        // A plain path keeps naming the file as it always did.
        assert_eq!(photo_file_name("/m/x.jpg"), "_m_x_jpg");
        // The CDN address changes its signature at every listing: the same photo, the same file.
        let first = "https://cdn.suap.example/media/fotos/1.jpg?X-Amz-Signature=aaa&X-Amz-Date=1";
        let second = "https://cdn.suap.example/media/fotos/1.jpg?X-Amz-Signature=bbb&X-Amz-Date=2";
        assert_eq!(photo_file_name(first), photo_file_name(second));
        assert_eq!(photo_file_name(first), "cdn_suap_example_media_fotos_1_jpg");
        assert_ne!(
            photo_file_name(first),
            photo_file_name("https://cdn.suap.example/media/fotos/2.jpg")
        );
    }

    #[tokio::test]
    async fn an_image_is_fetched_from_a_full_address_only_when_it_is_one() {
        let server = MockServer::start().await;
        let serve_as = |at: &'static str, kind: &'static str, body: Vec<u8>| {
            Mock::given(method("GET"))
                .and(path(at))
                .respond_with(ResponseTemplate::new(200).set_body_raw(body, kind))
        };
        serve_as("/ok.jpg", "image/jpeg", vec![1, 2, 3])
            .mount(&server)
            .await;
        serve_as("/page.html", "text/html", b"<html>login</html>".to_vec())
            .mount(&server)
            .await;
        serve_as("/big.jpg", "image/jpeg", vec![0; MAX_PHOTO_BYTES + 1])
            .mount(&server)
            .await;
        let at = |name: &str| Url::parse(&format!("{}{name}", server.uri())).unwrap();

        assert_eq!(fetch_image(&at("/ok.jpg")).await, Some(vec![1, 2, 3]));
        // Not an image (a login page, for example), too big, refused, or nobody there: no photo.
        assert_eq!(fetch_image(&at("/page.html")).await, None);
        assert_eq!(fetch_image(&at("/big.jpg")).await, None);
        assert_eq!(fetch_image(&at("/missing.jpg")).await, None);
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            Url::parse(&format!("http://{}/x.jpg", listener.local_addr().unwrap())).unwrap()
        };
        assert_eq!(fetch_image(&closed).await, None);
    }

    #[tokio::test]
    async fn only_the_photo_cdn_of_the_site_is_ever_asked_for_a_full_address() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_str().unwrap().to_owned();
        let chamados = Chamados::new(data, "https://suap.invalid/".to_owned()).unwrap();
        // Another server is refused without any request; the site's own CDN is tried (and, with
        // nobody there, there is no photo), never with the site's session.
        for address in [
            "https://outro.example/media/f.jpg",
            "http://cdn.suap.invalid/media/f.jpg",
            "https://cdn.suap.invalid/media/f.jpg?X-Amz-Signature=abc",
        ] {
            assert_eq!(
                chamados.ticket_photo(address.to_owned()).await.unwrap(),
                None
            );
        }
        assert!(!chamados.paths.data_dir().join("fotos").exists());
    }

    #[tokio::test]
    async fn failures_are_errors_the_app_can_show() {
        let server = MockServer::start().await;
        let (_dir, chamados) = app(&server);
        // SUAP out (no route): an error, nothing kept.
        assert!(chamados.refresh_user().await.is_err());
        assert_eq!(chamados.cached_user().unwrap(), None);

        serve(&server, "/", home("/x.jpg")).await;
        // The data folder is a file: the person cannot be kept.
        let data = chamados.paths.data_dir().to_path_buf();
        std::fs::remove_dir_all(&data).unwrap();
        std::fs::write(&data, "x").unwrap();
        assert!(chamados.refresh_user().await.is_err());
        assert!(chamados.cached_user().is_err());
        assert!(chamados.forget_user().is_err());
        std::fs::remove_file(&data).unwrap();
    }
}
