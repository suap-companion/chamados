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

/// O caminho de uma imagem do SUAP: um endereço do mesmo SUAP, relativo ou completo, e nunca de outro
/// servidor (a sessão só vai ao SUAP do perfil). A imagem padrão de "sem foto" não conta.
fn same_site_path(source: &str, base_url: &Url) -> Option<String> {
    let url = base_url.join(source).ok()?;
    let same_site = url.host_str() == base_url.host_str() && url.port() == base_url.port();
    let is_default = url.path().ends_with("/default.jpg");
    if !same_site || is_default {
        return None;
    }
    Some(match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    })
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
