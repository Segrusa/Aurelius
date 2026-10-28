//! aurelius-ctl: diagnostic CLI for the daemon. Uses the same protocol as the desktop.

use aurelius_client::{Client, Target, envelope, servers::Servers};
use aurelius_domain::{Command, ProjectId, project::{CreateProject, RenameProject, SetProjectDescription}};
use aurelius_protocol::HostWrite;
use std::time::Duration;
use tokio::sync::mpsc;

const USAGE: &str = "usage: aurelius-ctl [--server <name|host_id>] <command>
  health                              daemon status
  projects                            list projects
  create-project <name> [description] create a project
  rename-project <id> <version> <name>
  set-project-description <id> <version> [description]
                                      change the description (none clears it)
  jobs                                list jobs and progress
  connections                         list connections and defaults
  add-connection <provider> <name> [runtime_path]
                                      provider: claude_code | anthropic_api | openai_api
  set-key <connection_id>             read an API key from stdin into the OS credential store
  verify <connection_id>              probe status and models (no prompt is sent)
  use <connection_id> [model]         make it the global default for everything (employees, memory)
  watch [after_seq]                   print events (replay, then live)
  remote status|on|off                remote access for phones and computers
  remote pair                         a one-time pairing code (QR payload and text for a computer)
  remote allow [--owner]|refuse       decide the oldest waiting device; --owner gives a computer full access
  remote revoke <client_id>           remove a paired device
  autostart on|off|status             start the background service at sign-in (installed production only)
  load [watch]                        the server's load and when new runs start (watch: live)
  load set [max_runs=N] [cpu_high=N] [cpu_low=N] [mem_min_mb=N] [enabled=on|off]
                                      change the load limits (applied at once, kept)
  restart                             restart the server (it starts itself again)
  shutdown                            stop the daemon
  server pair <pairing text>          pair this computer with a server elsewhere (full access)
  server list                         paired servers
  server forget <name|host_id>        remove a pairing from this computer
  --server <name|host_id>             run the command against that server instead of this computer";

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let server = match args.iter().position(|a| a == "--server") {
        Some(i) if i + 1 < args.len() => {
            let name = args.remove(i + 1);
            args.remove(i);
            Some(name)
        }
        Some(_) => anyhow::bail!("--server needs a name or host id"),
        None => None,
    };
    let Some(cmd) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };
    if cmd == "autostart" {
        // Registered on this computer; needs no running service (a server without the app).
        use aurelius_host::startup;
        match args.get(1).map(String::as_str) {
            Some("on") => startup::set_enabled(true).map_err(anyhow::Error::msg)?,
            Some("off") => startup::set_enabled(false).map_err(anyhow::Error::msg)?,
            Some("status") | None => {}
            Some(_) => {
                println!("{USAGE}");
                return Ok(());
            }
        }
        println!("start at sign-in: {}", if startup::enabled() { "on" } else { "off" });
        return Ok(());
    }
    let root = aurelius_host::data_dir()?;
    if cmd == "server" {
        return servers_command(&root, &args[1..]).await;
    }
    if cmd == "shutdown" {
        // Over the internal bridge channel; never starts a daemon just to stop it.
        aurelius_client::bridge::shutdown_daemon(&root).await?;
        println!("daemon is shutting down");
        return Ok(());
    }
    let target = match &server {
        None => Target::Local { root: root.clone(), spawn: true },
        Some(name) => {
            let saved = Servers::load(&root);
            let host_id = find_server(&saved, name)?;
            Target::Remote(saved.credentials(host_id)?)
        }
    };
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let client: Client = Client::open(&target, "aurelius-ctl", events_tx, None).await?;

    match (cmd, &args[1..]) {
        ("health", _) => println!("{:#?}", client.health().await?),
        ("projects", _) => {
            let snapshot = client.projects().await?;
            println!("as_of_seq {}", snapshot.as_of_seq);
            for p in snapshot.projects {
                println!("{}  v{}  {}  {}", p.id, p.version, p.name, p.description.unwrap_or_default());
            }
        }
        ("create-project", [name, rest @ ..]) => {
            let description = (!rest.is_empty()).then(|| rest.join(" "));
            let env = envelope(Command::CreateProject(CreateProject { name: name.clone(), description, folder: None }), None);
            println!("{:#?}", client.execute(env).await?);
        }
        ("rename-project", [id, version, name @ ..]) => {
            let env = envelope(
                Command::RenameProject(RenameProject { project_id: ProjectId::parse(id)?, name: name.join(" ") }),
                Some(version.parse()?),
            );
            println!("{:#?}", client.execute(env).await?);
        }
        ("set-project-description", [id, version, description @ ..]) => {
            let env = envelope(
                Command::SetProjectDescription(SetProjectDescription {
                    project_id: ProjectId::parse(id)?,
                    description: description.join(" "),
                }),
                Some(version.parse()?),
            );
            println!("{:#?}", client.execute(env).await?);
        }
        ("connections", _) => {
            let snapshot = client.connections().await?;
            for c in snapshot.connections {
                println!(
                    "{}  {:<13} {:<10} key:{}  {}  {}",
                    c.id, c.provider, c.status, if c.has_secret { "yes" } else { "no" }, c.display_name, c.detail.unwrap_or_default()
                );
                if !c.models.is_empty() {
                    println!("    models: {}", c.models.join(", "));
                }
            }
        }
        ("add-connection", [provider, name, rest @ ..]) => {
            use aurelius_domain::connection::{AddProvider, ProviderKind};
            let provider = ProviderKind::parse(provider).ok_or_else(|| anyhow::anyhow!("unknown provider"))?;
            let env = envelope(
                Command::AddProvider(AddProvider { provider, display_name: name.clone(), runtime_path: rest.first().cloned(), default_model: None }),
                None,
            );
            println!("{:#?}", client.execute(env).await?.result);
        }
        ("remote", [sub, rest @ ..]) => {
            let print = |s: aurelius_protocol::remote::RemoteStatus| {
                println!("enabled {}  port {}  url {}", s.enabled, s.port, s.url.clone().unwrap_or_else(|| "-".into()));
                println!("tailscale online {} serving {} {}", s.tailscale.online, s.tailscale.serving, s.tailscale.error.clone().unwrap_or_default());
                for c in &s.clients {
                    let owner = if c.capabilities.contains(&aurelius_protocol::remote::Capability::Owner) { "  owner" } else { "" };
                    println!("device {}  {} ({}){owner}  connected {}  push {}", c.id, c.name, c.platform, c.connected, c.push);
                }
                for p in &s.pending {
                    let asks = if p.owner { "  asks for full access (allow with --owner)" } else { "" };
                    println!("waiting {}  {} ({}){asks}", p.pairing_id, p.device_name, p.platform);
                }
                if let Some(e) = s.error {
                    println!("error: {e}");
                }
            };
            match (sub.as_str(), rest) {
                ("status", _) => print(client.remote_status().await?),
                ("on", _) => print(client.remote(HostWrite::RemoteEnable).await?),
                ("off", _) => print(client.remote(HostWrite::RemoteDisable).await?),
                ("pair", _) => {
                    let code = client.pairing_start().await?;
                    println!("code {}\nqr {}\ncomputer {}", code.code, code.qr, code.pairing_text);
                }
                ("allow" | "refuse", flags) => {
                    let owner = flags.iter().any(|f| f == "--owner");
                    let status = client.remote_status().await?;
                    let Some(request) = status.pending.first() else { anyhow::bail!("no device is waiting") };
                    let decide = HostWrite::RemoteDecidePairing { pairing_id: request.pairing_id.clone(), approve: sub == "allow", owner };
                    print(client.remote(decide).await?);
                }
                ("revoke", [id]) => print(client.remote(HostWrite::RemoteRevoke { client_id: aurelius_domain::ClientId::parse(id)? }).await?),
                _ => println!("{USAGE}"),
            }
        }
        ("set-key", [id]) => {
            // Read from stdin so the key never appears in argv, shell history or process lists.
            let mut key = String::new();
            std::io::stdin().read_line(&mut key)?;
            let connection_id = aurelius_domain::ProviderId::parse(id)?;
            client.set_secret(connection_id, key.trim().to_owned()).await?;
            println!("saved in the OS credential store");
        }
        ("verify", [id]) => {
            use aurelius_domain::connection::VerifyProvider;
            let connection_id = aurelius_domain::ProviderId::parse(id)?;
            client.execute(envelope(Command::VerifyProvider(VerifyProvider { connection_id }), None)).await?;
            tokio::time::sleep(Duration::from_secs(4)).await;
            let snapshot = client.connections().await?;
            if let Some(c) = snapshot.connections.into_iter().find(|c| c.id == connection_id) {
                println!("{} · {} · models: {}", c.status, c.detail.unwrap_or_default(), c.models.join(", "));
            }
        }
        ("use", [id, rest @ ..]) => {
            use aurelius_domain::connection::{ConfigScope, Purpose, RunDefaults, SetRunDefaults};
            use aurelius_domain::employee::Setting;
            let connection_id = aurelius_domain::ProviderId::parse(id)?;
            let model = rest.first().map(|m| Setting::Value(m.clone())).unwrap_or(Setting::Inherit);
            for purpose in [Purpose::Employee, Purpose::Memory] {
                let settings = RunDefaults { connection: Setting::Value(connection_id), model: model.clone(), persona: Setting::Inherit, effort: Setting::Inherit };
                client.execute(envelope(Command::SetRunDefaults(SetRunDefaults { scope: ConfigScope::Global, purpose, settings }), None)).await?;
            }
            println!("global default for employees and memory set");
        }
        ("jobs", _) => {
            for job in client.jobs().await?.jobs {
                println!("{}  {:<9} {:>5.1}%  {}", job.id, job.state.as_str(), job.progress_permille as f32 / 10.0, job.label);
            }
        }
        ("watch", rest) => {
            let after = rest.first().map(|s| s.parse()).transpose()?.unwrap_or(0);
            client.subscribe(after).await?;
            loop {
                tokio::select! {
                    Some(e) = events.recv() => println!("#{:<5} {:<28} {}", e.seq, e.kind.as_str(), e.aggregate_id),
                    _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
                }
            }
        }
        ("load", []) => print_load(&client.load_snapshot().await?),
        ("load", [watch]) if watch == "watch" => {
            let mut loads = client.load_changes();
            print_load(&client.load_snapshot().await?);
            // Only live pushes: no replay of older events.
            client.subscribe(client.projects().await?.as_of_seq).await?;
            while let Some(snapshot) = loads.recv().await {
                print_load(&snapshot);
            }
        }
        ("load", [set, pairs @ ..]) if set == "set" && !pairs.is_empty() => {
            let mut update = aurelius_protocol::LoadLimitsUpdate::default();
            for pair in pairs {
                let Some((key, value)) = pair.split_once('=') else { anyhow::bail!("expected key=value, got {pair}") };
                match key {
                    "max_runs" => update.max_runs = Some(value.parse()?),
                    "cpu_high" => update.cpu_high = Some(value.parse()?),
                    "cpu_low" => update.cpu_low = Some(value.parse()?),
                    "mem_min_mb" => update.mem_min_mb = Some(value.parse()?),
                    "enabled" => update.enabled = Some(matches!(value, "on" | "true" | "1")),
                    _ => anyhow::bail!("unknown load limit {key}"),
                }
            }
            print_load(&client.set_load_limits(update).await?);
        }
        ("restart", _) => {
            client.restart_server().await?;
            println!("server is restarting");
        }
        _ => println!("{USAGE}"),
    }
    Ok(())
}

