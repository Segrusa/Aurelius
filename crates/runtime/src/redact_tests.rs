use super::*;

/// Every character that encodes differently in some form (E12, SEC0 canary).
const CANARY: &str = "sk-test-FAKE-0000 +/=&\"\\% é-canary-value";

fn redactor() -> Redactor {
    Redactor::new(&[Secret { name: "CANARY_TOKEN".into(), label: String::new(), value: Zeroizing::new(CANARY.into()) }])
}

fn b64(engine: &base64::engine::GeneralPurpose, bytes: &[u8]) -> String {
    engine.encode(bytes)
}

/// The forms of SEC0 E12, each inside a longer text.
fn printed() -> Vec<(&'static str, String)> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let mut out = vec![("plain", CANARY.to_owned())];
    for (label, engine) in [("b64", STANDARD), ("b64-nopad", STANDARD_NO_PAD), ("b64url", URL_SAFE), ("b64url-nopad", URL_SAFE_NO_PAD)] {
        for prefix in ["", "u", "us", "user:"] {
            out.push((label, b64(&engine, format!("{prefix}{CANARY}").as_bytes())));
            out.push((label, b64(&engine, format!("{prefix}{CANARY}:x").as_bytes())));
        }
    }
    let hex: String = CANARY.bytes().map(|b| format!("{b:02x}")).collect();
    out.push(("hex", hex.clone()));
    out.push(("HEX", hex.to_uppercase()));
    out.push(("percent", percent(CANARY.as_bytes(), true, false).to_string()));
    out.push(("percent-lower", percent(CANARY.as_bytes(), false, false).to_string()));
    out.push(("form", percent(CANARY.as_bytes(), true, true).to_string()));
    let json = serde_json::to_string(CANARY).unwrap();
    out.push(("json", json[1..json.len() - 1].to_owned()));
    out.push(("json-twice", serde_json::to_string(&json).unwrap()));
    out.push(("json-slash", json.replace('/', "\\/")));
    out.push(("json-ascii", json_escape(CANARY, false, true).to_string()));
    // SEC1 m1, written out by hand rather than with the redactor's own encoders.
    out.push(("python-quote", "sk-test-FAKE-0000%20%2B/%3D%26%22%5C%25%20%C3%A9-canary-value".to_owned()));
    out.push(("python-quote-lower", "sk-test-FAKE-0000%20%2b/%3d%26%22%5c%25%20%c3%a9-canary-value".to_owned()));
    out.push(("spaces-only", "sk-test-FAKE-0000%20+/=&\"\\%%20é-canary-value".to_owned()));
    out.push(("spaces-plus", "sk-test-FAKE-0000++/=&\"\\%+é-canary-value".to_owned()));
    out.push(("json-ascii-upper", "sk-test-FAKE-0000 +/=&\\\"\\\\% \\u00E9-canary-value".to_owned()));
    out.push(("dotnet", "sk-test-FAKE-0000 \\u002B/=\\u0026\\u0022\\\\% \\u00E9-canary-value".to_owned()));
    out
}

#[test]
fn every_form_of_a_value_inside_a_longer_text_is_redacted() {
    let r = redactor();
    for (label, form) in printed() {
        let text = format!("Authorization: Basic {form} and more");
        let clean = r.redact(&text);
        assert!(clean.contains("«secret:CANARY_TOKEN»"), "{label}: {clean}");
        for (_, other) in printed() {
            // What is left of the form is too short to give the value away.
            assert!(!clean.contains(&other) || other.len() < 8, "{label}: {clean}");
        }
        assert!(!clean.contains("FAKE-0000"), "{label}: {clean}");
    }
}

#[test]
fn a_text_without_a_value_is_borrowed_and_names_are_all_b10_shows() {
    let r = redactor();
    assert!(matches!(r.redact("nothing to see"), Cow::Borrowed(_)));
    assert_eq!(r.names_in(&format!("a {CANARY} b {CANARY}")), vec!["CANARY_TOKEN".to_owned()]);
    assert!(r.names_in("sk-test").is_empty());
    assert!(!format!("{r:?}").contains("FAKE"));
}

#[test]
fn a_short_value_is_never_a_pattern() {
    let r = Redactor::new(&[Secret { name: "SHORT".into(), label: String::new(), value: Zeroizing::new("abc1234".into()) }]);
    assert!(r.is_empty());
}

#[test]
fn json_strings_and_keys_are_redacted() {
    let r = redactor();
    let mut value = serde_json::json!({ "command": format!("curl -H 'X: {CANARY}'"), CANARY: [1, CANARY] });
    r.redact_json(&mut value);
    let text = value.to_string();
    assert!(!text.contains("FAKE-0000"), "{text}");
    assert_eq!(text.matches("«secret:CANARY_TOKEN»").count(), 3);
}

#[test]
fn a_value_split_over_any_two_deltas_is_redacted() {
    let r = redactor();
    for form in [CANARY.to_owned(), printed()[3].1.clone()] {
        let text = format!("before {form} after");
        for split in (0..=text.len()).filter(|i| text.is_char_boundary(*i)) {
            let mut stream = Stream::default();
            let mut out = stream.push(&r, &text[..split]);
            out.push_str(&stream.push(&r, &text[split..]));
            out.push_str(&stream.flush(&r));
            assert!(!out.contains("FAKE-0000") && out.contains("«secret:CANARY_TOKEN»"), "split {split}: {out}");
            assert!(out.starts_with("before ") && out.ends_with(" after"), "{out}");
        }
    }
    // Byte by byte, too.
    let mut stream = Stream::default();
    let mut out = String::new();
    for c in format!("x{CANARY}y").chars() {
        out.push_str(&stream.push(&r, &c.to_string()));
    }
    out.push_str(&stream.flush(&r));
    assert_eq!(out, "x«secret:CANARY_TOKEN»y");
}

#[test]
fn without_values_a_stream_holds_nothing_back() {
    let r = Redactor::default();
    let mut stream = Stream::default();
    assert_eq!(stream.push(&r, "hello"), "hello");
    assert!(stream.is_empty());
}

#[test]
fn the_global_automaton_is_swapped_at_once() {
    let _global = TEST_GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(text(CANARY), CANARY);
    install(redactor());
    assert_eq!(text(&format!("a {CANARY}")), "a «secret:CANARY_TOKEN»");
    let mut value = serde_json::json!([CANARY]);
    json(&mut value);
    assert_eq!(value, serde_json::json!(["«secret:CANARY_TOKEN»"]));
    install(Redactor::default());
    assert_eq!(string(CANARY.into()), CANARY);
}

#[test]
fn a_log_writer_redacts_every_record() {
    use std::io::Write;
    let _global = TEST_GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    install(redactor());
    let mut out = Writer(Vec::new());
    write!(out, "error: request failed with {CANARY}").unwrap();
    install(Redactor::default());
    let text = String::from_utf8(out.0).unwrap();
    assert_eq!(text, "error: request failed with «secret:CANARY_TOKEN»");
}

/// B10 names the item a board member knows (its label); the marker keeps
/// the env name (what a run knows it by).
#[test]
fn b10_names_the_item_and_the_marker_its_env_name() {
    let r = Redactor::new(&[Secret { name: "GITHUB_TOKEN".into(), label: "GitHub token".into(), value: Zeroizing::new(CANARY.into()) }]);
    assert_eq!(r.names_in(&format!("x {CANARY}")), vec!["GitHub token".to_owned()]);
    assert_eq!(r.redact(&format!("x {CANARY}")), "x «secret:GITHUB_TOKEN»");
}
