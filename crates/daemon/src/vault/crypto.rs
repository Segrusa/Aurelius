//! The vault's cryptography (Sentinel SEC0 E2, E3, E5), all from `ring`.
//!
//! * One master key of 32 random bytes, never used itself: HKDF-SHA256 gives
//!   the AEAD key (`enc`), the key's id (`key-id`, the first 16 bytes, in hex)
//!   and the HMAC key for request hashes (`idem`).
//! * ChaCha20-Poly1305 with a random 96-bit nonce per seal (`SystemRandom`),
//!   kept beside the ciphertext; never derived from the item or its version (a
//!   restored backup can bring version numbers back).
//! * The associated data binds a ciphertext to its key, item, version, kind,
//!   company and project, in fixed lengths with a format label: moved to
//!   another item, version or scope it does not open.
//! * A failed open is `Undecryptable`, never part of a plaintext.

pub use aurelius_storage::vault::{Sealed, VaultKind};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::hkdf;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

/// The format label in front of the associated data and the HKDF salt.
const LABEL: &[u8] = b"aurelius-vault-v1";
pub const KEY_LEN: usize = 32;
/// Bytes of the key id (shown as 32 hex characters).
const KEY_ID_LEN: usize = 16;
/// Label, key id, item, version, kind, company, project.
pub const AAD_LEN: usize = LABEL.len() + KEY_ID_LEN + 16 + 8 + 1 + 16 + 16;

/// A value that did not open: the key does not fit, or the row was changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undecryptable;

/// The master key. Never printed; wiped when dropped.
pub struct MasterKey(Zeroizing<[u8; KEY_LEN]>);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

impl MasterKey {
    /// 32 bytes from the system's random source.
    pub fn generate() -> Result<Self, ring::error::Unspecified> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        SystemRandom::new().fill(bytes.as_mut())?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        (bytes.len() == KEY_LEN).then(|| {
            key.copy_from_slice(bytes);
            Self(key)
        })
    }

    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

/// What a ciphertext is bound to (E2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub item_id: uuid::Uuid,
    pub version: u64,
    pub kind: VaultKind,
    pub company_id: uuid::Uuid,
    /// None: a company item (16 zero bytes).
    pub project_id: Option<uuid::Uuid>,
}

impl Binding {
    /// An item's current value (`version` the value's version).
    pub fn of_row(row: &aurelius_storage::vault::VaultItemRow) -> Option<Self> {
        Self::parse(&row.id, row.version, row.kind, &row.company_id, row.project_id.as_deref())
    }

    /// A stored value, current or previous, as it was sealed.
    pub fn of_value(value: &aurelius_storage::vault::StoredValue) -> Option<Self> {
        Self::parse(&value.item_id, value.version, value.kind, &value.company_id, value.project_id.as_deref())
    }

    pub fn parse(item: &str, version: i64, kind: VaultKind, company: &str, project: Option<&str>) -> Option<Self> {
        Some(Self {
            item_id: uuid::Uuid::parse_str(item).ok()?,
            version: u64::try_from(version).ok()?,
            kind,
            company_id: uuid::Uuid::parse_str(company).ok()?,
            project_id: match project {
                Some(p) => Some(uuid::Uuid::parse_str(p).ok()?),
                None => None,
            },
        })
    }
}

fn kind_byte(kind: VaultKind) -> u8 {
    match kind {
        VaultKind::Text => 1,
        VaultKind::Url => 2,
        VaultKind::Secret => 3,
    }
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// The keys derived from one master key.
pub struct Keys {
    enc: LessSafeKey,
    idem: hmac::Key,
    id: [u8; KEY_ID_LEN],
    rng: SystemRandom,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys").field("key_id", &self.key_id()).finish()
    }
}

impl Keys {
    pub fn derive(master: &MasterKey) -> Self {
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, LABEL).extract(master.bytes());
        let mut enc = Zeroizing::new([0u8; KEY_LEN]);
        prk.expand(&[b"enc"], Len(KEY_LEN)).and_then(|okm| okm.fill(enc.as_mut())).expect("HKDF gives 32 bytes");
        let mut id = [0u8; KEY_ID_LEN];
        prk.expand(&[b"key-id"], Len(KEY_ID_LEN)).and_then(|okm| okm.fill(&mut id)).expect("HKDF gives 16 bytes");
        let idem: hmac::Key = prk.expand(&[b"idem"], hmac::HMAC_SHA256).expect("HKDF gives an HMAC key").into();
        let enc = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, enc.as_ref()).expect("a 32-byte key"));
        Self { enc, idem, id, rng: SystemRandom::new() }
    }

    /// The key's id: 32 hex characters.
    pub fn key_id(&self) -> String {
        hex::encode(self.id)
    }

    fn aad(&self, binding: &Binding) -> [u8; AAD_LEN] {
        let mut aad = [0u8; AAD_LEN];
        let parts: [&[u8]; 7] = [
            LABEL,
            &self.id,
            binding.item_id.as_bytes(),
            &binding.version.to_be_bytes(),
            &[kind_byte(binding.kind)],
            binding.company_id.as_bytes(),
            binding.project_id.as_ref().map_or(&[0u8; 16], |p| p.as_bytes()),
        ];
        let mut at = 0;
        for part in parts {
            aad[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        debug_assert_eq!(at, AAD_LEN);
        aad
    }

    /// Encrypt `plaintext` for `binding` under a fresh random nonce.
    pub fn seal(&self, binding: &Binding, plaintext: &[u8]) -> Result<Sealed, ring::error::Unspecified> {
        let mut nonce = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce)?;
        let mut buffer = plaintext.to_vec();
        let sealed = self.enc.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad(binding)), &mut buffer);
        if let Err(e) = sealed {
            zeroize::Zeroize::zeroize(&mut buffer);
            return Err(e);
        }
        Ok(Sealed { ciphertext: buffer, nonce: nonce.to_vec(), key_id: self.key_id() })
    }

    /// The plaintext, or `Undecryptable` (another key, another binding, a
    /// changed row, not UTF-8). Never a part of it.
    pub fn open(&self, binding: &Binding, sealed: &Sealed) -> Result<Zeroizing<String>, Undecryptable> {
        if sealed.key_id != self.key_id() {
            return Err(Undecryptable);
        }
        let nonce: [u8; NONCE_LEN] = sealed.nonce.as_slice().try_into().map_err(|_| Undecryptable)?;
        let mut buffer = Zeroizing::new(sealed.ciphertext.clone());
        let plain = self.enc.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad(binding)), &mut buffer).map_err(|_| Undecryptable)?;
        let text = std::str::from_utf8(plain).map_err(|_| Undecryptable)?;
        Ok(Zeroizing::new(text.to_owned()))
    }

    /// HMAC-SHA256 of `value` under the `idem` key, in hex: what a request
    /// hash holds in place of a value (E5), never a plain hash of it.
    pub fn idem(&self, value: &[u8]) -> String {
        hex::encode(hmac::sign(&self.idem, value).as_ref())
    }
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
