use super::*;
use sha2::Digest;

const FAKE: &str = "sk-test-FAKE-0000-crypto";

fn binding() -> Binding {
    Binding { item_id: uuid::Uuid::now_v7(), version: 1, kind: VaultKind::Secret, company_id: uuid::Uuid::now_v7(), project_id: None }
}

fn keys() -> Keys {
    Keys::derive(&MasterKey::generate().unwrap())
}

#[test]
fn a_value_opens_only_with_its_own_key_and_binding() {
    let keys = keys();
    let b = binding();
    let sealed = keys.seal(&b, FAKE.as_bytes()).unwrap();
    assert_eq!(keys.open(&b, &sealed).unwrap().as_str(), FAKE);
    assert!(!sealed.ciphertext.windows(8).any(|w| FAKE.as_bytes().windows(8).any(|f| f == w)));
    assert!(!format!("{sealed:?} {keys:?}").contains("FAKE"));

    // E3: moved to another item, version, kind, company or project: it does not open.
    let project = uuid::Uuid::now_v7();
    let others = [
        Binding { item_id: uuid::Uuid::now_v7(), ..b },
        Binding { version: 2, ..b },
        Binding { kind: VaultKind::Text, ..b },
        Binding { company_id: uuid::Uuid::now_v7(), ..b },
        Binding { project_id: Some(project), ..b },
    ];
    for other in others {
        assert_eq!(keys.open(&other, &sealed), Err(Undecryptable), "{other:?}");
    }
    // A project item does not open as the company's.
    let in_project = Binding { project_id: Some(project), ..b };
    let sealed_p = keys.seal(&in_project, FAKE.as_bytes()).unwrap();
    assert_eq!(keys.open(&b, &sealed_p), Err(Undecryptable));

    // Another master key, a changed byte, another nonce: never a plaintext.
    assert_eq!(self::keys().open(&b, &sealed), Err(Undecryptable));
    let mut changed = sealed.clone();
    changed.ciphertext[0] ^= 1;
    assert_eq!(keys.open(&b, &changed), Err(Undecryptable));
    let mut short_nonce = sealed.clone();
    short_nonce.nonce.pop();
    assert_eq!(keys.open(&b, &short_nonce), Err(Undecryptable));
}

#[test]
fn swapping_the_ciphertext_or_nonce_of_two_items_fails_both() {
    let keys = keys();
    let (a, b) = (binding(), binding());
    let (sa, sb) = (keys.seal(&a, b"first-FAKE-value").unwrap(), keys.seal(&b, b"second-FAKE-value").unwrap());
    assert_eq!(keys.open(&a, &sb), Err(Undecryptable));
    assert_eq!(keys.open(&b, &sa), Err(Undecryptable));
    let swapped_nonce = Sealed { nonce: sb.nonce.clone(), ..sa.clone() };
    assert_eq!(keys.open(&a, &swapped_nonce), Err(Undecryptable));
}

#[test]
fn every_seal_takes_a_fresh_nonce() {
    let keys = keys();
    let b = binding();
    let (one, two) = (keys.seal(&b, FAKE.as_bytes()).unwrap(), keys.seal(&b, FAKE.as_bytes()).unwrap());
    assert_eq!(one.nonce.len(), 12);
    assert_ne!(one.nonce, two.nonce);
    assert_ne!(one.ciphertext, two.ciphertext);
}

#[test]
fn the_associated_data_has_fixed_lengths_and_a_label() {
    let keys = keys();
    let b = binding();
    let aad = keys.aad(&b);
    assert_eq!(AAD_LEN, 17 + 16 + 16 + 8 + 1 + 16 + 16);
    assert!(aad.starts_with(b"aurelius-vault-v1"));
    assert_eq!(&aad[17..33], &hex::decode(keys.key_id()).unwrap()[..]);
    assert_eq!(&aad[33..49], b.item_id.as_bytes());
    assert_eq!(&aad[49..57], &1u64.to_be_bytes());
    assert_eq!(aad[57], 3);
    assert_eq!(&aad[74..90], &[0u8; 16], "a company item: zeros for the project");
}

#[test]
fn derived_keys_are_apart_and_fixed_per_master_key() {
    let master = MasterKey::generate().unwrap();
    let (one, two) = (Keys::derive(&master), Keys::derive(&MasterKey::from_bytes(master.bytes()).unwrap()));
    assert_eq!(one.key_id(), two.key_id());
    assert_eq!(one.key_id().len(), 32);
    assert_ne!(one.key_id(), hex::encode(&master.bytes()[..16]), "the master key is never used itself");
    // The request hash part: keyed, fixed, never a plain hash of the value.
    assert_eq!(one.idem(FAKE.as_bytes()), two.idem(FAKE.as_bytes()));
    assert_ne!(one.idem(FAKE.as_bytes()), hex::encode(sha2::Sha256::digest(FAKE.as_bytes())));
    assert_ne!(one.idem(FAKE.as_bytes()), keys().idem(FAKE.as_bytes()));
    assert!(MasterKey::from_bytes(&[0u8; 31]).is_none());
    assert_eq!(format!("{master:?}"), "MasterKey(<redacted>)");
}
