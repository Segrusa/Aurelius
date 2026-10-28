use std::process::ExitCode;

const HELP: &str = "cargo xtask <command>

Commands:
  gen-dart           Write the Dart protocol types to clients/mobile/lib/data/protocol/
  gen-dart --check   Exit non-zero and list the files that differ from the generator's output
  help               This text";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["gen-dart"] => write(),
        ["gen-dart", "--check"] => check(),
        [] | ["help"] | ["--help"] | ["-h"] => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("unknown arguments: {}\n\n{HELP}", args.join(" "));
            ExitCode::from(2)
        }
    }
}

fn write() -> ExitCode {
    let dir = xtask::output_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    for (name, content) in xtask::generate() {
        let path = dir.join(&name);
        if xtask::read_checked_in(&name).as_deref() == Some(content.as_str()) {
            println!("unchanged {}", path.display());
            continue;
        }
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote     {}", path.display());
    }
    ExitCode::SUCCESS
}

fn check() -> ExitCode {
    let drifted = xtask::drifted();
    if drifted.is_empty() {
        println!("the generated Dart files are up to date");
        return ExitCode::SUCCESS;
    }
    eprintln!("these files differ from the generator's output (run `cargo xtask gen-dart` and commit the result):");
    for name in drifted {
        eprintln!("  {name}");
    }
    ExitCode::FAILURE
}
