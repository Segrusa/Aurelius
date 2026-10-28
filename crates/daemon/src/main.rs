// Release builds are GUI programs: no console window (dev keeps its console for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::fs::OpenOptions;

fn main() -> anyhow::Result<()> {
    let root = aurelius_host::data_dir()?;
    aurelius_host::ensure_private_dir(&root)?;
    std::fs::create_dir_all(root.join("logs"))?;
    // Technical log only: ids and states, never prompt text or file content.
    let log = OpenOptions::new().create(true).append(true).open(root.join("logs").join("daemon.log"))?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("AURELIUS_LOG").unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::sync::Mutex::new(log))
        .with_ansi(false)
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let stopped = runtime.block_on(aurelius_daemon::run_service(root.clone()))?;
    drop(runtime);
    if stopped == aurelius_daemon::Stopped::Restart {
        // The installed program by name: after an update this process runs
        // from the renamed `.old` file. The instance lock is free again.
        let exe = aurelius_host::sibling_executable("aurelius-daemon")?;
        tracing::info!(exe = %exe.display(), "starting again");
        aurelius_host::spawn_daemon_detached(&exe, &root)?;
    }
    Ok(())
}
