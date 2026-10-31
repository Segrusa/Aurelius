//! The vault's redactor (Sentinel SEC0 E12, E14, E16): one automaton over every
//! secret value the server knows (the current version of every item and the
//! versions before it), in every form a run or a tool is likely to print it.
//! A match becomes `«secret:NAME»`.
//!
//! It is global, not per run: memory extraction, the Team chat, webhooks and
//! push do not know their run. The daemon installs a new automaton
//! ([`install`]) before a new value can reach any run; every reader takes the
//! one that is current ([`current`]) without a lock.
//!
//! Where it applies (E14, the choke points): every runtime event as it leaves
//! the adapter ([`RunHandle::new`](crate::RunHandle::new), with a tail buffer
//! for the text stream), the entry of every bridge request and the daemon's
//! log writer (daemon), and B10 on the API side. What a run prints in pieces or
//! rewrites itself cannot be redacted (R5).

use aho_corasick::{AhoCorasick, MatchKind};
use arc_swap::ArcSwap;
use base64::Engine;
use std::borrow::Cow;
use std::sync::{Arc, LazyLock};
use zeroize::Zeroizing;

/// Shorter values are refused by the vault (E13): they would match everywhere.
pub const MIN_CHARS: usize = 8;

/// One value to redact, under the name it shows as (its marker), and the
/// name of its item a board member knows it by (B10's `matched_items`; empty:
/// the marker's name).
pub struct Secret {
    pub name: String,
    pub label: String,
    pub value: Zeroizing<String>,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret").field("name", &self.name).field("label", &self.label).field("value", &"<redacted>").finish()
    }
}

/// The automaton. Built once per change of the vault, then only read.
#[derive(Default)]
pub struct Redactor {
    automaton: Option<AhoCorasick>,
    /// Per pattern: the index of its name in `names`, and of its label in `labels`.
    owner: Vec<usize>,
    label_of: Vec<usize>,
    names: Vec<String>,
    labels: Vec<String>,
    longest: usize,
}

impl std::fmt::Debug for Redactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redactor").field("patterns", &self.owner.len()).field("names", &self.names).finish()
    }
}

/// The marker a value is replaced with.
pub fn marker(name: &str) -> String {
    format!("«secret:{name}»")
}

impl Redactor {
    pub fn new(secrets: &[Secret]) -> Self {
        Self::of(secrets.iter())
    }

    pub fn of<'a>(secrets: impl IntoIterator<Item = &'a Secret>) -> Self {
        let mut patterns: Vec<Zeroizing<Vec<u8>>> = Vec::new();
        let mut owner = Vec::new();
        let mut label_of = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        let index = |list: &mut Vec<String>, name: &str| match list.iter().position(|n| n == name) {
            Some(i) => i,
            None => {
                list.push(name.to_owned());
                list.len() - 1
            }
        };
        for secret in secrets {
            if secret.value.chars().count() < MIN_CHARS {
                continue;
            }
            let name = index(&mut names, &secret.name);
            let label = index(&mut labels, if secret.label.is_empty() { &secret.name } else { &secret.label });
            for form in forms(&secret.value) {
                if form.len() >= 4 && !patterns.iter().any(|p| **p == *form) {
                    patterns.push(form);
                    owner.push(name);
                    label_of.push(label);
                }
            }
        }
        if patterns.is_empty() {
            return Self::default();
        }
        let longest = patterns.iter().map(|p| p.len()).max().unwrap_or(0);
        let automaton = AhoCorasick::builder().match_kind(MatchKind::LeftmostLongest).build(patterns.iter().map(|p| p.as_slice())).ok();
        Self { automaton, owner, label_of, names, labels, longest }
    }

    pub fn is_empty(&self) -> bool {
        self.automaton.is_none()
    }

    /// How many bytes a stream holds back: the longest form less one.
    pub fn hold(&self) -> usize {
        self.longest.saturating_sub(1)
    }

    /// `text` with every form of every value replaced by its marker.
    pub fn redact<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let Some(automaton) = &self.automaton else { return Cow::Borrowed(text) };
        let mut out = String::new();
        let mut last = 0;
        for found in automaton.find_iter(text.as_bytes()) {
            out.push_str(&text[last..found.start()]);
            out.push_str(&marker(&self.names[self.owner[found.pattern().as_usize()]]));
            last = found.end();
        }
        if last == 0 && out.is_empty() {
            return Cow::Borrowed(text);
        }
        out.push_str(&text[last..]);
        Cow::Owned(out)
    }

    /// The items of the values in `text`, by their labels (B10: names only,
    /// never where).
    pub fn names_in(&self, text: &str) -> Vec<String> {
        let Some(automaton) = &self.automaton else { return Vec::new() };
        let mut found: Vec<String> = Vec::new();
        for m in automaton.find_iter(text.as_bytes()) {
            let name = &self.labels[self.label_of[m.pattern().as_usize()]];
            if !found.contains(name) {
                found.push(name.clone());
            }
        }
        found
    }

    /// Every string in `value` (keys too) redacted, in place.
    pub fn redact_json(&self, value: &mut serde_json::Value) {
        if self.is_empty() {
            return;
        }
        match value {
            serde_json::Value::String(s) => {
                if let Cow::Owned(clean) = self.redact(s) {
                    *s = clean;
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| self.redact_json(v)),
            serde_json::Value::Object(map) => {
                let entries = std::mem::take(map);
                for (key, mut v) in entries {
                    self.redact_json(&mut v);
                    map.insert(self.redact(&key).into_owned(), v);
                }
            }
            _ => {}
        }
    }
}

