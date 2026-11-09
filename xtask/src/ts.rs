//! Emits TypeScript from the schema model: `models.ts` (interfaces, string unions and
//! discriminated unions) and `methods.ts` (method names, their params and the protocol major).
//!
//! Types only: JSON is used as it comes off the wire, keys stay snake_case, nothing is parsed
//! at runtime. A key the schema does not require is optional. Output is deterministic like the Dart.

use crate::catalog::{Entry, Group};
use crate::dart::pascal;
use crate::schema::{Body, Enum, Field, Item, Model, Struct, Tagging, Ty, Union, Variant};
use aurelius_domain::{AppError, ErrorCode};
use aurelius_protocol::{Method, Outcome, Request, ServerMessage, WriteMethod};
use serde_json::json;
use std::collections::BTreeSet;
use std::fmt::Write as _;

const GENERATED: &str = "// GENERATED CODE - DO NOT EDIT.\n// Run: cargo xtask gen-ts\n";

/// A write without params, for `wire_samples`.
const WRITE_SAMPLE: &str = "power.cancel";

macro_rules! w {
    ($out:expr) => { $out.push('\n') };
    ($out:expr, $($arg:tt)*) => { writeln!($out, $($arg)*).expect("writing to a String") };
}

fn ts_str(s: &str) -> String {
    serde_json::to_string(s).expect("a string is JSON")
}

/// An object key: bare when it is an identifier, quoted otherwise.
fn key(s: &str) -> String {
    let mut chars = s.chars();
    let bare = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare { s.to_owned() } else { ts_str(s) }
}

fn doc(out: &mut String, indent: &str, text: Option<&str>) {
    let Some(text) = text else { return };
    w!(out, "{indent}/**");
    for line in text.lines() {
        // `*/` would end the comment.
        let line = line.replace("*/", "* /");
        if line.is_empty() {
            w!(out, "{indent} *");
        } else {
            w!(out, "{indent} * {line}");
        }
    }
    w!(out, "{indent} */");
}

fn ts_type(t: &Ty) -> String {
    match t {
        Ty::String => "string".to_owned(),
        Ty::Int | Ty::Double => "number".to_owned(),
        Ty::Bool => "boolean".to_owned(),
        Ty::Dynamic => "unknown".to_owned(),
        Ty::Ref(name) => pascal(name),
        Ty::List(inner) => match **inner {
            Ty::Nullable(_) => format!("({})[]", ts_type(inner)),
            _ => format!("{}[]", ts_type(inner)),
        },
        Ty::Map(inner) => format!("{{ [key: string]: {} }}", ts_type(inner)),
        Ty::Nullable(inner) => format!("{} | null", ts_type(inner)),
        Ty::Tuple => "unknown[]".to_owned(),
    }
}

fn field_type(f: &Field) -> String {
    match &f.konst {
        Some(value) => ts_str(value),
        None => ts_type(&f.ty),
    }
}

fn emit_fields(out: &mut String, indent: &str, fields: &[Field]) {
    for f in fields {
        doc(out, indent, f.doc.as_deref());
        let optional = if f.required { "" } else { "?" };
        w!(out, "{indent}{}{optional}: {};", key(&f.json), field_type(f));
    }
}

fn emit_struct(out: &mut String, s: &Struct) {
    doc(out, "", s.doc.as_deref());
    w!(out, "export interface {} {{", pascal(&s.name));
    emit_fields(out, "  ", &s.fields);
    w!(out, "}}");
}

fn emit_enum(out: &mut String, e: &Enum) {
    let name = pascal(&e.name);
    doc(out, "", e.doc.as_deref());
    w!(out, "export type {name} =");
    for (value, value_doc) in &e.values {
        doc(out, "  ", value_doc.as_deref());
        w!(out, "  | {}", ts_str(value));
    }
    // A union of no values cannot occur: the schema reader only makes enums of one or more.
    w!(out, ";");
}

