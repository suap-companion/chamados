//! Uma rodada da sincronização cifrada, com o bloqueio que impede duas ao mesmo tempo.

use chamados_sync::{sync_once, AnyBackend, Key, SyncError, SyncLock, SyncOptions, SyncReport};
use suap_core::AppPaths;

/// Runs one synchronization round. `Ok(None)` means another round already holds the lock, so
/// there was nothing to do.
pub async fn sync_round(
    paths: &AppPaths,
    backend: &AnyBackend,
    key: &Key,
    options: &SyncOptions<'_>,
) -> Result<Option<SyncReport>, SyncError> {
    let Some(_lock) = SyncLock::acquire(&paths.sync_lock_file())? else {
        return Ok(None);
    };
    Ok(Some(sync_once(paths, backend, key, options).await?))
}