/// The forms of a value (E12): plain; base64 standard and url-safe, the part
/// that stays the same at each of the three byte offsets it can have in a
/// longer text (so with and without padding); hex in small and capital
/// letters; percent-encoding (both cases), with `/` left as it is (Python's
/// `quote`), form-encoding, and only the spaces encoded (`%20` or `+`); JSON
/// escaping (`\"`, `\\`, `\/`, `\uXXXX` in small and capital letters, and .NET's
/// escaping of `"&'+<>` too) once and twice. Other partial encodings are R5.
fn forms(value: &str) -> Vec<Zeroizing<Vec<u8>>> {
    let bytes = value.as_bytes();
    let mut out: Vec<Zeroizing<Vec<u8>>> = vec![Zeroizing::new(bytes.to_vec())];
    for engine in [base64::engine::general_purpose::STANDARD_NO_PAD, base64::engine::general_purpose::URL_SAFE_NO_PAD] {
        for offset in 0..3usize {
            let mut input = Zeroizing::new(vec![0u8; offset]);
            input.extend_from_slice(bytes);
            let encoded = Zeroizing::new(engine.encode(input.as_slice()));
            // The characters the offset's bytes before it or the bytes after it touch.
            let start = [0, 2, 3][offset];
            let end = encoded.len() - usize::from(input.len() % 3 != 0);
            if end > start {
                out.push(Zeroizing::new(encoded.as_bytes()[start..end].to_vec()));
            }
        }
    }
    let hex_lower: Zeroizing<String> = Zeroizing::new(bytes.iter().map(|b| format!("{b:02x}")).collect());
    out.push(Zeroizing::new(hex_lower.to_ascii_uppercase().into_bytes()));
    out.push(Zeroizing::new(hex_lower.as_bytes().to_vec()));
    for upper in [true, false] {
        out.push(Zeroizing::new(percent(bytes, upper, false).as_bytes().to_vec()));
        out.push(Zeroizing::new(percent(bytes, upper, true).as_bytes().to_vec()));
        out.push(Zeroizing::new(percent_keeping(bytes, upper, b"/").as_bytes().to_vec()));
    }
    for space in ["%20", "+"] {
        out.push(Zeroizing::new(value.replace(' ', space).into_bytes()));
    }
    let once = json_escape(value, false, false);
    for (slash, ascii) in [(false, false), (true, false), (false, true), (true, true)] {
        let escaped = json_escape(value, slash, ascii);
        out.push(Zeroizing::new(escaped.as_bytes().to_vec()));
        out.push(Zeroizing::new(json_escape_with(value, slash, ascii, Hex::Upper).as_bytes().to_vec()));
    }
    out.push(Zeroizing::new(json_escape_with(value, false, true, Hex::DotNet).as_bytes().to_vec()));
    out.push(Zeroizing::new(json_escape(&once, false, false).as_bytes().to_vec()));
    out.push(Zeroizing::new(json_escape(&once, true, false).as_bytes().to_vec()));
    out
}

fn percent(bytes: &[u8], upper: bool, form: bool) -> Zeroizing<String> {
    percent_with(bytes, upper, form, b"")
}

/// Percent-encoding that leaves the bytes of `safe` as they are.
fn percent_keeping(bytes: &[u8], upper: bool, safe: &[u8]) -> Zeroizing<String> {
    percent_with(bytes, upper, false, safe)
}

fn percent_with(bytes: &[u8], upper: bool, form: bool, safe: &[u8]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    for &b in bytes {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ if safe.contains(&b) => out.push(b as char),
            b' ' if form => out.push('+'),
            _ if upper => out.push_str(&format!("%{b:02X}")),
            _ => out.push_str(&format!("%{b:02x}")),
        }
    }
    out
}

/// How `\uXXXX` is written: small letters (serde, JavaScript), capitals
/// (.NET, Java), or capitals with .NET's default escaping of `"&'+<>` and `` ` ``.
#[derive(Clone, Copy, PartialEq)]
enum Hex {
    Lower,
    Upper,
    DotNet,
}

fn json_escape(value: &str, slash: bool, ascii: bool) -> Zeroizing<String> {
    json_escape_with(value, slash, ascii, Hex::Lower)
}

