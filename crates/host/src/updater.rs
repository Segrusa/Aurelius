//! Signed releases. Every release zip is signed with an Ed25519 key that only
//! exists as a GitHub secret (and a backup in the OS credential store); the
//! public key below is compiled into the app. An update is installed only if
//! its signature verifies against this key.

use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::path::Path;

/// GitHub repository that publishes releases.
pub const REPO: &str = "Segrusa/Aurelius";
/// Release asset names.
pub const ZIP_PREFIX: &str = "aurelius-";
pub const ZIP_SUFFIX: &str = "-windows-x64.zip";

/// Public key of the release signing key (base64, 32 bytes).
pub const PUBLIC_KEY: &str = "SrqZ3sOnSvckbpmNDgzMXAHmz+9p22nvzWxWR+JsBlU=";

/// Every program of an installation (both products).
pub const BINARIES: [&str; 4] = ["aurelius.exe", "aurelius-daemon.exe", "aurelius-ctl.exe", "aurelius-bridge.exe"];

/// Written next to `aurelius.exe`: the installed desktop app's version, so the
/// service (which has its own version) knows whether the app is behind.
pub const DESKTOP_VERSION_FILE: &str = "aurelius.version";

/// The desktop app and the background server are released separately, each
/// with its own version, tag (`server-v1.2.3`, `desktop-v1.2.3`) and zip.
/// They work together as long as their protocol major matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    Server,
    Desktop,
}

impl Product {
    pub const ALL: [Product; 2] = [Product::Server, Product::Desktop];

    pub fn name(self) -> &'static str {
        match self {
            Product::Server => "server",
            Product::Desktop => "desktop",
        }
    }

    pub fn tag_prefix(self) -> &'static str {
        match self {
            Product::Server => "server-v",
            Product::Desktop => "desktop-v",
        }
    }

    pub fn tag(self, version: &str) -> String {
        format!("{}{version}", self.tag_prefix())
    }

    /// "server-v1.2.3" → "1.2.3" for the server; None for any other tag.
    pub fn version_of<'a>(self, tag: &'a str) -> Option<&'a str> {
        tag.trim().strip_prefix(self.tag_prefix()).filter(|v| parse_version(v).is_some())
    }

    /// The highest release of this product among `tags`.
    pub fn latest<'a>(self, tags: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
        tags.into_iter().filter_map(|t| self.version_of(t)).max_by_key(|v| parse_version(v))
    }

    pub fn zip_name(self, version: &str) -> String {
        format!("{ZIP_PREFIX}{}-{version}{ZIP_SUFFIX}", self.name())
    }

    /// The programs this product's zip contains (and an update replaces).
    pub fn binaries(self) -> &'static [&'static str] {
        match self {
            Product::Server => &["aurelius-daemon.exe", "aurelius-ctl.exe", "aurelius-bridge.exe"],
            Product::Desktop => &["aurelius.exe"],
        }
    }
}

/// New signing key: (secret base64, public base64).
pub fn generate_key() -> anyhow::Result<(String, String)> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    let key = SigningKey::from_bytes(&seed);
    Ok((B64.encode(key.to_bytes()), B64.encode(key.verifying_key().to_bytes())))
}

pub fn sign(bytes: &[u8], secret_b64: &str) -> anyhow::Result<String> {
    let secret: [u8; 32] = B64.decode(secret_b64.trim())?.try_into().map_err(|_| anyhow::anyhow!("signing key must be 32 bytes"))?;
    Ok(B64.encode(SigningKey::from_bytes(&secret).sign(bytes).to_bytes()))
}

pub fn verify_with(bytes: &[u8], signature_b64: &str, public_b64: &str) -> anyhow::Result<()> {
    let public: [u8; 32] = B64.decode(public_b64.trim())?.try_into().map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
    let key = VerifyingKey::from_bytes(&public).context("invalid public key")?;
    let sig: [u8; 64] = B64.decode(signature_b64.trim())?.try_into().map_err(|_| anyhow::anyhow!("signature must be 64 bytes"))?;
    key.verify(bytes, &Signature::from_bytes(&sig)).map_err(|_| anyhow::anyhow!("the signature does not match: this package was not made by the Aurelius release pipeline"))
}

