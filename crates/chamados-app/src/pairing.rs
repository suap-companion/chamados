//! Pareamento de dispositivos da sincronização: o que o QR (ou o texto colado) leva de um para o outro.
//!
//! Leva a chave de criptografia e os dados **não secretos** do bucket (endpoint, bucket, região,
//! prefixo). As credenciais de acesso ao bucket nunca vão aqui: cada dispositivo as recebe à parte.
//! Como o QR contém a chave, só deve ser mostrado a quem vai lê-lo (nunca em tela compartilhada).

use std::path::Path;

use chamados_sync::{Key, SyncError, R2_BACKEND, S3_BACKEND};
use qrcode::{render::unicode::Dense1x2, QrCode};
use suap_core::SyncSettings;
use url::Url;

const SCHEME: &str = "chamados-sync";
const VERSION: &str = "1";

/// A chave e o bucket de uma sincronização, prontos para ir a outro dispositivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub key: Key,
    pub backend: String,
    pub endpoint: String,
    pub region: Option<String>,
    pub bucket: String,
    pub prefix: Option<String>,
}

fn refused(detail: &str) -> SyncError {
    SyncError::Backend(format!("pareamento: {detail}"))
}

impl Pairing {
    /// O pareamento das configurações atuais com a `key`; só um bucket (`s3`/`r2`) pode ser pareado,
    /// já que uma pasta local não existe em outro aparelho.
    pub fn from_settings(settings: &SyncSettings, key: Key) -> Result<Self, SyncError> {
        let backend = settings.backend.clone().unwrap_or_default();
        if backend != S3_BACKEND && backend != R2_BACKEND {
            return Err(refused("só um bucket s3 ou r2 pode ser pareado"));
        }
        let (Some(endpoint), Some(bucket)) = (&settings.endpoint, &settings.bucket) else {
            return Err(refused("faltam o endpoint e o bucket na configuração"));
        };
        Ok(Self {
            key,
            backend,
            endpoint: endpoint.clone(),
            region: settings.region.clone(),
            bucket: bucket.clone(),
            prefix: settings.prefix.clone(),
        })
    }

    /// O texto do pareamento (o que o QR carrega): `chamados-sync://pareamento?...`.
    pub fn to_uri(&self) -> String {
        let mut uri = Url::parse(&format!("{SCHEME}://pareamento")).expect("static URL");
        {
            let mut query = uri.query_pairs_mut();
            query.append_pair("v", VERSION);
            query.append_pair("chave", &self.key.to_hex());
            query.append_pair("backend", &self.backend);
            query.append_pair("endpoint", &self.endpoint);
            query.append_pair("bucket", &self.bucket);
            if let Some(region) = &self.region {
                query.append_pair("regiao", region);
            }
            if let Some(prefix) = &self.prefix {
                query.append_pair("prefixo", prefix);
            }
        }
        uri.into()
    }

    /// Lê o texto de um pareamento (o conteúdo de um QR, ou o que foi colado).
    pub fn parse(text: &str) -> Result<Self, SyncError> {
        let uri = Url::parse(text.trim()).map_err(|_| refused("o texto não é um pareamento"))?;
        if uri.scheme() != SCHEME || uri.host_str() != Some("pareamento") {
            return Err(refused("o texto não é um pareamento do chamados"));
        }
        let field = |name: &str| {
            uri.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .filter(|value| !value.is_empty())
        };
        if field("v").as_deref() != Some(VERSION) {
            return Err(refused("versão não suportada; atualize o aplicativo"));
        }
        let backend = field("backend").unwrap_or_default();
        if backend != S3_BACKEND && backend != R2_BACKEND {
            return Err(refused("backend desconhecido"));
        }
        let (Some(key), Some(endpoint), Some(bucket)) =
            (field("chave"), field("endpoint"), field("bucket"))
        else {
            return Err(refused("faltam a chave, o endpoint ou o bucket"));
        };
        Ok(Self {
            key: Key::from_hex(&key)?,
            backend,
            endpoint,
            region: field("regiao"),
            bucket,
            prefix: field("prefixo"),
        })
    }

    /// Põe o bucket nas configurações e manda ler a chave do arquivo `key_file`.
    pub fn apply(&self, settings: &mut SyncSettings, key_file: &Path) {
        settings.backend = Some(self.backend.clone());
        settings.endpoint = Some(self.endpoint.clone());
        settings.bucket = Some(self.bucket.clone());
        settings.region = self.region.clone();
        settings.prefix = self.prefix.clone();
        settings.key_source = Some("file".to_owned());
        settings.key_file = Some(key_file.display().to_string());
    }
}

