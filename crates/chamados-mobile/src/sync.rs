//! Títulos locais e sincronização cifrada, como o aplicativo os usa.
//!
//! No aparelho não há chaveiro do sistema: a chave e as credenciais do bucket ficam em arquivos só
//! legíveis pelo aplicativo (`config/sync.key` e `config/s3-credentials`, `0600`), dentro do
//! armazenamento privado dele. A chave e as credenciais nunca vão para o documento sincronizado.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chamados_app::{pairing::Pairing, syncing::sync_round};
use chamados_core::{ArchiveStore, TitleStore};
use chamados_sync::{
    backend_from, key_exists, load_key, store_key, validate_settings, Key, KeySource,
    S3Credentials, SyncOptions, R2_BACKEND, S3_BACKEND,
};
use suap_core::{
    load_config, load_sync_settings, save_config, save_sync_settings, SuapConfig, SyncSettings,
};

use crate::{failed, Chamados, ChamadosError};

/// O título local de um chamado.
#[derive(Debug, uniffi::Record)]
pub struct TitleItem {
    pub id: String,
    pub title: String,
}

/// Como está a sincronização neste aparelho.
#[derive(Debug, uniffi::Record)]
pub struct SyncState {
    pub configured: bool,
    pub backend: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub prefix: Option<String>,
    pub has_key: bool,
    pub has_credentials: bool,
}

/// O bucket (`r2` ou `s3`) onde a nuvem cifrada fica.
#[derive(Debug, uniffi::Record)]
pub struct SyncSetup {
    pub backend: String,
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub prefix: Option<String>,
}

/// O resultado de uma rodada de sincronização.
#[derive(Debug, uniffi::Record)]
pub struct SyncSummary {
    pub profiles: u32,
    pub local_changes: u32,
    pub uploaded: bool,
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn forget_file(path: &Path) -> Result<(), ChamadosError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}

impl Chamados {
    fn key_file(&self) -> PathBuf {
        self.paths.default_key_file()
    }

    fn credentials_file(&self) -> PathBuf {
        self.paths.config_dir().join("s3-credentials")
    }

    fn key_source(&self) -> KeySource {
        KeySource::File(self.key_file())
    }

    /// Põe este perfil na sincronização. As configurações dele nunca valem mais que as de outro
    /// aparelho (`updated_at` zero): o app não muda o endereço do SUAP de quem já sincroniza.
    fn join_sync(&self) -> Result<(), ChamadosError> {
        let config = match load_config(&self.paths).map_err(failed)? {
            Some(config) => SuapConfig {
                sync: true,
                ..config
            },
            None => SuapConfig {
                base_url: self.base_url.clone(),
                sync: true,
                ..SuapConfig::default()
            },
        };
        save_config(&self.paths, &config).map_err(failed)
    }