/// Verify against the compiled-in release key.
pub fn verify(bytes: &[u8], signature_b64: &str) -> anyhow::Result<()> {
    if PUBLIC_KEY.starts_with("REPLACE") {
        bail!("this build has no release key; updates are disabled");
    }
    verify_with(bytes, signature_b64, PUBLIC_KEY)
}

/// "v1.2.3" / "1.2.3" → (1, 2, 3).
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.').map(|p| p.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("").parse::<u64>());
    Some((parts.next()?.ok()?, parts.next()?.ok()?, parts.next().unwrap_or(Ok(0)).ok()?))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// Extract only the product's binaries from a zip into `dest` (no paths from the archive are trusted).
pub fn extract(zip_path: &Path, dest: &Path, product: Product) -> anyhow::Result<()> {
    extract_names(zip_path, dest, product.binaries())
}

fn extract_names(zip_path: &Path, dest: &Path, binaries: &[&str]) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut found = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = Path::new(entry.name()).file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
        if !binaries.contains(&name.as_str()) {
            continue;
        }
        let mut out = std::fs::File::create(dest.join(&name))?;
        std::io::copy(&mut entry, &mut out)?;
        found += 1;
    }
    if found != binaries.len() {
        bail!("the update package is incomplete ({found} of {} programs)", binaries.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let (secret, public) = generate_key().unwrap();
        let sig = sign(b"release", &secret).unwrap();
        verify_with(b"release", &sig, &public).unwrap();
        assert!(verify_with(b"tampered", &sig, &public).is_err());
        let (_, other) = generate_key().unwrap();
        assert!(verify_with(b"release", &sig, &other).is_err());
    }

    #[test]
    fn versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
    }

    #[test]
    fn products_have_their_own_tags_and_zips() {
        assert_eq!(Product::Server.tag("0.6.1"), "server-v0.6.1");
        assert_eq!(Product::Desktop.zip_name("0.7.0"), "aurelius-desktop-0.7.0-windows-x64.zip");
        assert_eq!(Product::Server.version_of("server-v0.6.1"), Some("0.6.1"));
        assert_eq!(Product::Server.version_of("desktop-v0.6.1"), None);
        assert_eq!(Product::Desktop.version_of("v0.6.0"), None);
        let tags = ["v0.6.0", "server-v0.6.2", "desktop-v0.9.0", "server-v0.10.0", "server-vx"];
        assert_eq!(Product::Server.latest(tags), Some("0.10.0"));
        assert_eq!(Product::Desktop.latest(tags), Some("0.9.0"));
    }

    fn zip_of(dir: &Path, names: &[&str]) -> std::path::PathBuf {
        let zip_path = dir.join("r.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let opts: zip::write::SimpleFileOptions = Default::default();
        for name in names {
            w.start_file(format!("pkg/{name}"), opts).unwrap();
            std::io::Write::write_all(&mut w, name.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        zip_path
    }

    #[test]
    fn extracts_only_the_binaries() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = zip_of(dir.path(), &["aurelius.exe", "aurelius-daemon.exe", "aurelius-ctl.exe", "aurelius-bridge.exe", "../evil.exe"]);
        let out = dir.path().join("out");
        extract(&zip_path, &out, Product::Server).unwrap();
        assert!(out.join("aurelius-daemon.exe").exists() && out.join("aurelius-ctl.exe").exists() && out.join("aurelius-bridge.exe").exists());
        assert!(!out.join("aurelius.exe").exists(), "the server zip never installs the app");
        assert!(!dir.path().join("evil.exe").exists() && !out.join("evil.exe").exists());
    }

    #[test]
    fn a_product_zip_must_be_complete() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = zip_of(dir.path(), &["aurelius.exe"]);
        extract(&zip_path, &dir.path().join("app"), Product::Desktop).unwrap();
        assert!(extract(&zip_path, &dir.path().join("server"), Product::Server).is_err());
    }
}