/// O QR de `data` desenhado com caracteres de bloco, para o terminal.
pub fn qr_text(data: &str) -> Result<String, SyncError> {
    let code =
        QrCode::new(data.as_bytes()).map_err(|_| refused("texto grande demais para um QR"))?;
    Ok(code.render::<Dense1x2>().quiet_zone(true).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket() -> SyncSettings {
        SyncSettings {
            backend: Some(R2_BACKEND.to_owned()),
            endpoint: Some("https://conta.r2.cloudflarestorage.com".to_owned()),
            bucket: Some("chamados".to_owned()),
            region: Some("auto".to_owned()),
            prefix: Some("kelson/ifrn".to_owned()),
            ..SyncSettings::default()
        }
    }

    #[test]
    fn the_pairing_survives_the_trip_through_text() {
        let key = Key::generate();
        let pairing = Pairing::from_settings(&bucket(), key.clone()).unwrap();
        let uri = pairing.to_uri();
        assert!(uri.starts_with("chamados-sync://pareamento?v=1&chave="));
        assert_eq!(Pairing::parse(&format!("  {uri}\n")).unwrap(), pairing);

        // Region and prefix are optional.
        let plain = SyncSettings {
            region: None,
            prefix: None,
            ..bucket()
        };
        let plain = Pairing::from_settings(&plain, key).unwrap();
        assert!(!plain.to_uri().contains("regiao"));
        assert_eq!(Pairing::parse(&plain.to_uri()).unwrap(), plain);
    }

    #[test]
    fn only_a_complete_bucket_can_be_paired() {
        let key = Key::generate;
        let folder = SyncSettings {
            backend: Some("directory".to_owned()),
            ..bucket()
        };
        assert!(Pairing::from_settings(&folder, key()).is_err());
        assert!(Pairing::from_settings(&SyncSettings::default(), key()).is_err());
        let no_bucket = SyncSettings {
            bucket: None,
            ..bucket()
        };
        let error = Pairing::from_settings(&no_bucket, key()).unwrap_err();
        assert!(error.to_string().contains("faltam o endpoint e o bucket"));
    }

    #[test]
    fn a_text_that_is_not_a_pairing_is_refused() {
        let hex = Key::generate().to_hex();
        let base = "https://conta.r2.cloudflarestorage.com";
        let refuses = |text: &str| Pairing::parse(text).unwrap_err().to_string();
        assert!(refuses("não é uma URL").contains("não é um pareamento"));
        assert!(refuses("https://exemplo.com/?v=1").contains("do chamados"));
        assert!(refuses("chamados-sync://outro?v=1").contains("do chamados"));
        let versioned = |version: &str| {
            format!("chamados-sync://pareamento?v={version}&chave={hex}&backend=r2&endpoint={base}&bucket=b")
        };
        assert!(refuses(&versioned("2")).contains("versão não suportada"));
        let unknown = versioned("1").replace("backend=r2", "backend=ftp");
        assert!(refuses(&unknown).contains("backend desconhecido"));
        let no_key = format!("chamados-sync://pareamento?v=1&backend=r2&endpoint={base}&bucket=b");
        assert!(refuses(&no_key).contains("faltam a chave"));
        let short = versioned("1").replace(&hex, "abcd");
        assert!(refuses(&short).contains("hexadecimal"));
    }

    #[test]
    fn applying_it_points_the_settings_at_the_bucket_and_the_key_file() {
        let pairing = Pairing::from_settings(&bucket(), Key::generate()).unwrap();
        let mut settings = SyncSettings::default();
        pairing.apply(&mut settings, Path::new("/dados/sync.key"));
        assert_eq!(settings.backend.as_deref(), Some("r2"));
        assert_eq!(settings.bucket.as_deref(), Some("chamados"));
        assert_eq!(settings.prefix.as_deref(), Some("kelson/ifrn"));
        assert_eq!(settings.key_source.as_deref(), Some("file"));
        assert!(settings.key_file.as_deref().unwrap().ends_with("sync.key"));
    }

    #[test]
    fn the_qr_is_drawn_for_short_texts_only() {
        let uri = Pairing::from_settings(&bucket(), Key::generate())
            .unwrap()
            .to_uri();
        let drawing = qr_text(&uri).unwrap();
        assert!(drawing.lines().count() > 10 && drawing.contains('█'));
        let huge = "x".repeat(4000);
        assert!(qr_text(&huge).is_err());
    }
}