fn json_escape_with(value: &str, slash: bool, ascii: bool, hex: Hex) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    let unit = |out: &mut String, unit: u32| match hex {
        Hex::Lower => out.push_str(&format!("\\u{unit:04x}")),
        Hex::Upper | Hex::DotNet => out.push_str(&format!("\\u{unit:04X}")),
    };
    for c in value.chars() {
        match c {
            '"' | '&' | '\'' | '+' | '<' | '>' | '`' if hex == Hex::DotNet => unit(&mut out, c as u32),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' if slash => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => unit(&mut out, c as u32),
            c if ascii && !c.is_ascii() => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    unit(&mut out, u32::from(*u));
                }
            }
            c => out.push(c),
        }
    }
    out
}

static GLOBAL: LazyLock<ArcSwap<Redactor>> = LazyLock::new(|| ArcSwap::from_pointee(Redactor::default()));

/// Make `redactor` the one every reader uses from now on.
pub fn install(redactor: Redactor) {
    GLOBAL.store(Arc::new(redactor));
}

/// The values of every vault in this process, by owner (a daemon's data
/// folder): one process normally has one; the tests run several daemons.
static SETS: LazyLock<std::sync::Mutex<std::collections::BTreeMap<String, Vec<Secret>>>> = LazyLock::new(Default::default);

fn rebuild(sets: &std::collections::BTreeMap<String, Vec<Secret>>) {
    install(Redactor::of(sets.values().flatten()));
}

/// Make `secrets` the values of `owner`, and the automaton of them all
/// current, before this returns.
pub fn install_set(owner: &str, secrets: Vec<Secret>) {
    let mut sets = SETS.lock().unwrap_or_else(|e| e.into_inner());
    sets.insert(owner.to_owned(), secrets);
    rebuild(&sets);
}

/// Add one value to `owner`'s (a new or replaced secret, before it can be
/// delivered, E12).
pub fn add_to_set(owner: &str, secret: Secret) {
    let mut sets = SETS.lock().unwrap_or_else(|e| e.into_inner());
    sets.entry(owner.to_owned()).or_default().push(secret);
    rebuild(&sets);
}

/// The automaton that is current.
pub fn current() -> Arc<Redactor> {
    GLOBAL.load_full()
}

/// `text` redacted with the current automaton.
pub fn text(text: &str) -> Cow<'_, str> {
    // A borrow of the guard cannot leave this function: copy when it changed.
    match GLOBAL.load().redact(text) {
        Cow::Borrowed(_) => Cow::Borrowed(text),
        Cow::Owned(clean) => Cow::Owned(clean),
    }
}

/// `text` redacted, owned.
pub fn string(text: String) -> String {
    match GLOBAL.load().redact(&text) {
        Cow::Borrowed(_) => text,
        Cow::Owned(clean) => clean,
    }
}

/// Every string in `value` redacted with the current automaton.
pub fn json(value: &mut serde_json::Value) {
    GLOBAL.load().redact_json(value)
}

/// A log writer that redacts every line it is given with the current
/// automaton (SEC0 E14 c): the daemon's log never holds a vault value, also not
/// one an error or a library printed. A formatter writes one record at a time.
pub struct Writer<W>(pub W);

impl<W: std::io::Write> std::io::Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let redactor = current();
        if redactor.is_empty() {
            return self.0.write(buf);
        }
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(redactor.redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// A text that arrives in pieces (a run's text deltas): it holds back the
/// last [`Redactor::hold`] bytes, so a value split over two pieces is still
/// found, and lets them go at [`Stream::flush`].
#[derive(Default)]
pub struct Stream {
    pending: String,
}

impl Stream {
    /// What of `pending + piece` can go now, redacted.
    pub fn push(&mut self, redactor: &Redactor, piece: &str) -> String {
        if redactor.is_empty() {
            let mut out = std::mem::take(&mut self.pending);
            out.push_str(piece);
            return out;
        }
        self.pending.push_str(piece);
        let Some(automaton) = &redactor.automaton else { unreachable!() };
        let text = std::mem::take(&mut self.pending);
        let mut cut = text.len().saturating_sub(redactor.hold());
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let mut out = String::new();
        let mut last = 0;
        for found in automaton.find_iter(text.as_bytes()) {
            // A value that starts before the cut is whole in `text` (it is at
            // most `hold + 1` long): it goes now, even past the cut.
            if found.start() >= cut {
                break;
            }
            out.push_str(&text[last..found.start()]);
            out.push_str(&marker(&redactor.names[redactor.owner[found.pattern().as_usize()]]));
            last = found.end();
        }
        let keep_from = cut.max(last);
        out.push_str(&text[last..keep_from]);
        self.pending = text[keep_from..].to_owned();
        out
    }

    /// The rest, redacted (the end of the stream, or another event in between).
    pub fn flush(&mut self, redactor: &Redactor) -> String {
        let rest = std::mem::take(&mut self.pending);
        redactor.redact(&rest).into_owned()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Tests that install a global automaton take this first.
#[cfg(test)]
pub(crate) static TEST_GLOBAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
