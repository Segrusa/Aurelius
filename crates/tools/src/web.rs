//! Safe web fetch: HTTPS only, DNS validated before connecting,
//! loopback/private/link-local/metadata addresses blocked, every redirect
//! re-checked, response size and time bounded, no scripts executed.

use crate::{html, sha256_hex};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REDIRECTS: usize = 5;
pub const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FetchError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("only https:// is allowed")]
    NotHttps,
    #[error("address not allowed (local or private network): {0}")]
    Forbidden(String),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("response too large (more than {} MB)", MAX_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("content type not supported: {0}")]
    Unsupported(String),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("network error: {0}")]
    Network(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceDocument {
    pub requested_url: String,
    pub final_url: String,
    pub title: Option<String>,
    pub text: String,
    pub sha256: String,
    pub fetched_at_ms: i64,
}

/// Addresses that must never be reached from a fetch.
pub fn forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return forbidden_v4(v4);
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
                || v6 == Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254) // AWS IMDS v6
        }
    }
}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // includes 169.254.169.254 cloud metadata
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (64..=127).contains(&o[1])) // carrier-grade NAT
        || (o[0] == 198 && (18..=19).contains(&o[1])) // benchmarking
        || o[0] >= 240
}

pub fn validate(url: &str) -> Result<reqwest::Url, FetchError> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| FetchError::InvalidUrl(e.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(FetchError::NotHttps);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(FetchError::InvalidUrl("no credentials in URLs".into()));
    }
    if parsed.host_str().is_none() {
        return Err(FetchError::InvalidUrl("no host".into()));
    }
    Ok(parsed)
}

/// Resolve and check every address; returns one allowed address to pin.
async fn resolve_allowed(url: &reqwest::Url) -> Result<SocketAddr, FetchError> {
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']).to_owned();
    let port = url.port_or_known_default().unwrap_or(443);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if forbidden_ip(ip) { Err(FetchError::Forbidden(ip.to_string())) } else { Ok(SocketAddr::new(ip, port)) };
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.map_err(|e| FetchError::Network(e.to_string()))?.collect();
    if addrs.is_empty() {
        return Err(FetchError::Network("host not found".into()));
    }
    // All answers must be public: a mix is a rebinding signal.
    if let Some(bad) = addrs.iter().find(|a| forbidden_ip(a.ip())) {
        return Err(FetchError::Forbidden(bad.ip().to_string()));
    }
    Ok(addrs[0])
}

pub async fn fetch(url: &str) -> Result<SourceDocument, FetchError> {
    let requested = validate(url)?;
    let mut current = requested.clone();
    for _ in 0..=MAX_REDIRECTS {
        let addr = resolve_allowed(&current).await?;
        let host = current.host_str().unwrap_or_default().to_owned();
        // Pin the validated address so DNS cannot change between check and connect.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&host, addr)
            .timeout(TIMEOUT)
            .user_agent(concat!("Aurelius/", env!("CARGO_PKG_VERSION"), " (+local research)"))
            .build()
            .map_err(|e| FetchError::Network(e.to_string()))?;
        let mut response = client.get(current.clone()).send().await.map_err(|e| FetchError::Network(e.to_string()))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response.headers().get("location").and_then(|v| v.to_str().ok()).ok_or(FetchError::Status(status.as_u16()))?;
            let next = current.join(location).map_err(|e| FetchError::InvalidUrl(e.to_string()))?;
            current = validate(next.as_str())?;
            continue;
        }
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        if let Some(len) = response.content_length() {
            if len as usize > MAX_BYTES {
                return Err(FetchError::TooLarge);
            }
        }
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| FetchError::Network(e.to_string()))? {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_BYTES {
                return Err(FetchError::TooLarge);
            }
        }
        let raw = String::from_utf8_lossy(&body).into_owned();
        let (title, text) = if content_type.contains("html") || content_type.is_empty() && raw.trim_start().starts_with('<') {
            (html::title(&raw), html::to_text(&raw))
        } else if content_type.starts_with("text/") || content_type.contains("json") {
            (None, raw.clone())
        } else {
            return Err(FetchError::Unsupported(content_type));
        };
        return Ok(SourceDocument {
            requested_url: requested.to_string(),
            final_url: current.to_string(),
            title,
            sha256: sha256_hex(&body),
            text,
            fetched_at_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
        });
    }
    Err(FetchError::TooManyRedirects)
}

/// URLs in free text (for "fetch the links I mention").
pub fn urls_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split_whitespace()
        .filter_map(|w| w.find("https://").or_else(|| w.find("http://")).map(|i| &w[i..]))
        .map(|w| w.trim_end_matches(['.', ',', ')', ']', '>', '"', '\'', ';', '!', '?']).to_owned())
        .collect();
    out.dedup();
    out.truncate(5);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_metadata_addresses_are_forbidden() {
        for ip in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0", "::1", "fe80::1", "fd00::1", "::ffff:127.0.0.1"] {
            assert!(forbidden_ip(ip.parse().unwrap()), "{ip} must be forbidden");
        }
        for ip in ["1.1.1.1", "2606:4700:4700::1111"] {
            assert!(!forbidden_ip(ip.parse().unwrap()), "{ip} is public");
        }
    }

    #[test]
    fn only_https_without_credentials() {
        assert_eq!(validate("http://example.com"), Err(FetchError::NotHttps));
        assert!(validate("https://user:pw@example.com").is_err());
        assert!(validate("file:///etc/passwd").is_err());
        assert!(validate("https://example.com/a").is_ok());
    }

    #[tokio::test]
    async fn literal_private_hosts_are_refused_before_connecting() {
        assert!(matches!(fetch("https://127.0.0.1/").await, Err(FetchError::Forbidden(_))));
        assert!(matches!(fetch("https://[::1]/").await, Err(FetchError::Forbidden(_))));
        assert!(matches!(fetch("https://169.254.169.254/latest/meta-data").await, Err(FetchError::Forbidden(_))));
    }

    #[test]
    fn urls_are_found_in_text() {
        assert_eq!(urls_in("Kijk op https://a.nl/x. en (https://b.org)"), ["https://a.nl/x", "https://b.org"]);
    }
}
