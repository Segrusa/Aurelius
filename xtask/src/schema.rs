//! Reads the JSON Schema (draft 2020-12, as `schemars` 1.x writes it) of the protocol
//! types into a small model: structs, string enums and tagged unions.
//!
//! Shapes that are not understood panic with the schema path instead of producing
//! wrong Dart. Definitions that are plain aliases (id newtypes are `string`/`uuid`)
//! are inlined where they are used.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    String,
    Int,
    Double,
    Bool,
    /// Any JSON (`serde_json::Value`, untagged unions).
    Dynamic,
    /// A struct, enum or union of the model.
    Ref(String),
    List(Box<Ty>),
    /// `Map<String, T>`.
    Map(Box<Ty>),
    Nullable(Box<Ty>),
    /// A fixed-size array (`prefixItems`); kept as `List<dynamic>`.
    Tuple,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The JSON key.
    pub json: String,
    pub ty: Ty,
    /// The key is in the schema's `required`.
    pub required: bool,
    pub default: Option<Value>,
    /// A `const` (or one-value `enum`) string: the tag of a union variant.
    pub konst: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub name: String,
    pub doc: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: String,
    pub doc: Option<String>,
    /// `(wire value, doc)`, sorted by wire value.
    pub values: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Unit,
    Fields(Vec<Field>),
    /// One value: a struct that is spread (internal tag) or the content (adjacent/external tag).
    Payload(Ty),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub tag: String,
    pub doc: Option<String>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tagging {
    /// `{ "<tag>": "x", ...fields }`.
    Internal { tag: String },
    /// `{ "<tag>": "x", "<content>": ... }`.
    Adjacent { tag: String, content: String },
    /// `{ "x": ... }` or just `"x"` for a unit variant.
    External,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Union {
    pub name: String,
    pub doc: Option<String>,
    pub tagging: Tagging,
    /// Sorted by tag.
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Struct(Struct),
    Enum(Enum),
    Union(Union),
}

#[derive(Debug, Default)]
pub struct Model {
    pub items: BTreeMap<String, Item>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Struct,
    Enum,
    Union,
    Alias,
}

struct Ctx<'a> {
    defs: &'a Map<String, Value>,
    aliases: BTreeSet<String>,
    /// While a union's alternatives are read, an inline object property (`params`) is only a placeholder:
    /// the tagging code reads it properly afterwards.
    lenient: std::cell::Cell<bool>,
}

pub fn parse(defs: &Map<String, Value>) -> Model {
    let aliases = defs.iter().filter(|(_, s)| kind_of(s) == Kind::Alias).map(|(n, _)| n.clone()).collect();
    let ctx = Ctx { defs, aliases, lenient: std::cell::Cell::new(false) };
    let mut model = Model::default();
    for (name, schema) in defs {
        let path = format!("#/$defs/{name}");
        let doc = doc_of(schema);
        let item = match kind_of(schema) {
            Kind::Alias => continue,
            Kind::Struct => {
                let mut fields = fields_of(&ctx, schema, &path, &mut Vec::new(), 0);
                fields.sort_by(|a, b| a.json.cmp(&b.json));
                Item::Struct(Struct { name: name.clone(), doc, fields })
            }
            Kind::Enum => {
                let mut values = enum_values(schema).unwrap_or_else(|| panic!("{path}: not a string enum"));
                values.sort();
                values.dedup_by(|a, b| a.0 == b.0);
                Item::Enum(Enum { name: name.clone(), doc, values })
            }
            Kind::Union => Item::Union(parse_union(&ctx, name, schema, &path)),
        };
        model.items.insert(name.clone(), item);
    }
    model
}

