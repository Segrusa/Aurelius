//! What the modules that serve the running service (power,
//! backups) need from the daemon that owns them.

use aurelius_storage::Store;
use std::path::Path;
use std::sync::Arc;

pub trait Service: Send + Sync + 'static {
    fn store(&self) -> &Arc<Store>;
    /// The data folder.
    fn root(&self) -> &Path;
    /// The app version, written into backups and exports.
    fn version(&self) -> &'static str;
    /// Turns `true` when the service stops.
    fn shutdown_signal(&self) -> tokio::sync::watch::Receiver<bool>;
    fn power(&self) -> &crate::power::Power;
    /// Work goes on after a restore.
    fn resume_work(&self);
    /// Stop now and start again.
    fn stop_for_restart(&self);
}

/// `stop_for_restart` in a moment, so the reply that asked for it reaches the
/// client first (update, restore, `server.restart`).
pub fn restart_after_reply(service: &Arc<impl Service>) {
    let service = service.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        service.stop_for_restart();
    });
}
