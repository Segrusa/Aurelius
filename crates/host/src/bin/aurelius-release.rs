//! Release tool.
//!   aurelius-release keygen          new signing key → OS credential store; prints the PUBLIC key
//!   aurelius-release export-key      prints the secret key (only for piping into `gh secret set`)
//!   aurelius-release sign <file>     signs with $AURELIUS_SIGNING_KEY (CI) → <file>.sig
//!   aurelius-release verify <file>   checks <file>.sig against the compiled-in public key

use std::io::Write;

const KEY_REF: &str = "release-signing-key";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            if aurelius_host::secrets::get(KEY_REF)?.is_some() && !args.iter().any(|a| a == "--force") {
                anyhow::bail!("a signing key already exists; use --force to replace it (old releases stay valid only with the old public key)");
            }
            let (secret, public) = aurelius_host::updater::generate_key()?;
            aurelius_host::secrets::set(KEY_REF, &secret)?;
            println!("{public}");
        }
        Some("export-key") => {
            let secret = aurelius_host::secrets::get(KEY_REF)?.ok_or_else(|| anyhow::anyhow!("no signing key; run keygen first"))?;
            print!("{secret}");
            std::io::stdout().flush()?;
        }
        Some("sign") => {
            let file = args.get(1).ok_or_else(|| anyhow::anyhow!("usage: sign <file>"))?;
            let secret = std::env::var("AURELIUS_SIGNING_KEY").map_err(|_| anyhow::anyhow!("AURELIUS_SIGNING_KEY is not set"))?;
            let signature = aurelius_host::updater::sign(&std::fs::read(file)?, &secret)?;
            std::fs::write(format!("{file}.sig"), signature)?;
            println!("signed {file}");
        }
        Some("verify") => {
            let file = args.get(1).ok_or_else(|| anyhow::anyhow!("usage: verify <file>"))?;
            let signature = std::fs::read_to_string(format!("{file}.sig"))?;
            aurelius_host::updater::verify(&std::fs::read(file)?, &signature)?;
            println!("signature OK");
        }
        _ => anyhow::bail!("usage: aurelius-release keygen | export-key | sign <file> | verify <file>"),
    }
    Ok(())
}