fn doc_of(v: &Value) -> Option<String> {
    v.get("description").and_then(Value::as_str).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn alts_of(o: &Map<String, Value>) -> Option<&Vec<Value>> {
    o.get("oneOf").or_else(|| o.get("anyOf")).and_then(Value::as_array)
}

fn is_objectish(v: &Value) -> bool {
    v.get("properties").is_some() || v.get("type").and_then(Value::as_str) == Some("object")
}

fn kind_of(s: &Value) -> Kind {
    let Some(o) = s.as_object() else { return Kind::Alias };
    if o.contains_key("properties") || o.get("type").and_then(Value::as_str) == Some("object") {
        let free_map = !o.contains_key("properties") && o.get("additionalProperties").is_some_and(|a| *a != Value::Bool(false));
        return if free_map { Kind::Alias } else { Kind::Struct };
    }
    if enum_values(s).is_some() {
        return Kind::Enum;
    }
    if alts_of(o).is_some_and(|alts| alts.iter().any(is_objectish)) {
        return Kind::Union;
    }
    Kind::Alias
}

/// A string enum: `enum: [..]`, or `oneOf` of `const` strings / one-value enums.
fn enum_values(s: &Value) -> Option<Vec<(String, Option<String>)>> {
    let o = s.as_object()?;
    if let Some(e) = o.get("enum").and_then(Value::as_array) {
        let values: Option<Vec<_>> = e.iter().map(|v| v.as_str().map(|t| (t.to_owned(), None))).collect();
        return values.filter(|v| !v.is_empty());
    }
    let alts = alts_of(o)?;
    let mut out = Vec::new();
    for alt in alts {
        let doc = doc_of(alt);
        if let Some(c) = alt.get("const").and_then(Value::as_str) {
            out.push((c.to_owned(), doc));
        } else if let Some(e) = alt.get("enum").and_then(Value::as_array) {
            for v in e {
                out.push((v.as_str()?.to_owned(), doc.clone()));
            }
        } else {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn ref_name(r: &Value, path: &str) -> String {
    let text = r.as_str().unwrap_or_else(|| panic!("{path}: $ref is not a string"));
    text.strip_prefix("#/$defs/").or_else(|| text.strip_prefix("#/definitions/")).unwrap_or_else(|| panic!("{path}: unsupported $ref {text}")).to_owned()
}

fn is_null(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("null") || v.get("const").is_some_and(Value::is_null)
}

fn nullable(ty: Ty) -> Ty {
    match ty {
        Ty::Nullable(_) | Ty::Dynamic => ty,
        other => Ty::Nullable(Box::new(other)),
    }
}

fn ty_of(ctx: &Ctx, s: &Value, path: &str, depth: usize) -> Ty {
    assert!(depth < 32, "{path}: schema nests too deep (alias cycle?)");
    let o = match s {
        Value::Bool(true) => return Ty::Dynamic,
        Value::Bool(false) => panic!("{path}: the schema `false` has no Dart type"),
        Value::Object(o) => o,
        _ => panic!("{path}: a schema must be an object or a boolean"),
    };
    if let Some(r) = o.get("$ref") {
        let name = ref_name(r, path);
        let def = ctx.defs.get(&name).unwrap_or_else(|| panic!("{path}: $ref to unknown definition {name}"));
        return if ctx.aliases.contains(&name) { ty_of(ctx, def, &format!("#/$defs/{name}"), depth + 1) } else { Ty::Ref(name) };
    }
    if let Some(c) = o.get("const") {
        return match c {
            Value::String(_) => Ty::String,
            Value::Bool(_) => Ty::Bool,
            Value::Number(n) if n.is_f64() => Ty::Double,
            Value::Number(_) => Ty::Int,
            _ => Ty::Dynamic,
        };
    }
    if let Some(e) = o.get("enum").and_then(Value::as_array) {
        return if e.iter().all(Value::is_string) { Ty::String } else { Ty::Dynamic };
    }
    if let Some(all) = o.get("allOf").and_then(Value::as_array) {
        return match all.as_slice() {
            [one] => ty_of(ctx, one, path, depth + 1),
            _ => panic!("{path}: allOf with {} members is not supported here", all.len()),
        };
    }
    if let Some(alts) = alts_of(o) {
        let has_null = alts.iter().any(is_null);
        let rest: Vec<&Value> = alts.iter().filter(|a| !is_null(a)).collect();
        let ty = match rest.as_slice() {
            [] => Ty::Dynamic,
            [one] => ty_of(ctx, one, path, depth + 1),
            many if many.iter().all(|a| enum_values(&serde_json::json!({ "oneOf": [a] })).is_some()) => Ty::String,
            // An untagged union: any JSON.
            _ => Ty::Dynamic,
        };
        return if has_null { nullable(ty) } else { ty };
    }
    match o.get("type") {
        Some(Value::String(t)) => match t.as_str() {
            "string" => Ty::String,
            "integer" => Ty::Int,
            "number" => Ty::Double,
            "boolean" => Ty::Bool,
            "null" => Ty::Dynamic,
            "array" => {
                if o.contains_key("prefixItems") {
                    Ty::Tuple
                } else {
                    match o.get("items") {
                        Some(items) => Ty::List(Box::new(ty_of(ctx, items, &format!("{path}/items"), depth + 1))),
                        None => Ty::List(Box::new(Ty::Dynamic)),
                    }
                }
            }
            "object" => {
                if o.get("properties").and_then(Value::as_object).is_some_and(|p| !p.is_empty()) {
                    panic!("{path}: an inline object with properties is not supported (give the type a name)");
                }
                match o.get("additionalProperties") {
                    Some(a @ Value::Object(_)) => Ty::Map(Box::new(ty_of(ctx, a, &format!("{path}/additionalProperties"), depth + 1))),
                    _ => Ty::Map(Box::new(Ty::Dynamic)),
                }
            }
            other => panic!("{path}: unknown type {other:?}"),
        },
        Some(Value::Array(types)) => {
            let names: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
            let has_null = names.contains(&"null");
            let rest: Vec<&str> = names.into_iter().filter(|t| *t != "null").collect();
            let ty = match rest.as_slice() {
                [] => Ty::Dynamic,
                [one] => {
                    let mut single = o.clone();
                    single.insert("type".into(), Value::String((*one).to_owned()));
                    ty_of(ctx, &Value::Object(single), path, depth + 1)
                }
                _ => Ty::Dynamic,
            };
            if has_null { nullable(ty) } else { ty }
        }
        Some(_) => panic!("{path}: `type` must be a string or an array"),
        None => {
            if o.contains_key("properties") || o.contains_key("items") || o.contains_key("additionalProperties") {
                panic!("{path}: a schema with properties/items but no type is not supported");
            }
            Ty::Dynamic
        }
    }
}

fn field(ctx: &Ctx, key: &str, schema: &Value, required: bool, path: &str) -> Field {
    let konst = schema
        .get("const")
        .and_then(Value::as_str)
        .or_else(|| match schema.get("enum").and_then(Value::as_array).map(Vec::as_slice) {
            Some([Value::String(one)]) => Some(one.as_str()),
            _ => None,
        })
        .map(str::to_owned);
    let ty = if ctx.lenient.get() && inline_object(schema) { Ty::Dynamic } else { ty_of(ctx, schema, path, 0) };
    Field { json: key.to_owned(), ty, required, default: schema.get("default").cloned(), konst, doc: doc_of(schema) }
}

/// The properties written directly on this schema.
fn own_fields(ctx: &Ctx, s: &Value, path: &str) -> Vec<Field> {
    let Some(props) = s.get("properties").and_then(Value::as_object) else { return Vec::new() };
    let required: BTreeSet<&str> = s.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    props.iter().map(|(k, v)| field(ctx, k, v, required.contains(k.as_str()), &format!("{path}/properties/{k}"))).collect()
}

fn merge(out: &mut Vec<Field>, more: Vec<Field>) {
    for f in more {
        if !out.iter().any(|g| g.json == f.json) {
            out.push(f);
        }
    }
}

/// All fields of an object schema: its own properties, those of `allOf`/`$ref`
/// members (a flattened struct) and the optional fields of `oneOf`/`anyOf`
/// alternatives (a flattened enum). `refs` collects the direct `$ref` targets.
fn fields_of(ctx: &Ctx, s: &Value, path: &str, refs: &mut Vec<String>, depth: usize) -> Vec<Field> {
    assert!(depth < 16, "{path}: flatten nests too deep");
    let o = s.as_object().unwrap_or_else(|| panic!("{path}: expected an object schema"));
    let mut out = own_fields(ctx, s, path);
    let from_ref = |r: &Value, refs: &mut Vec<String>| -> Vec<Field> {
        let name = ref_name(r, path);
        let def = ctx.defs.get(&name).unwrap_or_else(|| panic!("{path}: $ref to unknown definition {name}"));
        refs.push(name.clone());
        fields_of(ctx, def, &format!("#/$defs/{name}"), &mut Vec::new(), depth + 1)
    };
    if let Some(r) = o.get("$ref") {
        let more = from_ref(r, refs);
        merge(&mut out, more);
    }
    if let Some(members) = o.get("allOf").and_then(Value::as_array) {
        for (i, m) in members.iter().enumerate() {
            let more = match m.get("$ref") {
                Some(r) => from_ref(r, refs),
                None => fields_of(ctx, m, &format!("{path}/allOf/{i}"), &mut Vec::new(), depth + 1),
            };
            merge(&mut out, more);
        }
    }
    if let Some(alts) = alts_of(o) {
        for (i, a) in alts.iter().enumerate() {
            let mut sub = match a.get("$ref") {
                Some(r) => from_ref(r, &mut Vec::new()),
                None => fields_of(ctx, a, &format!("{path}/alts/{i}"), &mut Vec::new(), depth + 1),
            };
            for f in &mut sub {
                f.required = false;
                f.ty = nullable(f.ty.clone());
            }
            merge(&mut out, sub);
        }
    }
    out
}

struct Alt {
    doc: Option<String>,
    /// A unit variant written as a plain string.
    unit: Option<String>,
    own: Vec<Field>,
    all: Vec<Field>,
    refs: Vec<String>,
    /// Has `allOf`/`$ref`/`oneOf`/`anyOf` next to the properties (flattened content).
    composed: bool,
    schema: Value,
}

fn read_alt(ctx: &Ctx, a: &Value, path: &str) -> Alt {
    let unit = a
        .get("const")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| match a.get("enum").and_then(Value::as_array).map(Vec::as_slice) {
            Some([Value::String(one)]) if a.get("properties").is_none() => Some(one.clone()),
            _ => None,
        });
    let mut refs = Vec::new();
    ctx.lenient.set(true);
    let (own, all) = if unit.is_some() { (Vec::new(), Vec::new()) } else { (own_fields(ctx, a, path), fields_of(ctx, a, path, &mut refs, 0)) };
    ctx.lenient.set(false);
    let composed = ["allOf", "$ref", "oneOf", "anyOf"].iter().any(|k| a.get(*k).is_some());
    Alt { doc: doc_of(a), unit, own, all, refs, composed, schema: a.clone() }
}

fn is_struct_def(ctx: &Ctx, name: &str) -> bool {
    ctx.defs.get(name).is_some_and(|d| kind_of(d) == Kind::Struct)
}

/// A property whose value is an inline object becomes the variant's fields.
fn inline_object(prop: &Value) -> bool {
    let composed = ["$ref", "allOf", "oneOf", "anyOf"].iter().any(|k| prop.get(*k).is_some());
    !composed && (prop.get("properties").is_some() || (prop.get("type").and_then(Value::as_str) == Some("object") && prop.get("additionalProperties").is_none()))
}

fn sorted(mut fields: Vec<Field>) -> Vec<Field> {
    fields.sort_by(|a, b| a.json.cmp(&b.json));
    fields
}

fn parse_union(ctx: &Ctx, name: &str, s: &Value, path: &str) -> Union {
    let o = s.as_object().expect("a union is an object schema");
    let alts_json = alts_of(o).unwrap_or_else(|| panic!("{path}: union without oneOf/anyOf"));
    let alts: Vec<Alt> = alts_json.iter().enumerate().map(|(i, a)| read_alt(ctx, a, &format!("{path}/oneOf/{i}"))).collect();
    let has_units = alts.iter().any(|a| a.unit.is_some());
    let objects: Vec<&Alt> = alts.iter().filter(|a| a.unit.is_none()).collect();

    // The tag: a const property that every object alternative has.
    let mut tag_candidates: Option<BTreeSet<String>> = None;
    for a in &objects {
        let names: BTreeSet<String> = a.own.iter().filter(|f| f.konst.is_some()).map(|f| f.json.clone()).collect();
        tag_candidates = Some(match tag_candidates {
            None => names,
            Some(prev) => prev.intersection(&names).cloned().collect(),
        });
    }
    let tag: Option<String> = if has_units {
        None
    } else {
        tag_candidates.and_then(|c| ["type", "kind", "method", "op"].iter().find(|p| c.contains(**p)).map(|p| (*p).to_owned()).or_else(|| c.into_iter().next()))
    };

    let mut variants: Vec<Variant> = Vec::new();
    let tagging;
    match &tag {
        Some(tag) => {
            // Adjacent: no composed alternative, at most one other property, always `params` or `value`.
            let mut content: Option<String> = None;
            let mut adjacent = true;
            for a in &objects {
                let extra: Vec<&Field> = a.own.iter().filter(|f| f.json != *tag).collect();
                if a.composed || extra.len() > 1 {
                    adjacent = false;
                    break;
                }
                if let Some(f) = extra.first() {
                    if f.json != "params" && f.json != "value" || content.as_ref().is_some_and(|c| *c != f.json) {
                        adjacent = false;
                        break;
                    }
                    content = Some(f.json.clone());
                }
            }
            let content = if adjacent { content } else { None };
            tagging = match &content {
                Some(c) => Tagging::Adjacent { tag: tag.clone(), content: c.clone() },
                None => Tagging::Internal { tag: tag.clone() },
            };
            for (i, a) in alts.iter().enumerate() {
                let vpath = format!("{path}/oneOf/{i}");
                let wire = a.own.iter().find(|f| f.json == *tag).and_then(|f| f.konst.clone()).unwrap_or_else(|| panic!("{vpath}: variant without a `{tag}` const"));
                let body = match &content {
                    Some(c) => match a.own.iter().find(|f| f.json == *c) {
                        None => Body::Unit,
                        Some(f) => {
                            let prop = &a.schema["properties"][c.as_str()];
                            if inline_object(prop) {
                                Body::Fields(sorted(fields_of(ctx, prop, &format!("{vpath}/properties/{c}"), &mut Vec::new(), 0)))
                            } else {
                                Body::Payload(f.ty.clone())
                            }
                        }
                    },
                    None => {
                        let rest = a.own.iter().filter(|f| f.json != *tag).count();
                        let flattened = ["oneOf", "anyOf"].iter().any(|k| a.schema.get(*k).is_some());
                        if a.refs.len() == 1 && rest == 0 && !flattened && is_struct_def(ctx, &a.refs[0]) {
                            Body::Payload(Ty::Ref(a.refs[0].clone()))
                        } else {
                            let fields: Vec<Field> = a.all.iter().filter(|f| f.json != *tag).cloned().collect();
                            if fields.is_empty() { Body::Unit } else { Body::Fields(sorted(fields)) }
                        }
                    }
                };
                variants.push(Variant { tag: wire, doc: a.doc.clone(), body });
            }
        }
        None => {
            tagging = Tagging::External;
            for (i, a) in alts.iter().enumerate() {
                let vpath = format!("{path}/oneOf/{i}");
                if let Some(u) = &a.unit {
                    variants.push(Variant { tag: u.clone(), doc: a.doc.clone(), body: Body::Unit });
                    continue;
                }
                if a.composed || a.own.len() != 1 {
                    panic!("{vpath}: not a tagged variant (no common const tag, and not `{{ \"name\": value }}`); untagged unions are not supported at a definition");
                }
                let f = &a.own[0];
                let prop = &a.schema["properties"][f.json.as_str()];
                let body = if inline_object(prop) {
                    Body::Fields(sorted(fields_of(ctx, prop, &format!("{vpath}/properties/{}", f.json), &mut Vec::new(), 0)))
                } else {
                    Body::Payload(f.ty.clone())
                };
                variants.push(Variant { tag: f.json.clone(), doc: a.doc.clone(), body });
            }
        }
    }
    variants.sort_by(|a, b| a.tag.cmp(&b.tag));
    for pair in variants.windows(2) {
        assert!(pair[0].tag != pair[1].tag, "{path}: the tag `{}` is used twice", pair[0].tag);
    }
    Union { name: name.to_owned(), doc: doc_of(s), tagging, variants }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_defs(v: Value) -> Model {
        parse(v.as_object().unwrap())
    }

    fn item<'a>(m: &'a Model, n: &str) -> &'a Item {
        m.items.get(n).unwrap_or_else(|| panic!("no item {n}"))
    }

    #[test]
    fn a_struct_with_an_optional_field_and_an_id_alias() {
        let m = parse_defs(json!({
            "ProjectId": { "type": "string", "format": "uuid" },
            "Project": {
                "type": "object",
                "properties": {
                    "id": { "$ref": "#/$defs/ProjectId" },
                    "folder": { "type": ["string", "null"] },
                    "count": { "type": "integer", "format": "uint32", "minimum": 0 },
                    "tags": { "type": "array", "items": { "type": "string" }, "default": [] },
                    "parent": { "anyOf": [{ "$ref": "#/$defs/ProjectId" }, { "type": "null" }] },
                    "raw": true
                },
                "required": ["id", "count"]
            }
        }));
        assert!(!m.items.contains_key("ProjectId"), "an alias is inlined");
        let Item::Struct(s) = item(&m, "Project") else { panic!("struct") };
        let by = |n: &str| s.fields.iter().find(|f| f.json == n).unwrap();
        assert_eq!(by("id").ty, Ty::String);
        assert!(by("id").required);
        assert_eq!(by("folder").ty, Ty::Nullable(Box::new(Ty::String)));
        assert_eq!(by("count").ty, Ty::Int);
        assert_eq!(by("tags").ty, Ty::List(Box::new(Ty::String)));
        assert!(!by("tags").required);
        assert_eq!(by("parent").ty, Ty::Nullable(Box::new(Ty::String)));
        assert_eq!(by("raw").ty, Ty::Dynamic);
    }

    #[test]
    fn a_string_enum_in_both_shapes() {
        let m = parse_defs(json!({
            "A": { "type": "string", "enum": ["open", "closed"] },
            "B": { "oneOf": [{ "type": "string", "const": "x", "description": "the x" }, { "type": "string", "const": "y" }] }
        }));
        let Item::Enum(a) = item(&m, "A") else { panic!("enum") };
        assert_eq!(a.values.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(), ["closed", "open"]);
        let Item::Enum(b) = item(&m, "B") else { panic!("enum") };
        assert_eq!(b.values[0], ("x".to_owned(), Some("the x".to_owned())));
    }

    #[test]
    fn an_internally_tagged_union_with_a_newtype_and_a_unit_variant() {
        let m = parse_defs(json!({
            "Create": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
            "Cmd": { "oneOf": [
                { "type": "object", "properties": { "type": { "type": "string", "const": "create" } }, "allOf": [{ "$ref": "#/$defs/Create" }], "required": ["type"] },
                { "type": "object", "properties": { "type": { "type": "string", "const": "ping" } }, "required": ["type"] },
                { "type": "object", "properties": { "type": { "const": "move" }, "to": { "type": "integer" } }, "required": ["type", "to"] }
            ] }
        }));
        let Item::Union(u) = item(&m, "Cmd") else { panic!("union") };
        assert_eq!(u.tagging, Tagging::Internal { tag: "type".into() });
        let body = |t: &str| u.variants.iter().find(|v| v.tag == t).unwrap().body.clone();
        assert_eq!(body("create"), Body::Payload(Ty::Ref("Create".into())));
        assert_eq!(body("ping"), Body::Unit);
        assert!(matches!(body("move"), Body::Fields(f) if f.len() == 1 && f[0].json == "to"));
    }

    #[test]
    fn an_adjacently_tagged_union() {
        let m = parse_defs(json!({
            "Params": { "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"] },
            "Method": { "oneOf": [
                { "type": "object", "properties": { "method": { "const": "x.get" } }, "required": ["method"] },
                { "type": "object", "properties": { "method": { "const": "x.set" }, "params": { "$ref": "#/$defs/Params" } }, "required": ["method", "params"] },
                { "type": "object", "properties": { "method": { "const": "x.list" }, "params": { "type": "object", "properties": { "p": { "type": ["string", "null"] } } } }, "required": ["method", "params"] }
            ] }
        }));
        let Item::Union(u) = item(&m, "Method") else { panic!("union") };
        assert_eq!(u.tagging, Tagging::Adjacent { tag: "method".into(), content: "params".into() });
        let body = |t: &str| u.variants.iter().find(|v| v.tag == t).unwrap().body.clone();
        assert_eq!(body("x.get"), Body::Unit);
        assert_eq!(body("x.set"), Body::Payload(Ty::Ref("Params".into())));
        assert!(matches!(body("x.list"), Body::Fields(f) if f.len() == 1));
    }

    #[test]
    fn an_externally_tagged_union() {
        let m = parse_defs(json!({
            "Outcome": { "oneOf": [
                { "type": "object", "properties": { "ok": { "type": "string" } }, "required": ["ok"], "additionalProperties": false },
                { "type": "object", "properties": { "error": { "type": "integer" } }, "required": ["error"], "additionalProperties": false }
            ] }
        }));
        let Item::Union(u) = item(&m, "Outcome") else { panic!("union") };
        assert_eq!(u.tagging, Tagging::External);
        assert_eq!(u.variants[0].tag, "error");
        assert_eq!(u.variants[1].body, Body::Payload(Ty::String));
    }

    #[test]
    fn a_flattened_enum_gives_optional_fields_and_maps_and_tuples_are_typed() {
        let m = parse_defs(json!({
            "Msg": { "oneOf": [
                { "type": "object", "properties": { "kind": { "const": "response" }, "request_id": { "type": "integer" } }, "required": ["kind", "request_id"],
                  "oneOf": [
                    { "type": "object", "properties": { "ok": { "type": "string" } }, "required": ["ok"] },
                    { "type": "object", "properties": { "error": { "type": "string" } }, "required": ["error"] }
                  ] },
                { "type": "object", "properties": { "kind": { "const": "ping" } }, "required": ["kind"] }
            ] },
            "Bag": { "type": "object", "properties": {
                "env": { "type": "object", "additionalProperties": { "type": "string" } },
                "pair": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "integer" }], "minItems": 2, "maxItems": 2 }
            }, "required": ["env", "pair"] }
        }));
        let Item::Union(u) = item(&m, "Msg") else { panic!("union") };
        assert_eq!(u.tagging, Tagging::Internal { tag: "kind".into() });
        let Body::Fields(f) = &u.variants.iter().find(|v| v.tag == "response").unwrap().body else { panic!("fields") };
        assert_eq!(f.iter().map(|f| f.json.as_str()).collect::<Vec<_>>(), ["error", "ok", "request_id"]);
        assert!(!f[0].required && f[2].required);
        let Item::Struct(bag) = item(&m, "Bag") else { panic!("struct") };
        assert_eq!(bag.fields[0].ty, Ty::Map(Box::new(Ty::String)));
        assert_eq!(bag.fields[1].ty, Ty::Tuple);
    }

    #[test]
    #[should_panic(expected = "#/$defs/Bad/properties/x")]
    fn an_inline_object_fails_loudly_with_its_path() {
        parse_defs(json!({ "Bad": { "type": "object", "properties": { "x": { "type": "object", "properties": { "y": { "type": "string" } } } } } }));
    }
}
