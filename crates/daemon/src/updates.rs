//! Automatic updates for the installed production app.
//!
//! Releases come from the private GitHub repository through the user's own
//! GitHub CLI login (Aurelius never sees a GitHub token). A package is used
//! only when its Ed25519 signature verifies against the compiled-in release
//! key. The server (service + aurelius-ctl) and the desktop app are released
//! separately (`server-v*`, `desktop-v*`), each with its own version; this
//! service installs both. Applying the server: running programs are renamed to
//! `.old` (allowed on Windows) and the new ones placed beside them; the service
//! stops and starts the new version. Applying the app: the app is a folder that
//! can't move while it runs, so the app closes and this service then swaps the
//! folder and opens the new one. Any failure restores the old files.

use crate::Shared;
use aurelius_protocol::UpdateStatus;
use aurelius_host::updater::Product;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;

const CHECK_EVERY_MS: i64 = 6 * 60 * 60 * 1000;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Folder of the running programs, if this is an installed production build.
fn install_dir() -> Result<PathBuf, String> {
    if aurelius_host::Environment::current() != aurelius_host::Environment::Production {
        return Err("Updates are only for the production app.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no install folder")?.to_path_buf();
    if dir.components().any(|c| c.as_os_str() == "target") {
        return Err("This is a build from source; install it with scripts/install-production.ps1.".into());
    }
    Ok(dir)
}

fn gh() -> PathBuf {
    let default = PathBuf::from(r"C:\Program Files\GitHub CLI\gh.exe");
    if default.exists() { default } else { PathBuf::from("gh") }
}

async fn gh_output(args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(gh());
    command.args(args).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = command.output().await.map_err(|_| "GitHub CLI not found. Install it and sign in with `gh auth login`.".to_string())?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.contains("auth login") || err.contains("not logged") {
            "GitHub CLI is not signed in. Run `gh auth login`.".into()
        } else if err.contains("release not found") {
            "No release has been published yet.".into()
        } else {
            format!("GitHub: {}", err.lines().next().unwrap_or("unknown error"))
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether this computer has the desktop app: a server-only install
/// (`install-production.ps1 -Role server`) has none.
fn has_app(dir: &Path) -> bool {
    let server_only = std::fs::read_to_string(dir.join("role.txt")).is_ok_and(|r| r.trim() == "server");
    !server_only && aurelius_host::updater::app_program(dir).exists()
}

/// The installed app's version (written beside it at install and update);
/// empty when unknown, which makes any app release count as newer.
fn app_version(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(aurelius_host::updater::DESKTOP_VERSION_FILE)).map(|v| v.trim().to_owned()).unwrap_or_default()
}

fn is_newer(latest: &str, current: &str) -> bool {
    if current.is_empty() { aurelius_host::updater::parse_version(latest).is_some() } else { aurelius_host::updater::is_newer(latest, current) }
}

fn stored(shared: &Shared, product: Product) -> UpdateStatus {
    let server = shared.update.lock().ok().map(|s| s.clone()).unwrap_or_default();
    match product {
        Product::Server => server,
        Product::Desktop => server.app.map(|a| *a).unwrap_or_default(),
    }
}

fn store(shared: &Shared, product: Product, status: &UpdateStatus) {
    if let Ok(mut s) = shared.update.lock() {
        match product {
            Product::Server => {
                let app = s.app.take();
                *s = UpdateStatus { app, ..status.clone() };
            }
            Product::Desktop => s.app = Some(Box::new(UpdateStatus { app: None, ..status.clone() })),
        }
    }
}

fn base_status(shared: &Shared, product: Product) -> UpdateStatus {
    let mut status = stored(shared, product);
    status.app = None;
    let dir = install_dir();
    status.current = match product {
        Product::Server => VERSION.into(),
        Product::Desktop => dir.as_deref().map(app_version).unwrap_or_default(),
    };
    match dir {
        Ok(_) => {
            status.enabled = true;
            status.reason = None;
        }
        Err(reason) => {
            status.enabled = false;
            status.reason = Some(reason);
        }
    }
    status
}

/// Server status with the app's status inside (when this computer has the app).
fn combined(shared: &Shared) -> UpdateStatus {
    let mut status = base_status(shared, Product::Server);
    if install_dir().is_ok_and(|d| has_app(&d)) {
        status.app = Some(Box::new(base_status(shared, Product::Desktop)));
    }
    status
}

/// The published release tags in the repository.
async fn release_tags() -> Result<Vec<String>, String> {
    let json = gh_output(&["release", "list", "--repo", aurelius_host::updater::REPO, "--exclude-drafts", "--limit", "100", "--json", "tagName"]).await?;
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| format!("GitHub: {e}"))?;
    Ok(value.as_array().into_iter().flatten().filter_map(|r| r["tagName"].as_str().map(str::to_owned)).collect())
}

/// Fill in the latest release of `product` (and its notes) from `tags`.
async fn check_product(product: Product, tags: &Result<Vec<String>, String>, status: &mut UpdateStatus) {
    status.checked_at_ms = aurelius_storage::now_ms();
    let tags = match tags {
        Ok(tags) => tags,
        Err(e) => {
            status.error = Some(e.clone());
            return;
        }
    };
    let latest = product.latest(tags.iter().map(String::as_str)).map(str::to_owned);
    status.notes = None;
    if let Some(version) = &latest
        && let Ok(json) = gh_output(&["release", "view", &product.tag(version), "--repo", aurelius_host::updater::REPO, "--json", "body"]).await
    {
        let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        status.notes = value["body"].as_str().map(|b| b.chars().take(2_000).collect());
    }
    status.available = latest.as_deref().is_some_and(|l| is_newer(l, &status.current));
    status.latest = latest;
    status.error = None;
}

/// Current status of the server, with the app's inside; `check` asks GitHub
/// for the latest release of each product first.
pub async fn status(shared: &Arc<Shared>, check: bool) -> UpdateStatus {
    let status = combined(shared);
    if !status.enabled || !check {
        return status;
    }
    let tags = release_tags().await;
    let mut server = base_status(shared, Product::Server);
    check_product(Product::Server, &tags, &mut server).await;
    store(shared, Product::Server, &server);
    if status.app.is_some() {
        let mut app = base_status(shared, Product::Desktop);
        check_product(Product::Desktop, &tags, &mut app).await;
        store(shared, Product::Desktop, &app);
    }
    combined(shared)
}

/// Download, verify and unpack the latest release of `product` into the data folder.
async fn prepare_product(shared: &Arc<Shared>, product: Product) -> UpdateStatus {
    let mut status = base_status(shared, product);
    if !status.enabled {
        return status;
    }
    check_product(product, &release_tags().await, &mut status).await;
    let version = match (&status.latest, status.available) {
        (Some(version), true) => version.clone(),
        _ => {
            store(shared, product, &status);
            return status;
        }
    };
    let staging = shared.root.join("updates").join(product.name()).join(&version);
    let zip = product.zip_name(&version);
    let result = async {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        tokio::fs::create_dir_all(&staging).await.map_err(|e| e.to_string())?;
        let dir = staging.to_string_lossy().into_owned();
        gh_output(&["release", "download", &product.tag(&version), "--repo", aurelius_host::updater::REPO, "--pattern", &zip, "--pattern", &format!("{zip}.sig"), "--dir", &dir, "--clobber"]).await?;
        let zip_path = staging.join(&zip);
        let bytes = tokio::fs::read(&zip_path).await.map_err(|e| format!("download missing: {e}"))?;
        let signature = tokio::fs::read_to_string(staging.join(format!("{zip}.sig"))).await.map_err(|e| format!("signature missing: {e}"))?;
        aurelius_host::updater::verify(&bytes, &signature).map_err(|e| e.to_string())?;
        let out = staging.join("bin");
        tokio::task::spawn_blocking(move || aurelius_host::updater::extract(&zip_path, &out, product)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        Ok::<_, String>(())
    }
    .await;
    match result {
        Ok(()) => {
            status.staged = Some(version);
            status.error = None;
        }
        Err(e) => {
            status.staged = None;
            status.error = Some(e);
        }
    }
    store(shared, product, &status);
    status
}

/// Prepare the server's latest release; returns the combined status.
pub async fn prepare(shared: &Arc<Shared>) -> UpdateStatus {
    prepare_product(shared, Product::Server).await;
    combined(shared)
}

/// Prepare the desktop app's latest release; returns the app's status.
pub async fn prepare_app(shared: &Arc<Shared>) -> UpdateStatus {
    if !install_dir().is_ok_and(|d| has_app(&d)) {
        return UpdateStatus { error: Some("This computer has no desktop app installed.".into()), ..base_status(shared, Product::Desktop) };
    }
    prepare_product(shared, Product::Desktop).await
}

fn staged_bin(shared: &Shared, product: Product) -> Result<(String, PathBuf, PathBuf), String> {
    let version = stored(shared, product).staged.ok_or("No verified update is ready.")?;
    let dir = install_dir()?;
    let bin = shared.root.join("updates").join(product.name()).join(&version).join("bin");
    Ok((version, dir, bin))
}

/// Swap the server programs for the staged version. On success the service
/// stops and starts the new version itself (a local app reconnects). On
/// failure the old programs are restored. The app is not touched.
pub async fn apply(shared: &Arc<Shared>) -> Result<UpdateStatus, String> {
    let (version, dir, bin) = staged_bin(shared, Product::Server)?;
    swap(&dir, &bin, Product::Server).map_err(|e| format!("Update not applied (old version restored): {e}"))?;
    tracing::info!(%version, "server update applied; stopping for restart");
    let status = combined(shared);
    shared.restart_after_reply();
    Ok(status)
}

/// Accepts the staged app version. The app closes on this answer; this service then swaps the
/// folder and opens the new app (`install_app_when_closed`).
pub async fn apply_app(shared: &Arc<Shared>) -> Result<UpdateStatus, String> {
    let (version, dir, bin) = staged_bin(shared, Product::Desktop)?;
    if !has_app(&dir) {
        return Err("This computer has no desktop app installed.".into());
    }
    // An app of one program restarts itself by name on this answer: it must not find itself.
    retire_single_program(&dir);
    tracing::info!(%version, "app update accepted; waiting for the app to close");
    tokio::spawn(install_app_when_closed(shared.clone(), version, dir, bin));
    let mut status = base_status(shared, Product::Desktop);
    status.staged = None;
    status.available = false;
    store(shared, Product::Desktop, &status);
    Ok(status)
}

/// How long the app gets to close; after that the old one stays and the status says why.
const APP_CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(60);
const APP_CLOSE_STEP: std::time::Duration = std::time::Duration::from_millis(500);

async fn install_app_when_closed(shared: Arc<Shared>, version: String, dir: PathBuf, bin: PathBuf) {
    let started = std::time::Instant::now();
    let result = loop {
        tokio::time::sleep(APP_CLOSE_STEP).await;
        match swap_app(&dir, &bin) {
            Ok(()) => break Ok(()),
            Err(e) if started.elapsed() >= APP_CLOSE_WAIT => break Err(e),
            Err(_) => {}
        }
    };
    match result {
        Ok(()) => {
            if let Err(e) = std::fs::write(dir.join(aurelius_host::updater::DESKTOP_VERSION_FILE), &version) {
                tracing::warn!(error = %e, "could not write the app's version");
            }
            tracing::info!(%version, "app update applied");
        }
        Err(e) => {
            restore_single_program(&dir);
            tracing::warn!(error = %e, "app update not applied");
            let mut status = base_status(&shared, Product::Desktop);
            status.error = Some(format!("Update not applied (old version kept): {e}. Close the app and try again."));
            store(&shared, Product::Desktop, &status);
        }
    }
    open_app(&dir);
}

fn open_app(dir: &Path) {
    let program = aurelius_host::updater::app_program(dir);
    let folder = program.parent().map_or_else(|| dir.to_path_buf(), Path::to_path_buf);
    if let Err(e) = std::process::Command::new(&program).current_dir(folder).spawn() {
        tracing::warn!(error = %e, "could not open the app");
    }
}

fn old_app_dir(dir: &Path) -> PathBuf {
    dir.join(format!("{}.old", aurelius_host::updater::APP_DIR))
}

fn single_program_old(dir: &Path) -> (PathBuf, PathBuf) {
    let name = aurelius_host::updater::APP_EXE;
    (dir.join(name), dir.join(format!("{name}.old")))
}

/// A running program may be renamed on Windows; `start` removes it at the next start.
fn retire_single_program(dir: &Path) {
    let (program, old) = single_program_old(dir);
    if program.exists() {
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(&program, &old);
    }
}

fn restore_single_program(dir: &Path) {
    let (program, old) = single_program_old(dir);
    if old.exists() && !program.exists() {
        let _ = std::fs::rename(&old, &program);
    }
}

/// Puts the staged folder in place of the app's. Moving a folder fails while the app still runs
/// from it, which is the caller's sign to wait. Any failure leaves the old folder in place.
fn swap_app(dir: &Path, bin: &Path) -> std::io::Result<()> {
    let app = dir.join(aurelius_host::updater::APP_DIR);
    let old = old_app_dir(dir);
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    let had = app.exists();
    if had {
        std::fs::rename(&app, &old)?;
    }
    let placed = std::fs::rename(bin, &app).or_else(|_| copy_tree(bin, &app));
    if let Err(e) = placed {
        let _ = std::fs::remove_dir_all(&app);
        if had {
            std::fs::rename(&old, &app)?;
        }
        return Err(e);
    }
    Ok(())
}

/// For staging on another drive than the install folder, where a rename can't.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Replace the product's programs in `dir` with those in `bin`; any failure
/// restores the old ones.
fn swap(dir: &Path, bin: &Path, product: Product) -> std::io::Result<()> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = (|| {
        for name in product.binaries() {
            let target = dir.join(name);
            let old = dir.join(format!("{name}.old"));
            let _ = std::fs::remove_file(&old);
            if target.exists() {
                std::fs::rename(&target, &old)?;
                moved.push((old.clone(), target.clone()));
            }
            std::fs::copy(bin.join(name), &target)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for (old, target) in moved.iter().rev() {
            let _ = std::fs::remove_file(target);
            let _ = std::fs::rename(old, target);
        }
    }
    result
}

/// Startup: remove `.old` programs from a previous update; periodic checks.
pub fn start(shared: Arc<Shared>) {
    if let Ok(dir) = install_dir() {
        for name in aurelius_host::updater::BINARIES {
            // An app still running from its `.old` file keeps it; that's fine.
            let _ = std::fs::remove_file(dir.join(format!("{name}.old")));
        }
        // A swap that stopped halfway leaves only the old folder: it is still the app.
        let (app, old) = (dir.join(aurelius_host::updater::APP_DIR), old_app_dir(&dir));
        if old.exists() {
            if app.exists() {
                let _ = std::fs::remove_dir_all(&old);
            } else {
                let _ = std::fs::rename(&old, &app);
            }
        }
        let _ = std::fs::remove_dir_all(shared.root.join("updates"));
    } else {
        return;
    }
    tokio::spawn(async move {
        loop {
            let last = shared.update.lock().ok().map(|s| s.checked_at_ms).unwrap_or(0);
            if aurelius_storage::now_ms() - last >= CHECK_EVERY_MS {
                let s = status(&shared, true).await;
                if s.available {
                    tracing::info!(latest = ?s.latest, "server update available");
                }
                if let Some(app) = s.app.filter(|a| a.available) {
                    tracing::info!(latest = ?app.latest, "app update available");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin_with_everything() -> tempfile::TempDir {
        let bin = tempfile::tempdir().unwrap();
        for name in aurelius_host::updater::BINARIES {
            std::fs::write(bin.path().join(name), "new").unwrap();
        }
        bin
    }

    fn installed() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in aurelius_host::updater::BINARIES {
            std::fs::write(dir.path().join(name), "old").unwrap();
        }
        dir
    }

    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    #[test]
    fn a_server_update_leaves_the_app_alone() {
        let (dir, bin) = (installed(), bin_with_everything());
        swap(dir.path(), bin.path(), Product::Server).unwrap();
        assert_eq!(read(dir.path(), "aurelius-daemon.exe"), "new");
        assert_eq!(read(dir.path(), "aurelius-ctl.exe"), "new");
        assert_eq!(read(dir.path(), "aurelius.exe"), "old");
    }

    fn app_folder(root: &Path, text: &str) -> PathBuf {
        let app = root.join("resources");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(root.join("aurelius.exe"), text).unwrap();
        std::fs::write(app.join("app.asar"), text).unwrap();
        root.to_path_buf()
    }

    #[test]
    fn an_app_update_swaps_the_folder_and_leaves_the_server_alone() {
        let dir = installed();
        app_folder(&dir.path().join("app"), "old");
        std::fs::write(dir.path().join("app/left-behind"), "old").unwrap();
        let staged = tempfile::tempdir().unwrap();
        let bin = app_folder(&staged.path().join("bin"), "new");
        swap_app(dir.path(), &bin).unwrap();
        assert_eq!(read(&dir.path().join("app"), "aurelius.exe"), "new");
        assert_eq!(read(&dir.path().join("app/resources"), "app.asar"), "new");
        assert!(!dir.path().join("app/left-behind").exists());
        assert_eq!(read(dir.path(), "aurelius-daemon.exe"), "old");
        assert_eq!(read(dir.path(), "aurelius-ctl.exe"), "old");
    }

    #[test]
    fn a_failed_app_swap_keeps_the_old_folder() {
        let dir = installed();
        app_folder(&dir.path().join("app"), "old");
        let missing = dir.path().join("nothing-staged");
        assert!(swap_app(dir.path(), &missing).is_err());
        assert_eq!(read(&dir.path().join("app"), "aurelius.exe"), "old");
        assert!(!dir.path().join("app.old").exists());
    }

    #[test]
    fn a_folder_can_be_placed_across_drives_too() {
        let from = tempfile::tempdir().unwrap();
        let bin = app_folder(&from.path().join("bin"), "new");
        let to = tempfile::tempdir().unwrap();
        copy_tree(&bin, &to.path().join("app")).unwrap();
        assert_eq!(read(&to.path().join("app/resources"), "app.asar"), "new");
    }

    #[test]
    fn the_single_program_goes_out_of_the_way_and_can_come_back() {
        let dir = installed();
        retire_single_program(dir.path());
        assert!(!dir.path().join("aurelius.exe").exists());
        assert_eq!(read(dir.path(), "aurelius.exe.old"), "old");
        restore_single_program(dir.path());
        assert_eq!(read(dir.path(), "aurelius.exe"), "old");
    }

    #[test]
    fn a_failed_swap_restores_the_old_programs() {
        let (dir, bin) = (installed(), tempfile::tempdir().unwrap());
        std::fs::write(bin.path().join("aurelius-daemon.exe"), "new").unwrap(); // ctl missing
        assert!(swap(dir.path(), bin.path(), Product::Server).is_err());
        assert_eq!(read(dir.path(), "aurelius-daemon.exe"), "old");
        assert_eq!(read(dir.path(), "aurelius-ctl.exe"), "old");
    }

    #[test]
    fn a_server_only_install_has_no_app() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("aurelius-daemon.exe"), "old").unwrap();
        assert!(!has_app(dir.path()));
        app_folder(&dir.path().join("app"), "old");
        assert!(has_app(dir.path()));
        std::fs::write(dir.path().join("role.txt"), "server").unwrap();
        assert!(!has_app(dir.path()));
    }

    #[test]
    fn an_app_of_unknown_version_is_offered_the_release() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(app_version(dir.path()), "");
        assert!(is_newer("0.6.0", ""));
        std::fs::write(dir.path().join(aurelius_host::updater::DESKTOP_VERSION_FILE), "0.6.1\n").unwrap();
        assert_eq!(app_version(dir.path()), "0.6.1");
        assert!(!is_newer("0.6.1", &app_version(dir.path())));
        assert!(is_newer("0.6.2", &app_version(dir.path())));
    }
}
