// Release builds are GUI programs: no console window (dev keeps its console for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::fs::OpenOptions;

fn main() -> anyhow::Result<()> {
    let root = aurelius_host::data_dir()?;
    aurelius_host::ensure_private_dir(&root)?;
    // `aurelius-daemon vault-key import [--file PATH]`: the key from standard
    // input (a pipe or a file, never a terminal) or a 0600 file, never from the
    // command line; only while the daemon is stopped.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("vault-key") {
        let own_root = aurelius_host::default_data_dir().ok().is_some_and(|d| d == root);
        let outcome = match args.get(1).map(String::as_str) {
            Some("import") => {
                use aurelius_daemon::vault::import;
                use std::io::IsTerminal;
                import::parse_args(&args[2..])
                    .and_then(|source| import::refuse_terminal(&source, std::io::stdin().is_terminal()).map(|()| source))
                    .and_then(|source| import::run(&root, own_root, &source, &mut std::io::stdin().lock()))
            }
            _ => Err("usage: aurelius-daemon vault-key import [--file PATH]  (or: aurelius-daemon vault-key import < key.txt)".to_string()),
        };
        return match outcome {
            Ok(done) => {
                println!("{done}");
                Ok(())
            }
            Err(why) => {
                eprintln!("vault-key: {why}");
                std::process::exit(1);
            }
        };
    }
    std::fs::create_dir_all(root.join("logs"))?;
    // Technical log only: ids and states, never prompt text or file content.
    let log = OpenOptions::new().create(true).append(true).open(root.join("logs").join("daemon.log"))?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("AURELIUS_LOG").unwrap_or_else(|_| "info".into()),
        )
        // Every record redacted of the vault's values (SEC0 E14 c).
        .with_writer(std::sync::Mutex::new(aurelius_runtime::redact::Writer(log)))
        .with_ansi(false)
        .init();
    aurelius_daemon::install_panic_hook();

    // The decoy hash of `user.use` (S5-4), beside the start instead of on the first unknown board member.
    std::thread::spawn(aurelius_daemon::api::warm_up);
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
