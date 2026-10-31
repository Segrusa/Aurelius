//! One-time imports: a v0.6 data folder into a fresh v0.7 one, and the fresh
//! start of a protocol 3 workspace into v0.11.

use aurelius_storage::fresh_start;
use aurelius_storage::import::import;
use std::path::{Path, PathBuf};

const USAGE: &str = "Usage: aurelius-import --from <v0.6 data dir> --to <new data dir> [--dry-run]
       aurelius-import fresh-start [--dry-run] [--names] <data dir>

Copies projects, employees, memories, ideas and recurring tasks from a v0.6 data
folder (data/aurelius.sqlite + blobs/) into a fresh v0.7 folder. The new folder
must not contain a database yet. --dry-run only prints the counts.

Not copied: providers and connections (their secrets live in the OS credential
store; add them again in Settings), conversations, messages, runs, tasks,
approvals, shares, attachments, usage, events, agent functions and API keys.
Forgotten memories are skipped. Project folders on disk stay where they are.

fresh-start: the move of a protocol 3 workspace (v0.9/v0.10) to v0.11. Its
data/ and blobs/ are set aside in <data dir>/pre-v0.11/ and the employees,
projects, memory and configuration are imported into a new database; tasks,
conversations and the rest of the history stay in pre-v0.11/. Schedules,
heartbeats and agent functions come over paused. Stop Aurelius first.
--dry-run runs the whole import from a copy in a private temporary folder
outside <data dir> and writes nothing in <data dir> (a read-only copy works).
It stops with the same reason as the real run would (an existing pre-v0.11/,
tables without a decision) and says when there is nothing to do. The report
shows counts; --names adds department names.";

fn usage(code: i32) -> ! {
    eprintln!("{USAGE}");
    std::process::exit(code)
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("aurelius-import: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fresh-start") {
        return fresh_start_command(&args[1..]);
    }
    let (mut from, mut to, mut dry_run) = (None, None, false);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--from" => from = args.next().map(PathBuf::from),
            "--to" => to = args.next().map(PathBuf::from),
            "--dry-run" => dry_run = true,
            _ => usage(if arg == "--help" || arg == "-h" { 0 } else { 1 }),
        }
    }
    let (Some(from), Some(to)) = (from, to) else { usage(1) };
    match import(&from, &to, dry_run) {
        Ok(r) => {
            println!("{}", if dry_run { "Dry run, nothing written. Would copy:" } else { "Imported:" });
            println!("  projects: {}\n  employees: {}\n  memories: {}\n  ideas: {}\n  recurring tasks: {}", r.projects, r.managers, r.memories, r.ideas, r.recurring_tasks);
            for s in &r.skipped {
                println!("  skipped: {s}");
            }
        }
        Err(e) => fail(e),
    }
}

fn fresh_start_command(args: &[String]) {
    let (mut dry_run, mut names, mut root) = (false, false, None);
    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--names" => names = true,
            "--help" | "-h" => usage(0),
            flag if flag.starts_with('-') => usage(1),
            path if root.is_none() => root = Some(PathBuf::from(path)),
            _ => usage(1),
        }
    }
    let Some(root) = root else { usage(1) };
    if dry_run {
        match fresh_start::dry_run(&root) {
            Ok(Some(report)) => {
                println!("Dry run, nothing written in the data folder. The fresh start would give:");
                print_lines(&report.lines(names));
            }
            Ok(None) => println!("Nothing to do: {} holds no protocol 3 (or older) database.", root.display()),
            Err(e) => fail(e),
        }
        return;
    }
    // The daemon holds this lock while it runs; it must not see its data move.
    let _lock = hold_daemon_lock(&root);
    let environment = std::fs::read_to_string(root.join("data").join("environment")).map(|e| e.trim().to_string()).unwrap_or_else(|_| "production".into());
    match fresh_start::upgrade(&root, Some((env!("CARGO_PKG_VERSION"), &environment))) {
        Ok(Some(report)) => {
            println!("Fresh start done. The old workspace is in {}.", root.join(fresh_start::BACKUP_DIR).display());
            print_lines(&report.lines(names));
        }
        Ok(None) => println!("Nothing to do: {} holds no protocol 3 (or older) database.", root.display()),
        Err(e) => fail(e),
    }
}

fn hold_daemon_lock(root: &Path) -> std::fs::File {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("daemon.lock"))
        .unwrap_or_else(|e| fail(format!("cannot open the daemon lock: {e}")));
    match file.try_lock() {
        Ok(()) => file,
        Err(std::fs::TryLockError::WouldBlock) => fail("Aurelius is running on this data folder; stop it first"),
        Err(std::fs::TryLockError::Error(e)) => fail(format!("cannot take the daemon lock: {e}")),
    }
}

fn print_lines(lines: &[String]) {
    for line in lines {
        println!("  {line}");
    }
}