/// One object per line, indented for a member of a union.
fn inline_fields(fields: &[Field], first: Option<String>) -> String {
    let mut parts: Vec<String> = first.into_iter().collect();
    parts.extend(fields.iter().map(|f| format!("{}{}: {}", key(&f.json), if f.required { "" } else { "?" }, field_type(f))));
    format!("{{ {} }}", parts.join("; "))
}

fn variant_type(u: &Union, v: &Variant) -> String {
    let tag_value = ts_str(&v.tag);
    match &u.tagging {
        Tagging::Internal { tag } => {
            let head = format!("{}: {tag_value}", key(tag));
            match &v.body {
                Body::Unit => format!("{{ {head} }}"),
                Body::Fields(fields) => inline_fields(fields, Some(head)),
                Body::Payload(ty) => format!("{{ {head} }} & {}", ts_type(ty)),
            }
        }
        Tagging::Adjacent { tag, content } => {
            let head = format!("{}: {tag_value}", key(tag));
            match &v.body {
                Body::Unit => format!("{{ {head} }}"),
                Body::Fields(fields) => format!("{{ {head}; {}: {} }}", key(content), inline_fields(fields, None)),
                Body::Payload(ty @ Ty::Nullable(_)) => format!("{{ {head}; {}?: {} }}", key(content), ts_type(ty)),
                Body::Payload(ty) => format!("{{ {head}; {}: {} }}", key(content), ts_type(ty)),
            }
        }
        Tagging::External => match &v.body {
            Body::Unit => tag_value,
            Body::Fields(fields) => format!("{{ {}: {} }}", key(&v.tag), inline_fields(fields, None)),
            Body::Payload(ty) => format!("{{ {}: {} }}", key(&v.tag), ts_type(ty)),
        },
    }
}

fn emit_union(out: &mut String, u: &Union) {
    doc(out, "", u.doc.as_deref());
    w!(out, "export type {} =", pascal(&u.name));
    for v in &u.variants {
        doc(out, "  ", v.doc.as_deref());
        w!(out, "  | ({})", variant_type(u, v));
    }
    w!(out, ";");
}

/// `models.ts`. `Map` and generics are not used, so a schema type may hide any global;
/// only two items that end up with the same TypeScript name are an error.
pub fn models(model: &Model) -> String {
    let mut names = BTreeSet::new();
    let mut out = String::new();
    out.push_str(GENERATED);
    for (name, item) in &model.items {
        let ts_name = pascal(name);
        assert!(names.insert(ts_name.clone()), "the types collide as the TypeScript name `{ts_name}`");
        w!(out);
        match item {
            Item::Struct(s) => emit_struct(&mut out, s),
            Item::Enum(e) => emit_enum(&mut out, e),
            Item::Union(u) => emit_union(&mut out, u),
        }
    }
    out
}

fn params_type(body: &Body) -> String {
    match body {
        Body::Unit => "undefined".to_owned(),
        Body::Fields(fields) => inline_fields(fields, None),
        Body::Payload(ty) => ts_type(ty),
    }
}

fn group_name(group: Group) -> &'static str {
    match group {
        Group::Read => "read",
        Group::Host => "host",
        Group::Command => "command",
    }
}

/// `methods.ts`. Writes need an `idempotency_key` on the wire, so a client has to tell
/// them from reads: `METHODS` gives the group of every method.
pub fn methods(entries: &[Entry], major: u16) -> String {
    let mut out = String::new();
    out.push_str(GENERATED);
    w!(out);
    w!(out, "import type * as M from \"./models\";");
    w!(out);
    w!(out, "/** The protocol major this app speaks. The server refuses another one. */");
    w!(out, "export const PROTOCOL_MAJOR = {major};");
    w!(out);
    w!(out, "/** `read`: reads and session methods. `host`: writes only the host can do. `command`: domain commands. */");
    w!(out, "export type MethodGroup = \"read\" | \"host\" | \"command\";");
    w!(out);
    w!(out, "/** The wire name of every method, with its group. */");
    w!(out, "export const METHODS = {{");
    for e in entries {
        w!(out, "  {}: {},", ts_str(&e.method), ts_str(group_name(e.group)));
    }
    w!(out, "}} as const satisfies Record<string, MethodGroup>;");
    w!(out);
    w!(out, "export type MethodName = keyof typeof METHODS;");
    w!(out);
    w!(out, "/** The `params` of every method; `undefined`: it takes none. */");
    w!(out, "export interface MethodParams {{");
    for e in entries {
        doc(&mut out, "  ", e.doc.as_deref());
        w!(out, "  {}: {};", ts_str(&e.method), qualify(&params_type(&e.body), &e.body));
    }
    w!(out, "}}");
    out
}