    /// Guarda as configurações do bucket (só `s3`/`r2`), com a chave em arquivo.
    fn save_bucket(&self, settings: &mut SyncSettings) -> Result<(), ChamadosError> {
        if !matches!(settings.backend.as_deref(), Some(S3_BACKEND | R2_BACKEND)) {
            return Err(failed("o app sincroniza com um bucket s3 ou r2"));
        }
        settings.key_source = Some("file".to_owned());
        settings.key_file = Some(self.key_file().display().to_string());
        validate_settings(settings).map_err(failed)?;
        save_sync_settings(&self.paths, settings).map_err(failed)?;
        self.join_sync()
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Chamados {
    /// Os títulos locais dos chamados (só os que têm título).
    pub fn titles(&self) -> Result<Vec<TitleItem>, ChamadosError> {
        let store = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let entries = store.entries().iter();
        Ok(entries
            .filter_map(|(id, entry)| {
                let title = entry.title.clone()?;
                Some(TitleItem {
                    id: id.clone(),
                    title,
                })
            })
            .collect())
    }

    /// Dá um título local (uma linha, até 120 caracteres) ao chamado.
    pub fn set_title(&self, id: String, title: String) -> Result<(), ChamadosError> {
        let mut store = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        store.set(&id, &title, unix_now()).map_err(failed)?;
        store.save().map_err(failed)
    }

    /// Tira o título local do chamado; diz se ele tinha um.
    pub fn remove_title(&self, id: String) -> Result<bool, ChamadosError> {
        let mut store = TitleStore::open(self.paths.titles_file()).map_err(failed)?;
        let had_title = store.remove(&id, unix_now()).map_err(failed)?;
        store.save().map_err(failed)?;
        Ok(had_title)
    }

    /// Os números dos chamados arquivados neste aparelho (e sincronizados com os outros).
    pub fn archived_ids(&self) -> Result<Vec<String>, ChamadosError> {
        let store = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        Ok(store.archived_ids())
    }

    /// Arquiva (ou desarquiva) o chamado; diz se mudou algo. Nada é enviado ao SUAP.
    pub fn set_archived(&self, id: String, archived: bool) -> Result<bool, ChamadosError> {
        let mut store = ArchiveStore::open(self.paths.archive_file()).map_err(failed)?;
        let changed = store.set(&id, archived, unix_now()).map_err(failed)?;
        store.save().map_err(failed)?;
        Ok(changed)
    }

    /// Como está a sincronização: o bucket, e se já há chave e credenciais.
    pub fn sync_state(&self) -> Result<SyncState, ChamadosError> {
        let settings = load_sync_settings(&self.paths).map_err(failed)?;
        let credentials = S3Credentials::load_file(&self.credentials_file()).map_err(failed)?;
        Ok(SyncState {
            configured: settings.backend.is_some(),
            backend: settings.backend,
            endpoint: settings.endpoint,
            bucket: settings.bucket,
            region: settings.region,
            prefix: settings.prefix,
            has_key: key_exists(&self.key_source()).map_err(failed)?,
            has_credentials: credentials.is_some(),
        })
    }

    /// Configura o bucket (as credenciais e a chave são dadas à parte).
    pub fn sync_setup(&self, setup: SyncSetup) -> Result<SyncState, ChamadosError> {
        let mut settings = load_sync_settings(&self.paths).map_err(failed)?;
        settings.backend = Some(setup.backend);
        settings.endpoint = Some(setup.endpoint);
        settings.bucket = Some(setup.bucket);
        settings.region = setup.region.filter(|region| !region.trim().is_empty());
        settings.prefix = setup.prefix.filter(|prefix| !prefix.trim().is_empty());
        self.save_bucket(&mut settings)?;
        self.sync_state()
    }

    /// Guarda a chave de acesso ao bucket (nunca é mostrada de volta).
    pub fn sync_set_credentials(
        &self,
        access_key_id: String,
        secret_access_key: String,
    ) -> Result<(), ChamadosError> {
        let text = format!("{}\n{}", access_key_id.trim(), secret_access_key.trim());
        let credentials = S3Credentials::parse(&text).map_err(failed)?;
        credentials
            .store_file(&self.credentials_file())
            .map_err(failed)
    }

    /// Aplica um pareamento (o texto do QR do `chamados sync key pair`): o bucket e a chave.
    /// Uma chave que já existia é substituída.
    pub fn sync_apply_pairing(&self, text: String) -> Result<SyncState, ChamadosError> {
        let pairing = Pairing::parse(&text).map_err(failed)?;
        let mut settings = load_sync_settings(&self.paths).map_err(failed)?;
        pairing.apply(&mut settings, &self.key_file());
        self.save_bucket(&mut settings)?;
        store_key(&self.key_source(), &pairing.key, true).map_err(failed)?;
        self.sync_state()
    }

    /// Cria uma chave nova (a primeira do usuário). Com `replace`, troca a que existe: o que foi
    /// cifrado com a antiga fica ilegível.
    pub fn sync_generate_key(&self, replace: bool) -> Result<(), ChamadosError> {
        store_key(&self.key_source(), &Key::generate(), replace).map_err(failed)
    }

    /// Guarda a chave vinda de outro dispositivo (64 caracteres hexadecimais).
    pub fn sync_import_key(&self, hex: String, replace: bool) -> Result<(), ChamadosError> {
        let key = Key::from_hex(&hex).map_err(failed)?;
        store_key(&self.key_source(), &key, replace).map_err(failed)
    }

    /// A chave em hexadecimal, para levá-la a outro dispositivo (`chamados sync key import`).
    pub fn sync_key_hex(&self) -> Result<Option<String>, ChamadosError> {
        let key = load_key(&self.key_source()).map_err(failed)?;
        Ok(key.map(|key| key.to_hex()))
    }

    /// Para de sincronizar: esquece o bucket, as credenciais e a chave deste aparelho. Os títulos
    /// continuam aqui.
    pub fn sync_forget(&self) -> Result<(), ChamadosError> {
        save_sync_settings(&self.paths, &SyncSettings::default()).map_err(failed)?;
        forget_file(&self.key_file())?;
        forget_file(&self.credentials_file())
    }

    /// Uma rodada de sincronização: baixa, mescla, envia e aplica (perfis e títulos).
    pub async fn sync_now(&self) -> Result<SyncSummary, ChamadosError> {
        let settings = load_sync_settings(&self.paths).map_err(failed)?;
        if settings.backend.is_none() {
            return Err(failed("a sincronização não está configurada"));
        }
        let credentials = S3Credentials::load_file(&self.credentials_file()).map_err(failed)?;
        let needs_credentials =
            matches!(settings.backend.as_deref(), Some(S3_BACKEND | R2_BACKEND));
        if needs_credentials && credentials.is_none() {
            return Err(failed("faltam as credenciais do bucket"));
        }
        let backend = backend_from(&settings, credentials).map_err(failed)?;
        let key = load_key(&self.key_source()).map_err(failed)?;
        let key = key.ok_or_else(|| failed("falta a chave de criptografia"))?;
        let round = sync_round(&self.paths, &backend, &key, &SyncOptions::default()).await;
        let report = round
            .map_err(failed)?
            .ok_or_else(|| failed("outra sincronização já está em andamento"))?;
        Ok(SyncSummary {
            profiles: report.profiles as u32,
            local_changes: report.local_changes as u32,
            uploaded: report.uploaded,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chamados_sync::DIRECTORY_BACKEND;

    use super::*;

    fn device(root: &Path, name: &str) -> Arc<Chamados> {
        let data = root.join(name).to_str().unwrap().to_owned();
        Chamados::new(data, "http://10.0.2.2:8000/".to_owned()).unwrap()
    }

    fn bucket() -> SyncSetup {
        SyncSetup {
            backend: R2_BACKEND.to_owned(),
            endpoint: "https://conta.r2.cloudflarestorage.com".to_owned(),
            bucket: "chamados".to_owned(),
            region: Some("  ".to_owned()),
            prefix: Some("kelson".to_owned()),
        }
    }

    /// Points a device at a folder instead of a bucket (what the tests can reach).
    fn use_folder(chamados: &Chamados, folder: &Path) {
        let settings = SyncSettings {
            backend: Some(DIRECTORY_BACKEND.to_owned()),
            path: Some(folder.display().to_string()),
            ..SyncSettings::default()
        };
        save_sync_settings(&chamados.paths, &settings).unwrap();
        chamados.join_sync().unwrap();
    }

    #[test]
    fn titles_are_kept_validated_and_removed() {
        let root = tempfile::tempdir().unwrap();
        let chamados = device(root.path(), "a");
        assert!(chamados.titles().unwrap().is_empty());
        chamados
            .set_title("5".to_owned(), "  Moodle 5.3 ".to_owned())
            .unwrap();
        let titles = chamados.titles().unwrap();
        assert_eq!(
            (titles[0].id.as_str(), titles[0].title.as_str()),
            ("5", "Moodle 5.3")
        );
        assert!(chamados.set_title("5".to_owned(), "\n".to_owned()).is_err());
        assert!(chamados.set_title("x".to_owned(), "t".to_owned()).is_err());
        assert!(chamados.remove_title("5".to_owned()).unwrap());
        assert!(!chamados.remove_title("5".to_owned()).unwrap());
        assert!(chamados.remove_title("x".to_owned()).is_err());
        assert!(chamados.titles().unwrap().is_empty());

        // Archiving is a private mark: it is kept, listed and undone without touching SUAP.
        assert!(chamados.archived_ids().unwrap().is_empty());
        assert!(chamados.set_archived("5".to_owned(), true).unwrap());
        assert!(!chamados.set_archived("5".to_owned(), true).unwrap());
        assert_eq!(chamados.archived_ids().unwrap(), ["5"]);
        assert!(chamados.set_archived("x".to_owned(), true).is_err());
        assert!(chamados.set_archived("5".to_owned(), false).unwrap());
        assert!(chamados.archived_ids().unwrap().is_empty());
        std::fs::write(chamados.paths.archive_file(), "{").unwrap();
        assert!(chamados.archived_ids().is_err());
        assert!(chamados.set_archived("5".to_owned(), true).is_err());
        std::fs::remove_file(chamados.paths.archive_file()).unwrap();

        // A damaged file is an error, never an empty list that would hide the user's titles.
        std::fs::write(chamados.paths.titles_file(), "{").unwrap();
        assert!(chamados.titles().is_err());
        assert!(chamados.set_title("5".to_owned(), "t".to_owned()).is_err());
        assert!(chamados.remove_title("5".to_owned()).is_err());
    }

    #[test]
    fn the_bucket_the_credentials_and_the_key_are_set_up_apart() {
        let root = tempfile::tempdir().unwrap();
        let chamados = device(root.path(), "a");
        let state = chamados.sync_state().unwrap();
        assert!(!state.configured && !state.has_key && !state.has_credentials);

        let state = chamados.sync_setup(bucket()).unwrap();
        assert_eq!(state.backend.as_deref(), Some("r2"));
        assert_eq!(state.bucket.as_deref(), Some("chamados"));
        assert_eq!(state.prefix.as_deref(), Some("kelson"));
        assert!(state.region.is_none() && state.endpoint.is_some());
        assert!(state.configured && !state.has_key && !state.has_credentials);
        // The profile now takes part in the sync, without claiming to be newer than anyone.
        let profile = load_config(&chamados.paths).unwrap().unwrap();
        assert!(profile.sync && profile.updated_at == 0);
        // Setting up again keeps the profile that is already there.
        chamados.sync_setup(bucket()).unwrap();

        let wrong = SyncSetup {
            backend: "directory".to_owned(),
            ..bucket()
        };
        assert!(chamados.sync_setup(wrong).is_err());
        let insecure = SyncSetup {
            endpoint: "http://conta.example.com".to_owned(),
            ..bucket()
        };
        assert!(chamados.sync_setup(insecure).is_err());

        chamados
            .sync_set_credentials(" AKIA123 ".to_owned(), "segredo/valor\n".to_owned())
            .unwrap();
        assert!(chamados
            .sync_set_credentials(String::new(), "x".to_owned())
            .is_err());
        assert!(chamados.sync_state().unwrap().has_credentials);

        assert_eq!(chamados.sync_key_hex().unwrap(), None);
        chamados.sync_generate_key(false).unwrap();
        assert!(chamados.sync_generate_key(false).is_err());
        let hex = chamados.sync_key_hex().unwrap().unwrap();
        chamados.sync_generate_key(true).unwrap();
        assert_ne!(chamados.sync_key_hex().unwrap().unwrap(), hex);
        chamados.sync_import_key(hex.clone(), false).unwrap_err();
        chamados.sync_import_key(hex.clone(), true).unwrap();
        assert_eq!(chamados.sync_key_hex().unwrap().unwrap(), hex);
        assert!(chamados.sync_import_key("curta".to_owned(), true).is_err());
        assert!(chamados.sync_state().unwrap().has_key);

        chamados.sync_forget().unwrap();
        let state = chamados.sync_state().unwrap();
        assert!(!state.configured && !state.has_key && !state.has_credentials);
        chamados.sync_forget().unwrap();
        // Something that is not a file where the key should be cannot be forgotten.
        std::fs::create_dir_all(chamados.key_file()).unwrap();
        assert!(chamados.sync_forget().is_err());
    }

    #[test]
    fn a_pairing_brings_the_bucket_and_the_key_but_not_the_credentials() {
        let root = tempfile::tempdir().unwrap();
        let (cli, phone) = (device(root.path(), "cli"), device(root.path(), "phone"));
        cli.sync_setup(bucket()).unwrap();
        cli.sync_generate_key(false).unwrap();
        let settings = load_sync_settings(&cli.paths).unwrap();
        let key = load_key(&cli.key_source()).unwrap().unwrap();
        let text = Pairing::from_settings(&settings, key).unwrap().to_uri();

        let state = phone.sync_apply_pairing(format!(" {text}\n")).unwrap();
        assert!(state.configured && state.has_key && !state.has_credentials);
        assert_eq!(state.bucket.as_deref(), Some("chamados"));
        assert_eq!(phone.sync_key_hex().unwrap(), cli.sync_key_hex().unwrap());

        assert!(phone
            .sync_apply_pairing("qualquer coisa".to_owned())
            .is_err());
        let to_folder = text.replace("backend=r2", "backend=ftp");
        assert!(phone.sync_apply_pairing(to_folder).is_err());
    }

    #[tokio::test]
    async fn two_devices_converge_their_titles_through_the_encrypted_folder() {
        let root = tempfile::tempdir().unwrap();
        let cloud = root.path().join("nuvem");
        let (a, b) = (device(root.path(), "a"), device(root.path(), "b"));
        for device in [&a, &b] {
            use_folder(device, &cloud);
        }
        // Nothing works before there is a key; a bucket also needs its credentials.
        let error = a.sync_now().await.unwrap_err().to_string();
        assert!(error.contains("falta a chave"), "{error}");
        a.sync_generate_key(false).unwrap();
        a.set_title("5".to_owned(), "Moodle 5.3".to_owned())
            .unwrap();
        let report = a.sync_now().await.unwrap();
        assert!(report.uploaded && report.profiles == 1);
        let stored = std::fs::read(cloud.join("chamados-sync-v1.bin")).unwrap();
        assert!(!String::from_utf8_lossy(&stored).contains("Moodle"));

        b.sync_import_key(a.sync_key_hex().unwrap().unwrap(), false)
            .unwrap();
        let report = b.sync_now().await.unwrap();
        assert!(report.local_changes >= 1 && !report.uploaded);
        assert_eq!(b.titles().unwrap()[0].title, "Moodle 5.3");

        // A holds the lock: the round is not run twice at the same time.
        let lock = chamados_sync::SyncLock::acquire(&a.paths.sync_lock_file()).unwrap();
        let busy = a.sync_now().await.unwrap_err().to_string();
        assert!(busy.contains("já está em andamento"), "{busy}");
        drop(lock);

        // The key of somebody else cannot read the cloud.
        b.sync_generate_key(true).unwrap();
        assert!(b.sync_now().await.is_err());
    }

    #[tokio::test]
    async fn syncing_needs_a_configuration_and_the_bucket_credentials() {
        let root = tempfile::tempdir().unwrap();
        let chamados = device(root.path(), "a");
        let error = chamados.sync_now().await.unwrap_err().to_string();
        assert!(error.contains("não está configurada"), "{error}");
        chamados.sync_setup(bucket()).unwrap();
        let error = chamados.sync_now().await.unwrap_err().to_string();
        assert!(error.contains("faltam as credenciais"), "{error}");
        std::fs::write(chamados.credentials_file(), "uma linha só").unwrap();
        assert!(chamados.sync_state().is_err());
        assert!(chamados.sync_now().await.is_err());
    }
}
