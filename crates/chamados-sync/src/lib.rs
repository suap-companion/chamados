//! Encrypted synchronization of the local `chamados` data (ticket titles and profile settings).
//!
//! The generic pieces (encryption, key storage, backends, the run lock) live in `suap-sync`
//! (repository `suap-companion/suap-kit`) and are re-exported here. What stays in this crate:
//!
//! - [`document`]: the syncable document of each profile and its entry-by-entry merge;
//! - [`engine`]: one synchronization round (download, merge, upload, apply).
//!
//! Sessions, passwords, cloud credentials and the key itself are never part of the document.

pub mod document;
pub mod engine;

pub use document::{
    apply_merged, collect_local, merge, record_removal, ApplyReport, ProfileDoc, SettingsEntry,
    SyncDocument,
};
pub use engine::{sync_once, SyncOptions, SyncReport, OBJECT_NAME};
pub use suap_sync::SyncError;
pub use suap_sync::{
    access_key_env, backend, backend_from, credentials, crypto, key_env, key_exists,
    key_source_from, keys, load_key, passphrase_env, protected, s3, secret_key_env, setup, sigv4,
    store_key, validate_settings, AnyBackend, Condition, DirectoryBackend, Key, KeySource, Object,
    Passphrase, PutOutcome, S3Backend, S3Credentials, S3Settings, SyncBackend, SyncLock,
    DIRECTORY_BACKEND, R2_BACKEND, S3_BACKEND,
};

/// Brings the errors of the local stores (`TicketError`) into [`SyncError`] with `?`.
pub(crate) trait LocalResult<T> {
    fn local(self) -> Result<T, SyncError>;
}

impl<T> LocalResult<T> for Result<T, chamados_core::TicketError> {
    fn local(self) -> Result<T, SyncError> {
        self.map_err(SyncError::local)
    }
}
