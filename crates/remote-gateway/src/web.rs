//! What a browser needs from the gateway. A browser's WebSocket cannot set `Authorization`
//! and page scripts should never hold a credential, so a browser authenticates with `HttpOnly`
//! cookies. Those only count together with this gateway's own `Origin`, so another site cannot
//! use them. Native clients keep their headers. The web client itself is served from a `web`
//! folder beside the daemon, from the same origin, so no CORS is involved.

use crate::Hub;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::Response;
use std::path::PathBuf;

/// `client_id:device_secret`; only `POST /v1/session` receives it.
const DEVICE_COOKIE: &str = "aurelius_device";
/// The session token, for every other route.
const SESSION_COOKIE: &str = "aurelius_session";
/// A year: the browser stays paired until it is revoked.
const DEVICE_MAX_AGE_SECS: i64 = 365 * 24 * 60 * 60;

pub(crate) const SECURITY_HEADERS: [(&str, &str); 3] = [
    ("content-security-policy", "default-src 'self'; img-src 'self' data: blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "no-referrer"),
];

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name && !value.is_empty()).then(|| value.to_owned())
    })
}

pub(crate) fn device_cookie(headers: &HeaderMap) -> Option<(aurelius_domain::ClientId, String)> {
    let value = cookie(headers, DEVICE_COOKIE)?;
    let (id, secret) = value.split_once(':')?;
    Some((aurelius_domain::ClientId::parse(id).ok()?, secret.to_owned()))
}

pub(crate) fn session_cookie(headers: &HeaderMap) -> Option<String> {
    cookie(headers, SESSION_COOKIE)
}

fn set_cookie(response: &mut Response, name: &str, value: &str, path: &str, max_age_secs: i64) {
    let line = format!("{name}={value}; Path={path}; Max-Age={max_age_secs}; HttpOnly; Secure; SameSite=Strict");
    let line = HeaderValue::from_str(&line).expect("ids and tokens are plain ASCII");
    response.headers_mut().append(header::SET_COOKIE, line);
}

pub(crate) fn set_device_cookie(response: &mut Response, client_id: &str, secret: &str) {
    set_cookie(response, DEVICE_COOKIE, &format!("{client_id}:{secret}"), "/v1/session", DEVICE_MAX_AGE_SECS);
}

pub(crate) fn set_session_cookie(response: &mut Response, token: &str, max_age_secs: i64) {
    set_cookie(response, SESSION_COOKIE, token, "/v1", max_age_secs);
}

/// This browser forgets its pairing and its session.
pub(crate) fn clear_cookies(response: &mut Response) {
    set_cookie(response, DEVICE_COOKIE, "", "/v1/session", 0);
    set_cookie(response, SESSION_COOKIE, "", "/v1", 0);
}

/// `https://host:443` and `https://host` are one origin; a browser never writes the default port.
fn normal_origin(origin: &str) -> String {
    let origin = origin.trim_end_matches('/').to_ascii_lowercase();
    origin.strip_suffix(":443").filter(|o| o.starts_with("https://")).map_or_else(|| origin.clone(), str::to_owned)
}

fn same_origin(origin: &str, allowed: &str) -> bool {
    normal_origin(origin) == normal_origin(allowed)
}

impl Hub {
    /// The request comes from this gateway's own pages. No `Origin` is no: a browser sends one on
    /// a WebSocket upgrade and on every request that changes something.
    pub(crate) async fn allows_origin(&self, headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return false };
        let status = self.status().await;
        if status.url.as_deref().is_some_and(|url| same_origin(origin, url)) {
            return true;
        }
        // In dev the pages are opened on this computer, without Tailscale.
        self.inner.environment == "dev"
            && [format!("http://127.0.0.1:{}", status.port), format!("http://localhost:{}", status.port)].iter().any(|local| same_origin(origin, local))
    }
}

/// The folder with the web client: `AURELIUS_WEB_DIR`, else `web` beside this program. None when
/// it has no `index.html`: a server without a web client answers only `/v1`.
pub(crate) fn site_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("AURELIUS_WEB_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_exe().ok()?.parent()?.join("web"),
    };
    dir.join("index.html").is_file().then_some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(line: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(line).unwrap());
        headers
    }

    #[test]
    fn a_cookie_is_found_by_its_exact_name() {
        let h = headers("other=1; aurelius_session=abc; aurelius_session2=no");
        assert_eq!(session_cookie(&h).as_deref(), Some("abc"));
        assert_eq!(session_cookie(&headers("xaurelius_session=abc")), None);
        assert_eq!(session_cookie(&headers("aurelius_session=")), None);
        assert_eq!(session_cookie(&HeaderMap::new()), None);
    }

    #[test]
    fn origins_compare_without_the_default_port() {
        assert!(same_origin("https://pc.tail.ts.net", "https://pc.tail.ts.net:443"));
        assert!(same_origin("https://PC.tail.ts.net:8443", "https://pc.tail.ts.net:8443"));
        assert!(!same_origin("https://pc.tail.ts.net", "https://pc.tail.ts.net:8443"));
        assert!(!same_origin("http://pc.tail.ts.net", "https://pc.tail.ts.net:443"));
        assert!(!same_origin("https://pc.tail.ts.net.evil.example", "https://pc.tail.ts.net"));
        assert!(!same_origin("null", "https://pc.tail.ts.net"));
    }
}