/// The params of `methods.ts` name their types through the `M` import.
fn qualify(text: &str, body: &Body) -> String {
    let mut refs = BTreeSet::new();
    match body {
        Body::Unit => {}
        Body::Payload(ty) => collect_refs(ty, &mut refs),
        Body::Fields(fields) => fields.iter().for_each(|f| collect_refs(&f.ty, &mut refs)),
    }
    if refs.is_empty() {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut word = String::new();
    let mut in_string = false;
    for c in text.chars().chain(std::iter::once('\0')) {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        if !in_string && refs.contains(&word) {
            out.push_str("M.");
        }
        out.push_str(&word);
        word.clear();
        if c == '"' {
            in_string = !in_string;
        }
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

fn collect_refs(t: &Ty, refs: &mut BTreeSet<String>) {
    match t {
        Ty::Ref(name) => {
            refs.insert(pascal(name));
        }
        Ty::List(inner) | Ty::Map(inner) | Ty::Nullable(inner) => collect_refs(inner, refs),
        Ty::String | Ty::Int | Ty::Double | Ty::Bool | Ty::Dynamic | Ty::Tuple => {}
    }
}

/// `index.ts`: the one import for the rest of the app.
pub fn barrel() -> String {
    format!("{GENERATED}\nexport * from \"./methods\";\nexport type * from \"./models\";\n")
}

/// `wire-samples.json`: messages as the server's own types write them. The envelope is not in
/// the schema, so the client's tests hold its hand-written `wire.ts` against these.
pub fn wire_samples() -> String {
    const ID: &str = "01900000-0000-7000-8000-000000000001";
    let message = |value: serde_json::Value| {
        let parsed: ServerMessage = serde_json::from_value(value).expect("a sample the server can read");
        serde_json::to_value(parsed).expect("a message is JSON")
    };
    let key = aurelius_domain::CommandId::parse(ID).expect("a valid id");
    let write: WriteMethod = serde_json::from_value(json!({ "method": WRITE_SAMPLE })).expect("a sample the server can read");
    let mut write = serde_json::to_value(Request::write(8, key, write)).expect("a request is JSON");
    write["expected_version"] = json!(3);
    let mut samples = json!({
        "request_read": Request::read(7, Method::UserWhoami),
        "request_write": write,
        "response_error": ServerMessage::Response { request_id: 7, outcome: Outcome::Error(AppError::new(ErrorCode::Forbidden, "Not here.")) },
        "response_receipt": message(json!({
            "type": "response", "request_id": 8,
            "ok": { "type": "receipt", "value": { "idempotency_key": ID, "committed_seq": 91, "replayed": false, "result": { "kind": "command", "value": { "type": "chat_message_sent", "conversation_id": ID, "message_id": ID, "run_id": ID } } } }
        })),
        "stream_append": message(json!({ "type": "stream_append", "run_id": ID, "conversation_id": ID, "message_id": ID, "offset": 5, "text": " there" })),
        "signed_out": message(json!({ "type": "signed_out", "reason": "revoked" })),
    });
    // The daemon turns on serde_json's `preserve_order`, and in a workspace build that holds for
    // this crate too: without sorting, the file depends on what else cargo is building.
    samples.sort_all_objects();
    let mut out = serde_json::to_string_pretty(&samples).expect("samples are JSON");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse;

    fn model(defs: serde_json::Value) -> Model {
        parse(defs.as_object().unwrap())
    }

    #[test]
    fn a_struct_keeps_its_wire_keys_and_optional_fields() {
        let ts = models(&model(json!({
            "Project": { "type": "object", "description": "A project.", "properties": {
                "name": { "type": "string" },
                "folder": { "type": ["string", "null"] },
                "max_runs": { "type": "integer", "default": 3 },
                "tags": { "type": "array", "items": { "type": "string" } },
                "per": { "type": "object", "additionalProperties": { "type": "boolean" } }
            }, "required": ["name"] }
        })));
        assert!(ts.starts_with("// GENERATED CODE - DO NOT EDIT.\n// Run: cargo xtask gen-ts\n"));
        assert!(ts.contains("/**\n * A project.\n */\nexport interface Project {"));
        assert!(ts.contains("  name: string;\n"));
        assert!(ts.contains("  folder?: string | null;\n"));
        assert!(ts.contains("  max_runs?: number;\n"));
        assert!(ts.contains("  tags?: string[];\n"));
        assert!(ts.contains("  per?: { [key: string]: boolean };\n"));
    }

    #[test]
    fn a_string_enum_is_a_union_of_its_wire_values() {
        let ts = models(&model(json!({ "Status": { "type": "string", "enum": ["open", "task.closed"] } })));
        assert!(ts.contains("export type Status =\n  | \"open\"\n  | \"task.closed\"\n;\n"));
    }

    #[test]
    fn unions_keep_their_tagging() {
        let ts = models(&model(json!({
            "Create": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
            "Cmd": { "oneOf": [
                { "type": "object", "properties": { "type": { "const": "create" } }, "allOf": [{ "$ref": "#/$defs/Create" }], "required": ["type"] },
                { "type": "object", "properties": { "type": { "const": "ping" } }, "required": ["type"] }
            ] },
            "Call": { "oneOf": [
                { "type": "object", "properties": { "method": { "const": "x.set" }, "params": { "$ref": "#/$defs/Create" } }, "required": ["method", "params"] },
                { "type": "object", "properties": { "method": { "const": "x.get" } }, "required": ["method"] }
            ] }
        })));
        assert!(ts.contains("  | ({ type: \"create\" } & Create)\n"), "{ts}");
        assert!(ts.contains("  | ({ type: \"ping\" })\n"), "{ts}");
        assert!(ts.contains("  | ({ method: \"x.set\"; params: Create })\n"), "{ts}");
        assert!(ts.contains("  | ({ method: \"x.get\" })\n"), "{ts}");
    }

    #[test]
    fn methods_name_their_params_through_the_models_import() {
        let entries = [
            Entry { method: "x.get".to_owned(), group: Group::Read, doc: None, body: Body::Unit },
            Entry { method: "x.set".to_owned(), group: Group::Command, doc: None, body: Body::Payload(Ty::Ref("Create".to_owned())) },
        ];
        let ts = methods(&entries, 5);
        assert!(ts.contains("export const PROTOCOL_MAJOR = 5;"));
        assert!(ts.contains("  \"x.get\": \"read\",\n"));
        assert!(ts.contains("  \"x.set\": \"command\",\n"));
        assert!(ts.contains("  \"x.get\": undefined;\n"));
        assert!(ts.contains("  \"x.set\": M.Create;\n"));
    }

    #[test]
    fn the_wire_samples_are_sorted_whatever_cargo_builds_alongside() {
        fn assert_sorted(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    let keys: Vec<_> = map.keys().collect();
                    assert!(keys.is_sorted(), "{keys:?}");
                    map.values().for_each(assert_sorted);
                }
                serde_json::Value::Array(items) => items.iter().for_each(assert_sorted),
                _ => {}
            }
        }
        assert_sorted(&serde_json::from_str(&wire_samples()).unwrap());
    }

    #[test]
    fn a_comment_cannot_be_closed_by_its_text() {
        let mut out = String::new();
        doc(&mut out, "", Some("glob: src/*/x and a */ end"));
        assert_eq!(out.matches("*/").count(), 1, "only the real end: {out}");
    }
}
