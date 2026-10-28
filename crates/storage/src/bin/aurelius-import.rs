//! One-time import from a v0.6 data folder into a fresh v0.7 one.

use aurelius_storage::import::import;
use std::path::PathBuf;

const USAGE: &str = "Usage: aurelius-import --from <v0.6 data dir> --to <new data dir> [--dry-run]

Copies projects, employees, memories, ideas and recurring tasks from a v0.6 data
folder (data/aurelius.sqlite + blobs/) into a fresh v0.7 folder. The new folder
must not contain a database yet. --dry-run only prints the counts.

Not copied: providers and connections (their secrets live in the OS credential
store; add them again in Settings), conversations, messages, runs, tasks,
approvals, shares, attachments, usage, events, agent functions and API keys.
Forgotten memories are skipped. Project folders on disk stay where they are.";

fn main() {
    let (mut from, mut to, mut dry_run) = (None, None, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--from" => from = args.next().map(PathBuf::from),
            "--to" => to = args.next().map(PathBuf::from),
            "--dry-run" => dry_run = true,
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(if arg == "--help" || arg == "-h" { 0 } else { 1 });
            }
        }
    }
    let (Some(from), Some(to)) = (from, to) else {
        eprintln!("{USAGE}");
        std::process::exit(1);
    };
    match import(&from, &to, dry_run) {
        Ok(r) => {
            println!("{}", if dry_run { "Dry run, nothing written. Would copy:" } else { "Imported:" });
            println!("  projects: {}\n  employees: {}\n  memories: {}\n  ideas: {}\n  recurring tasks: {}", r.projects, r.managers, r.memories, r.ideas, r.recurring_tasks);
            for s in &r.skipped {
                println!("  skipped: {s}");
            }
        }
        Err(e) => {
            eprintln!("aurelius-import: {e}");
            std::process::exit(1);
        }
    }
}
