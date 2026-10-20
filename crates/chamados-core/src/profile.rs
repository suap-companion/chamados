//! Quem está logado, como o SUAP mostra no canto da página: o nome usual, o vínculo (a matrícula) e a
//! foto. Tudo vem do próprio HTML (`#user-tools a.user-profile`); nada é inventado nem guardado fora
//! do aparelho.

use scraper::Html;
use url::Url;

use crate::{flat_text, selector};

/// A pessoa logada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    /// O nome usual.
    pub name: String,
    /// O vínculo (matrícula), do `title="Vínculo: ..."`.
    pub enrollment: Option<String>,
    /// O caminho da foto no SUAP (sem a foto padrão, que não é de ninguém).
    pub photo_path: Option<String>,
}

/// Se `url` é um endereço https num subdomínio do SUAP do perfil (o CDN das fotos, como
/// `cdn.suap.exemplo`): as fotos de produção são servidas de lá, com o endereço já assinado.
pub fn is_site_cdn(url: &Url, base_url: &Url) -> bool {
    let (Some(host), Some(base)) = (url.host_str(), base_url.host_str()) else {
        return false;
    };
    let subdomain = host
        .strip_suffix(base)
        .is_some_and(|rest| rest.len() > 1 && rest.ends_with('.'));
    url.scheme() == "https" && url.port().is_none() && subdomain
}

/// O endereço de uma imagem do SUAP: o caminho, se for do mesmo SUAP (relativo ou completo), ou o
/// endereço completo, se for do CDN de um subdomínio dele; nunca de outro servidor (a sessão só vai ao
/// SUAP do perfil). A imagem padrão de "sem foto" não conta.
pub(crate) fn same_site_path(source: &str, base_url: &Url) -> Option<String> {
    let url = base_url.join(source).ok()?;
    if url.path().ends_with("/default.jpg") {
        return None;
    }
    if url.host_str() == base_url.host_str() && url.port() == base_url.port() {
        return Some(match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        });
    }
    is_site_cdn(&url, base_url).then(|| url.to_string())
}

/// Lê o nome, o vínculo e a foto do canto da página do SUAP; `None` se a página não os tem (por
/// exemplo, a de login).
pub fn parse_user_profile(html: &str, base_url: &Url) -> Option<UserProfile> {
    let document = Html::parse_document(html);
    let link = document
        .select(&selector("#user-tools a.user-profile"))
        .next()?;
    let label = link.select(&selector("span[title]")).next()?;
    let name = flat_text(label);
    if name.is_empty() {
        return None;
    }
    let enrollment = label
        .value()
        .attr("title")
        .and_then(|title| title.split_once(':'))
        .map(|(_, enrollment)| enrollment.trim().to_owned())
        .filter(|enrollment| !enrollment.is_empty());
    let photo_path = link
        .select(&selector("img"))
        .next()
        .and_then(|image| image.value().attr("src"))
        .and_then(|source| same_site_path(source, base_url));
    Some(UserProfile {
        name,
        enrollment,
        photo_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(photo: &str, label: &str) -> String {
        format!(
            r#"<header><div id="user-tools">
              <a href="/rh/servidor/2080882/" class="user-profile">
                <div class="photo-circle sm"><img src="{photo}" alt="Foto"></div>
                {label}
              </a></div></header>"#
        )
    }

    fn base() -> Url {
        Url::parse("https://suap.example/").unwrap()
    }

    #[test]
    fn reads_the_name_the_enrollment_and_the_photo() {
        let html = page(
            "/media/fotos/2080882.jpg",
            r#"<span title="Vínculo: 2080882">Pessoa  Zero</span>"#,
        );
        let profile = parse_user_profile(&html, &base()).unwrap();
        assert_eq!(profile.name, "Pessoa Zero");
        assert_eq!(profile.enrollment.as_deref(), Some("2080882"));
        assert_eq!(
            profile.photo_path.as_deref(),
            Some("/media/fotos/2080882.jpg")
        );
    }

    #[test]
    fn only_photos_from_the_same_site_count() {
        let label = r#"<span title="Vínculo: 7">Ana</span>"#;
        let photo = |source: &str| {
            parse_user_profile(&page(source, label), &base())
                .unwrap()
                .photo_path
        };
        assert_eq!(
            photo("https://suap.example/media/f.jpg?v=3").as_deref(),
            Some("/media/f.jpg?v=3")
        );
        assert_eq!(photo("fotos/f.jpg").as_deref(), Some("/fotos/f.jpg"));
        // The default picture belongs to nobody; another server never gets the session.
        assert_eq!(photo("/static/comum/img/default.jpg"), None);
        assert_eq!(photo("https://outro.example/f.jpg"), None);
        assert_eq!(photo("https://suap.example:8443/f.jpg"), None);
        assert_eq!(photo("http://[::1"), None);
    }

    #[test]
    fn the_photo_cdn_of_the_site_counts_but_nothing_that_only_looks_like_it() {
        let label = r#"<span title="Vínculo: 7">Ana</span>"#;
        let photo = |source: &str| {
            parse_user_profile(&page(source, label), &base())
                .unwrap()
                .photo_path
        };
        // The CDN is a subdomain of the site, over https, with its signed address kept whole.
        let signed = "https://cdn.suap.example/media/fotos/75x100/1.jpg?X-Amz-Signature=abc&X-Amz-Expires=86400";
        assert_eq!(photo(signed).as_deref(), Some(signed));
        let deep = "https://a.b.suap.example/f.jpg";
        assert_eq!(photo(deep).as_deref(), Some(deep));
        // Not the CDN: plain http, another port, a host that only ends with the site's name, or a
        // site that merely contains it.
        assert_eq!(photo("http://cdn.suap.example/f.jpg"), None);
        assert_eq!(photo("https://cdn.suap.example:8443/f.jpg"), None);
        assert_eq!(photo("https://evilsuap.example/f.jpg"), None);
        assert_eq!(photo("https://suap.example.evil.example/f.jpg"), None);
        assert_eq!(
            photo("https://cdn.suap.example/static/comum/img/default.jpg"),
            None
        );
        // An address without a host (a data URL) is not the CDN of anything.
        let no_host = Url::parse("data:text/plain,x").unwrap();
        assert!(!is_site_cdn(&no_host, &base()));
        assert!(!is_site_cdn(&base(), &no_host));
    }

    #[test]
    fn without_the_user_corner_there_is_no_profile() {
        assert!(parse_user_profile("<html><body>login</body></html>", &base()).is_none());
        let no_label = page("/x.jpg", "");
        assert!(parse_user_profile(&no_label, &base()).is_none());
        let empty_name = page("/x.jpg", r#"<span title="Vínculo: 1">  </span>"#);
        assert!(parse_user_profile(&empty_name, &base()).is_none());
        // A label without a usable enrollment still names the person.
        let blank = page("/x.jpg", r#"<span title="Vínculo:  ">Bia</span>"#);
        assert_eq!(
            parse_user_profile(&blank, &base()).unwrap().enrollment,
            None
        );
        let no_colon = page("/x.jpg", r#"<span title="vínculo 1">Bia</span>"#);
        assert_eq!(
            parse_user_profile(&no_colon, &base()).unwrap().enrollment,
            None
        );
        let no_image = r#"<div id="user-tools"><a class="user-profile"><span title="Vínculo: 2">Cris</span></a></div>"#;
        assert_eq!(
            parse_user_profile(no_image, &base()).unwrap().photo_path,
            None
        );
    }
}