fn print_load(l: &aurelius_protocol::LoadSnapshot) {
    let measured = if l.checks_active { format!("cpu {:.0}%  memory free {} of {} MB", l.cpu_pct, l.mem_free_mb, l.mem_total_mb) } else { "cpu and memory not measured here".into() };
    let hold = match &l.hold {
        None => String::new(),
        Some(h) => format!("  (held: {:?})", h.kind),
    };
    println!("{measured}  running {}  waiting {}  gate {:?}{hold}", l.running, l.held, l.gate);
    let m = &l.limits;
    println!(
        "limits: max_runs {}  cpu_high {}%  cpu_low {}%  mem_min_mb {}  cpu/memory checks {}",
        m.max_runs, m.cpu_high, m.cpu_low, m.mem_min_mb, if m.enabled { "on" } else { "off" }
    );
}

fn find_server(saved: &Servers, name: &str) -> anyhow::Result<aurelius_domain::HostId> {
    saved
        .servers
        .iter()
        .find(|s| s.host_id.to_string() == name || s.host_name.eq_ignore_ascii_case(name))
        .map(|s| s.host_id)
        .ok_or_else(|| anyhow::anyhow!("no paired server \"{name}\" (see: aurelius-ctl server list)"))
}

/// `server pair|list|forget`: this computer's pairings with servers elsewhere.
async fn servers_command(root: &std::path::Path, args: &[String]) -> anyhow::Result<()> {
    let mut saved = Servers::load(root);
    match (args.first().map(String::as_str), &args[1.min(args.len())..]) {
        (Some("pair"), [text, ..]) => {
            let name = aurelius_host::host_name();
            let server = aurelius_client::remote::pair(text, &name, |progress| {
                let aurelius_client::remote::PairProgress::Waiting { host_name } = progress;
                println!("waiting for \"Allow as owner\" on {host_name}…");
            })
            .await?;
            saved.add(&server)?;
            saved.save(root)?;
            println!("paired with {} ({}) as {}", server.host_name, server.url, server.client_id);
        }
        (Some("list") | None, _) => {
            for s in &saved.servers {
                let active = if saved.active == Some(s.host_id) { "  (active)" } else { "" };
                println!("{}  {}  {}  {}{active}", s.host_id, s.host_name, s.url, s.environment);
            }
            if saved.servers.is_empty() {
                println!("no paired servers");
            }
        }
        (Some("forget"), [name]) => {
            let host_id = find_server(&saved, name)?;
            saved.forget(host_id)?;
            saved.save(root)?;
            println!("forgotten");
        }
        _ => println!("{USAGE}"),
    }
    Ok(())
}
