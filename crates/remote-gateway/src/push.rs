//! Push notifications straight from this PC to Apple (APNs, token auth).
//! No hosted backend: the daemon signs an ES256 JWT with the user's own
//! APNs key and posts over HTTP/2.
//!
//! Content is discreet: a title and the task title at most, plus routing
//! data. Approvals are always judged in the app, with the full action.

use crate::PushIds;
use aurelius_protocol::NotificationView;
use aurelius_storage::remote::PushTarget;
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use std::sync::Mutex;

/// APNs wants a fresh token between 20 and 60 minutes.
const JWT_MAX_AGE_S: i64 = 40 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub title: String,
    pub body: String,
    /// Routing for the app (`target_type`, ids); no content.
    pub data: serde_json::Value,
    /// The iOS notification category: it carries the Approve / View actions ("plan" or "approval").
    pub category: Option<&'static str>,
}

impl Alert {
    pub fn for_notification(found: &aurelius_storage::timeline::FoundNotification, project_id: Option<aurelius_domain::ProjectId>) -> Self {
        let n: &NotificationView = &found.view;
        // Plans and approvals get action buttons; a question (needs an answer) only opens.
        let category = match (n.target_type.as_str(), n.title.as_str()) {
            ("approval.requested", aurelius_storage::timeline::PLAN_TITLE) => Some("plan"),
            ("approval.requested", "Approval needed") => Some("approval"),
            _ => None,
        };
        Alert {
            category,
            title: n.title.clone(),
            body: if n.subject.is_empty() { "Open Aurelius to see what needs you.".into() } else { n.subject.clone() },
            data: serde_json::json!({
                "target_type": n.target_type,
                "aggregate_type": found.aggregate_type,
                "aggregate_id": found.aggregate_id,
                "task_id": n.task_id,
                "project_id": project_id,
                "notification_id": n.id,
            }),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("Push is not set up on the PC (APNs key, Key ID, Team ID, Bundle ID).")]
    NotConfigured,
    #[error("Android notifications are not set up yet.")]
    Unsupported,
    #[error("The device token is no longer valid; the phone registers again when the app opens.")]
    Unregistered,
    #[error("Apple refused the notification: {0}")]
    Rejected(String),
    #[error("Could not reach Apple: {0}")]
    Network(String),
    #[error("{0}")]
    Key(String),
}

/// PEM (`-----BEGIN PRIVATE KEY-----`) → PKCS#8 DER, validated as a P-256 key.
pub fn parse_p8(pem: &str) -> Result<Vec<u8>, String> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).map(str::trim).collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|_| "This is not an APNs key file (.p8).".to_string())?;
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new())
        .map_err(|_| "This is not an APNs key file (.p8).".to_string())?;
    Ok(der)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// ES256 JWT for APNs: header {alg, kid}, claims {iss, iat}.
pub fn apns_jwt(key_pem: &str, key_id: &str, team_id: &str, issued_at_s: i64) -> Result<String, String> {
    let der = parse_p8(key_pem)?;
    let rng = SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &rng).map_err(|e| e.to_string())?;
    let header = b64(serde_json::json!({ "alg": "ES256", "kid": key_id }).to_string().as_bytes());
    let claims = b64(serde_json::json!({ "iss": team_id, "iat": issued_at_s }).to_string().as_bytes());
    let signing_input = format!("{header}.{claims}");
    let signature = pair.sign(&rng, signing_input.as_bytes()).map_err(|e| e.to_string())?;
    Ok(format!("{signing_input}.{}", b64(signature.as_ref())))
}

pub struct Pusher {
    http: reqwest::Client,
    jwt: Mutex<Option<(String, String, i64)>>,
    last_error: Mutex<Option<String>>,
}

impl Pusher {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            jwt: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }

    fn token(&self, key_pem: &str, key_id: &str, team_id: &str) -> Result<String, PushError> {
        let now = aurelius_storage::now_ms() / 1000;
        let mut cached = self.jwt.lock().unwrap();
        if let Some((kid, jwt, issued)) = cached.as_ref() {
            if kid == key_id && now - issued < JWT_MAX_AGE_S {
                return Ok(jwt.clone());
            }
        }
        let jwt = apns_jwt(key_pem, key_id, team_id, now).map_err(PushError::Key)?;
        *cached = Some((key_id.to_owned(), jwt.clone(), now));
        Ok(jwt)
    }

    pub async fn send(&self, ids: &PushIds, key_pem: Option<&str>, target: &PushTarget, alert: &Alert) -> Result<(), PushError> {
        let outcome = self.send_inner(ids, key_pem, target, alert).await;
        *self.last_error.lock().unwrap() = match &outcome {
            Ok(()) | Err(PushError::Unregistered) => None,
            Err(err) => Some(err.to_string()),
        };
        outcome
    }

    async fn send_inner(&self, ids: &PushIds, key_pem: Option<&str>, target: &PushTarget, alert: &Alert) -> Result<(), PushError> {
        if target.platform != "apns" {
            return Err(PushError::Unsupported);
        }
        let (Some(key_id), Some(team_id), Some(bundle_id), Some(key_pem)) = (&ids.key_id, &ids.team_id, &ids.bundle_id, key_pem) else {
            return Err(PushError::NotConfigured);
        };
        let jwt = self.token(key_pem, key_id, team_id)?;
        let host = if target.environment == "sandbox" { "api.sandbox.push.apple.com" } else { "api.push.apple.com" };
        // Stored text is redacted already; once more as it leaves (SEC0 E14).
        let (title, text) = (aurelius_runtime::redact::text(&alert.title), aurelius_runtime::redact::text(&alert.body));
        let mut body = serde_json::json!({
            "aps": {
                "alert": { "title": title, "body": text },
                "sound": "default",
                "thread-id": alert.data["target_type"],
            },
        });
        if let Some(category) = alert.category {
            body["aps"]["category"] = category.into();
        }
        body["aurelius"] = alert.data.clone();
        let response = self
            .http
            .post(format!("https://{host}/3/device/{}", target.token))
            .header("authorization", format!("bearer {jwt}"))
            .header("apns-topic", bundle_id)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&body)
            .send()
            .await
            .map_err(|e| PushError::Network(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let reason = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["reason"].as_str().map(str::to_owned))
            .unwrap_or_else(|| status.to_string());
        if status.as_u16() == 410 || matches!(reason.as_str(), "BadDeviceToken" | "Unregistered" | "DeviceTokenNotForTopic") {
            return Err(PushError::Unregistered);
        }
        if reason == "ExpiredProviderToken" {
            *self.jwt.lock().unwrap() = None;
        }
        Err(PushError::Rejected(reason))
    }
}

impl Default for Pusher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_FIXED, KeyPair, UnparsedPublicKey};

    fn test_key() -> (String, Vec<u8>) {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            base64::engine::general_purpose::STANDARD.encode(pkcs8.as_ref())
        );
        let public = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap().public_key().as_ref().to_vec();
        (pem, public)
    }

    #[test]
    fn jwt_is_a_valid_es256_signature() {
        let (pem, public) = test_key();
        let jwt = apns_jwt(&pem, "ABC123DEFG", "TEAM123456", 1_700_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value =
            serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!((header["alg"].as_str(), header["kid"].as_str()), (Some("ES256"), Some("ABC123DEFG")));
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        assert_eq!(signature.len(), 64, "JWS uses r||s, not DER");
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, public)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn garbage_is_not_a_key() {
        assert!(parse_p8("hello").is_err());
    }
}
