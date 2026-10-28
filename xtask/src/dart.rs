//! Emits Dart from the schema model: `models.dart` (structs, enums, sealed unions),
//! `methods.dart` (method names and request builders) and the barrel `protocol.dart`.
//!
//! Output is deterministic: items, fields, variants and methods are sorted, lines end in `\n`.

use crate::catalog::{Entry, Group};
use crate::schema::{Body, Enum, Field, Item, Model, Struct, Tagging, Ty, Union, Variant};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

const GENERATED: &str = "// GENERATED CODE - DO NOT EDIT.\n// Run: cargo xtask gen-dart\n";
const IGNORES: &str = "// ignore_for_file: unnecessary_cast, unnecessary_null_aware_operator, unnecessary_non_null_assertion, constant_identifier_names, use_null_aware_elements, unintended_html_in_doc_comment\n";

macro_rules! w {
    ($out:expr) => { $out.push('\n') };
    ($out:expr, $($arg:tt)*) => { writeln!($out, $($arg)*).expect("writing to a String") };
}

const RESERVED: &[&str] = &[
    "assert", "await", "break", "case", "catch", "class", "const", "continue", "default", "do", "else", "enum", "extends", "false", "final", "finally", "for", "if", "in", "is", "new",
    "null", "rethrow", "return", "super", "switch", "this", "throw", "true", "try", "var", "void", "while", "with", "yield",
    // members every class has, or that these classes define
    "toJson", "fromJson", "hashCode", "runtimeType", "toString", "noSuchMethod",
];

/// Members of every Dart enum, plus the `wire` field of the generated ones.
const ENUM_MEMBERS: &[&str] = &["values", "index", "name", "wire"];

/// Names that would hide a Dart core type.
const CORE_TYPES: &[&str] = &["Map", "List", "String", "Object", "Type", "Function", "Future", "Stream", "Set", "Duration", "DateTime", "Uri", "Error", "Exception", "Iterable", "Null", "Symbol", "Record", "Enum"];

#[derive(Clone, Copy, PartialEq, Debug)]
enum K {
    Class,
    Enum,
    Tagged,
    External,
}

struct Kinds(BTreeMap<String, K>);

impl Kinds {
    fn of(model: &Model) -> Self {
        Kinds(
            model
                .items
                .iter()
                .map(|(n, i)| {
                    let k = match i {
                        Item::Struct(_) => K::Class,
                        Item::Enum(_) => K::Enum,
                        Item::Union(u) if u.tagging == Tagging::External => K::External,
                        Item::Union(_) => K::Tagged,
                    };
                    (n.clone(), k)
                })
                .collect(),
        )
    }

    fn get(&self, name: &str) -> K {
        *self.0.get(name).unwrap_or_else(|| panic!("the schema refers to the unknown type {name}"))
    }
}

// ---- names ----

pub fn pascal(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            let first = c.next().expect("non-empty").to_ascii_uppercase();
            format!("{first}{}", c.as_str())
        })
        .collect()
}

pub fn camel(s: &str) -> String {
    let p = pascal(s);
    let mut c = p.chars();
    match c.next() {
        Some(f) => format!("{}{}", f.to_ascii_lowercase(), c.as_str()),
        None => panic!("`{s}` has no letters or digits to make a Dart name from"),
    }
}

fn ident(s: &str) -> String {
    let mut s = s.to_owned();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s = format!("v{s}");
    }
    if RESERVED.contains(&s.as_str()) {
        s.push('_');
    }
    s
}

fn dart_str(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn doc(out: &mut String, indent: &str, text: Option<&str>) {
    let Some(text) = text else { return };
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            w!(out, "{indent}///");
        } else {
            w!(out, "{indent}/// {line}");
        }
    }
}

// ---- types and (de)serialisation expressions ----

fn dart_type(t: &Ty) -> String {
    match t {
        Ty::String => "String".into(),
        Ty::Int => "int".into(),
        Ty::Double => "double".into(),
        Ty::Bool => "bool".into(),
        Ty::Dynamic => "dynamic".into(),
        Ty::Ref(n) => n.clone(),
        Ty::List(i) => format!("List<{}>", dart_type(i)),
        Ty::Map(i) => format!("Map<String, {}>", dart_type(i)),
        Ty::Nullable(i) if **i == Ty::Dynamic => "dynamic".into(),
        Ty::Nullable(i) => format!("{}?", dart_type(i)),
        Ty::Tuple => "List<dynamic>".into(),
    }
}

/// Dart code that turns the JSON value `e` (a simple expression) into `t`.
fn from_expr(k: &Kinds, t: &Ty, e: &str, d: usize) -> String {
    match t {
        Ty::String => format!("({e} as String)"),
        Ty::Int => format!("({e} as num).toInt()"),
        Ty::Double => format!("({e} as num).toDouble()"),
        Ty::Bool => format!("({e} as bool)"),
        Ty::Dynamic => e.to_owned(),
        Ty::Tuple => format!("({e} as List<dynamic>)"),
        Ty::Ref(n) => match k.get(n) {
            K::Enum => format!("{n}.fromJson({e} as String)"),
            K::Class | K::Tagged => format!("{n}.fromJson({e} as Map<String, dynamic>)"),
            K::External => format!("{n}.fromJson({e})"),
        },
        Ty::List(i) => format!("({e} as List<dynamic>).map((x{d}) => {}).toList()", from_expr(k, i, &format!("x{d}"), d + 1)),
        Ty::Map(i) if **i == Ty::Dynamic => format!("({e} as Map<String, dynamic>)"),
        Ty::Map(i) => format!("({e} as Map<String, dynamic>).map((k{d}, v{d}) => MapEntry(k{d}, {}))", from_expr(k, i, &format!("v{d}"), d + 1)),
        Ty::Nullable(i) if **i == Ty::Dynamic => e.to_owned(),
        Ty::Nullable(i) => format!("({e} == null ? null : {})", from_expr(k, i, e, d)),
    }
}

/// Values that are already JSON.
fn plain(t: &Ty) -> bool {
    match t {
        Ty::String | Ty::Int | Ty::Double | Ty::Bool | Ty::Dynamic | Ty::Tuple => true,
        Ty::List(i) | Ty::Map(i) | Ty::Nullable(i) => plain(i),
        Ty::Ref(_) => false,
    }
}

/// Dart code that turns the value `e` of type `t` into JSON; `safe` when `e` may be null.
fn to_expr(t: &Ty, e: &str, safe: bool, d: usize) -> String {
    if plain(t) {
        return e.to_owned();
    }
    let op = if safe { "?." } else { "." };
    match t {
        Ty::Ref(_) => format!("{e}{op}toJson()"),
        Ty::List(i) => format!("{e}{op}map((x{d}) => {}).toList()", to_expr(i, &format!("x{d}"), false, d + 1)),
        Ty::Map(i) => format!("{e}{op}map((k{d}, v{d}) => MapEntry(k{d}, {}))", to_expr(i, &format!("v{d}"), false, d + 1)),
        Ty::Nullable(i) => to_expr(i, e, true, d),
        _ => unreachable!("plain types returned above"),
    }
}

// ---- fields ----

enum Mode {
    Required,
    /// Nullable: an optional parameter, left out of the JSON when null.
    Optional,
    Dynamic { required: bool },
    /// Not required in the schema but not nullable: it has a Dart default.
    Default(String),
}

struct Plan {
    name: String,
    json: String,
    ty: Ty,
    dart_ty: String,
    mode: Mode,
    doc: Option<String>,
}

fn default_code(t: &Ty, default: &Option<serde_json::Value>) -> Option<String> {
    match (t, default) {
        (Ty::List(_), _) => Some("const []".into()),
        (Ty::Map(_), _) => Some("const {}".into()),
        (Ty::Bool, Some(serde_json::Value::Bool(b))) => Some(b.to_string()),
        (Ty::Int, Some(serde_json::Value::Number(n))) if n.is_i64() || n.is_u64() => Some(n.to_string()),
        (Ty::Double, Some(serde_json::Value::Number(n))) => n.as_f64().map(|f| format!("{f:?}")),
        (Ty::String, Some(serde_json::Value::String(s))) => Some(dart_str(s)),
        _ => None,
    }
}

fn plan(f: &Field) -> Plan {
    let (ty, mode) = match &f.ty {
        Ty::Nullable(_) => (f.ty.clone(), Mode::Optional),
        Ty::Dynamic => (Ty::Dynamic, Mode::Dynamic { required: f.required }),
        t if f.required => (t.clone(), Mode::Required),
        t => match default_code(t, &f.default) {
            Some(code) => (t.clone(), Mode::Default(code)),
            None => (Ty::Nullable(Box::new(t.clone())), Mode::Optional),
        },
    };
    Plan { name: ident(&camel(&f.json)), json: f.json.clone(), dart_ty: dart_type(&ty), ty, mode, doc: f.doc.clone() }
}

fn plans(fields: &[Field], owner: &str) -> Vec<Plan> {
    let plans: Vec<Plan> = fields.iter().map(plan).collect();
    let mut seen = BTreeSet::new();
    for p in &plans {
        assert!(seen.insert(p.name.clone()), "{owner}: two fields become the Dart name `{}`", p.name);
    }
    plans
}

impl Plan {
    /// A parameter of a constructor (`this.x`) or, with `typed`, of a function.
    fn param(&self, typed: bool) -> String {
        let target = if typed { format!("{} {}", self.dart_ty, self.name) } else { format!("this.{}", self.name) };
        match &self.mode {
            Mode::Required | Mode::Dynamic { required: true } => format!("required {target}"),
            Mode::Optional | Mode::Dynamic { required: false } => target,
            Mode::Default(code) => format!("{target} = {code}"),
        }
    }

    fn read(&self, k: &Kinds, src: &str) -> String {
        let key = format!("{src}['{}']", self.json);
        match &self.mode {
            Mode::Required | Mode::Optional => from_expr(k, &self.ty, &key, 0),
            Mode::Dynamic { .. } => key,
            Mode::Default(code) => format!("({key} == null ? {code} : {})", from_expr(k, &self.ty, &key, 0)),
        }
    }

    fn entry(&self) -> String {
        let key = dart_str(&self.json);
        let value = to_expr(&self.ty, &self.name, matches!(self.mode, Mode::Optional), 0);
        match &self.mode {
            Mode::Required | Mode::Default(_) | Mode::Dynamic { required: true } => format!("{key}: {value}"),
            Mode::Optional | Mode::Dynamic { required: false } => format!("if ({} != null) {key}: {value}", self.name),
        }
    }
}

/// `<String, dynamic>{ a, b }` with one entry per line.
fn map_lit(entries: &[String], indent: &str) -> String {
    if entries.is_empty() {
        return "<String, dynamic>{}".into();
    }
    let mut s = String::from("<String, dynamic>{\n");
    for e in entries {
        s.push_str(&format!("{indent}  {e},\n"));
    }
    s.push_str(indent);
    s.push('}');
    s
}

fn ctor(out: &mut String, name: &str, params: &[String]) {
    if params.is_empty() {
        w!(out, "  const {name}();");
    } else {
        w!(out, "  const {name}({{");
        for p in params {
            w!(out, "    {p},");
        }
        w!(out, "  }});");
    }
}

fn field_decls(out: &mut String, plans: &[Plan]) {
    for p in plans {
        doc(out, "  ", p.doc.as_deref());
        w!(out, "  final {} {};", p.dart_ty, p.name);
    }
}

fn args(k: &Kinds, plans: &[Plan], src: &str, indent: &str) -> String {
    plans.iter().map(|p| format!("\n{indent}  {}: {},", p.name, p.read(k, src))).collect::<String>() + &format!("\n{indent}")
}

// ---- items ----

fn emit_struct(out: &mut String, k: &Kinds, s: &Struct) {
    let plans = plans(&s.fields, &s.name);
    doc(out, "", s.doc.as_deref());
    w!(out, "final class {} {{", s.name);
    ctor(out, &s.name, &plans.iter().map(|p| p.param(false)).collect::<Vec<_>>());
    if !plans.is_empty() {
        w!(out);
        field_decls(out, &plans);
    }
    w!(out);
    if plans.is_empty() {
        w!(out, "  factory {}.fromJson(Map<String, dynamic> json) => const {}();", s.name, s.name);
    } else {
        w!(out, "  factory {}.fromJson(Map<String, dynamic> json) => {}({});", s.name, s.name, args(k, &plans, "json", "  "));
    }
    w!(out);
    let entries: Vec<String> = plans.iter().map(Plan::entry).collect();
    w!(out, "  Map<String, dynamic> toJson() => {};", map_lit(&entries, "  "));
    w!(out, "}}");
}

fn emit_enum(out: &mut String, e: &Enum) {
    assert!(!e.values.is_empty(), "{}: an enum without values", e.name);
    let mut seen = BTreeSet::new();
    doc(out, "", e.doc.as_deref());
    w!(out, "enum {} {{", e.name);
    for (i, (wire, d)) in e.values.iter().enumerate() {
        let mut name = ident(&camel(wire));
        if ENUM_MEMBERS.contains(&name.as_str()) {
            name.push('_');
        }
        assert!(seen.insert(name.clone()), "{}: the values collide as the Dart name `{name}`", e.name);
        doc(out, "  ", d.as_deref());
        let end = if i + 1 == e.values.len() { ";" } else { "," };
        w!(out, "  {name}({}){end}", dart_str(wire));
    }
    w!(out);
    w!(out, "  const {}(this.wire);", e.name);
    w!(out);
    w!(out, "  final String wire;");
    w!(out);
    w!(out, "  static {} fromJson(String value) => values.firstWhere(", e.name);
    w!(out, "        (e) => e.wire == value,");
    w!(out, "        orElse: () => throw FormatException('Unknown {} value: $value'),", e.name);
    w!(out, "      );");
    w!(out);
    w!(out, "  String toJson() => wire;");
    w!(out, "}}");
}

/// How a variant is built from `json`: code before the `return`, and the expression.
fn variant_build(k: &Kinds, u: &Union, v: &Variant, class: &str) -> (Option<String>, String) {
    match &v.body {
        Body::Unit => (None, format!("const {class}()")),
        Body::Fields(f) => {
            let plans = plans(f, class);
            if plans.is_empty() {
                return (None, format!("const {class}()"));
            }
            let src = match &u.tagging {
                Tagging::Internal { .. } => return (None, format!("{class}({})", args(k, &plans, "json", "        "))),
                Tagging::Adjacent { content, .. } => format!("(json['{content}'] as Map<String, dynamic>?) ?? const <String, dynamic>{{}}"),
                Tagging::External => format!("json['{}'] as Map<String, dynamic>", v.tag),
            };
            (Some(format!("final src = {src};")), format!("{class}({})", args(k, &plans, "src", "        ")))
        }
        Body::Payload(ty) => {
            let source = match &u.tagging {
                Tagging::Internal { .. } => "json".to_owned(),
                Tagging::Adjacent { content, .. } => format!("json['{content}']"),
                Tagging::External => format!("json['{}']", v.tag),
            };
            (None, format!("{class}({}: {})", payload_name(u), from_expr(k, ty, &source, 0)))
        }
    }
}

fn payload_name(u: &Union) -> String {
    match &u.tagging {
        Tagging::Adjacent { content, .. } => ident(&camel(content)),
        _ => "value".into(),
    }
}

fn variant_to_json(k: &Kinds, u: &Union, v: &Variant, class: &str) -> String {
    let tag = dart_str(&v.tag);
    match (&u.tagging, &v.body) {
        (Tagging::External, Body::Unit) => tag,
        (Tagging::External, Body::Fields(f)) => {
            let entries: Vec<String> = plans(f, class).iter().map(Plan::entry).collect();
            map_lit(&[format!("{tag}: {}", map_lit(&entries, "    "))], "    ")
        }
        (Tagging::External, Body::Payload(ty)) => map_lit(&[format!("{tag}: {}", to_expr(ty, "value", matches!(ty, Ty::Nullable(_)), 0))], "    "),
        (Tagging::Internal { tag: key }, Body::Unit) => map_lit(&[format!("{}: {tag}", dart_str(key))], "    "),
        (Tagging::Internal { tag: key }, Body::Fields(f)) => {
            let mut entries = vec![format!("{}: {tag}", dart_str(key))];
            entries.extend(plans(f, class).iter().map(Plan::entry));
            map_lit(&entries, "    ")
        }
        (Tagging::Internal { tag: key }, Body::Payload(ty)) => {
            let spread = match ty {
                Ty::Ref(n) if matches!(k.get(n), K::Class | K::Tagged) => "...value.toJson()",
                other => panic!("{class}: an internally tagged variant must hold a struct, found {other:?}"),
            };
            map_lit(&[format!("{}: {tag}", dart_str(key)), spread.to_owned()], "    ")
        }
        (Tagging::Adjacent { tag: key, .. }, Body::Unit) => map_lit(&[format!("{}: {tag}", dart_str(key))], "    "),
        (Tagging::Adjacent { tag: key, content }, Body::Fields(f)) => {
            let entries: Vec<String> = plans(f, class).iter().map(Plan::entry).collect();
            map_lit(&[format!("{}: {tag}", dart_str(key)), format!("{}: {}", dart_str(content), map_lit(&entries, "    "))], "    ")
        }
        (Tagging::Adjacent { tag: key, content }, Body::Payload(ty)) => {
            let name = payload_name(u);
            let content = dart_str(content);
            let entry = match ty {
                Ty::Nullable(_) => format!("if ({name} != null) {content}: {}", to_expr(ty, &name, true, 0)),
                _ => format!("{content}: {}", to_expr(ty, &name, false, 0)),
            };
            map_lit(&[format!("{}: {tag}", dart_str(key)), entry], "    ")
        }
    }
}

fn emit_union(out: &mut String, k: &Kinds, u: &Union, used: &mut BTreeSet<String>) {
    assert!(!u.variants.is_empty(), "{}: a union without variants", u.name);
    let external = u.tagging == Tagging::External;
    let classes: Vec<String> = u
        .variants
        .iter()
        .map(|v| {
            let class = format!("{}{}", u.name, pascal(&v.tag));
            assert!(used.insert(class.clone()), "the variant class name {class} is already taken");
            class
        })
        .collect();

    doc(out, "", u.doc.as_deref());
    w!(out, "sealed class {} {{", u.name);
    w!(out, "  const {}();", u.name);
    w!(out);

    // fromJson
    if external {
        w!(out, "  factory {}.fromJson(Object? json) {{", u.name);
        let units: Vec<usize> = (0..u.variants.len()).filter(|i| u.variants[*i].body == Body::Unit).collect();
        if !units.is_empty() {
            w!(out, "    if (json is String) {{");
            w!(out, "      switch (json) {{");
            for i in units {
                w!(out, "        case {}:", dart_str(&u.variants[i].tag));
                w!(out, "          return const {}();", classes[i]);
            }
            w!(out, "      }}");
            w!(out, "    }}");
        }
        for (v, class) in u.variants.iter().zip(&classes) {
            if v.body == Body::Unit {
                continue;
            }
            let (prelude, expr) = variant_build(k, u, v, class);
            w!(out, "    if (json is Map<String, dynamic> && json.containsKey({})) {{", dart_str(&v.tag));
            if let Some(p) = prelude {
                w!(out, "      {p}");
            }
            w!(out, "      return {expr};");
            w!(out, "    }}");
        }
        w!(out, "    throw FormatException('Unknown {}: $json');", u.name);
        w!(out, "  }}");
    } else {
        let key = match &u.tagging {
            Tagging::Internal { tag } | Tagging::Adjacent { tag, .. } => tag.clone(),
            Tagging::External => unreachable!(),
        };
        w!(out, "  factory {}.fromJson(Map<String, dynamic> json) {{", u.name);
        w!(out, "    final tag = json[{}] as String;", dart_str(&key));
        w!(out, "    switch (tag) {{");
        for (v, class) in u.variants.iter().zip(&classes) {
            let (prelude, expr) = variant_build(k, u, v, class);
            w!(out, "      case {}:", dart_str(&v.tag));
            match prelude {
                Some(p) => {
                    w!(out, "        {{");
                    w!(out, "          {p}");
                    w!(out, "          return {expr};");
                    w!(out, "        }}");
                }
                None => w!(out, "        return {expr};"),
            }
        }
        w!(out, "      default:");
        w!(out, "        throw FormatException('Unknown {} {key}: $tag');", u.name);
        w!(out, "    }}");
        w!(out, "  }}");
    }
    w!(out);
    w!(out, "  {} toJson();", if external { "Object" } else { "Map<String, dynamic>" });
    w!(out, "}}");

    for (v, class) in u.variants.iter().zip(&classes) {
        w!(out);
        doc(out, "", v.doc.as_deref());
        w!(out, "final class {class} extends {} {{", u.name);
        match &v.body {
            Body::Unit => ctor(out, class, &[]),
            Body::Fields(f) => {
                let plans = plans(f, class);
                ctor(out, class, &plans.iter().map(|p| p.param(false)).collect::<Vec<_>>());
                if !plans.is_empty() {
                    w!(out);
                    field_decls(out, &plans);
                }
            }
            Body::Payload(ty) => {
                let name = payload_name(u);
                let param = if matches!(ty, Ty::Nullable(_)) { format!("this.{name}") } else { format!("required this.{name}") };
                ctor(out, class, &[param]);
                w!(out);
                w!(out, "  final {} {name};", dart_type(ty));
            }
        }
        w!(out);
        w!(out, "  @override");
        w!(out, "  {} toJson() => {};", if external { "Object" } else { "Map<String, dynamic>" }, variant_to_json(k, u, v, class));
        w!(out, "}}");
    }
}

/// `models.dart`: every struct, enum and union of the schema.
pub fn models(model: &Model) -> String {
    let k = Kinds::of(model);
    for name in model.items.keys() {
        assert!(!CORE_TYPES.contains(&name.as_str()), "the type {name} would hide a Dart core type; rename it in Rust");
    }
    let mut used: BTreeSet<String> = model.items.keys().cloned().collect();
    let mut out = String::new();
    out.push_str(GENERATED);
    out.push_str(IGNORES);
    for item in model.items.values() {
        w!(out);
        match item {
            Item::Struct(s) => emit_struct(&mut out, &k, s),
            Item::Enum(e) => emit_enum(&mut out, e),
            Item::Union(u) => emit_union(&mut out, &k, u, &mut used),
        }
    }
    out
}

/// `methods.dart`: the protocol major, the method names and one request builder per method.
/// A builder returns the `method`/`params` part of the envelope; the app adds
/// `protocol`, `request_id`, `idempotency_key` and `expected_version`.
pub fn methods(entries: &[Entry], major: u16) -> String {
    let mut out = String::new();
    out.push_str(GENERATED);
    out.push_str(IGNORES);
    w!(out);
    w!(out, "import 'models.dart';");
    w!(out);
    w!(out, "/// The protocol major this app speaks. The server refuses another one.");
    w!(out, "const int protocolMajor = {major};");
    w!(out);
    w!(out, "/// The wire name of every method.");
    w!(out, "abstract final class Methods {{");
    let mut names = BTreeSet::new();
    for e in entries {
        let name = ident(&camel(&e.method));
        assert!(names.insert(name.clone()), "the methods collide as the Dart name `{name}`");
        doc(&mut out, "  ", e.doc.as_deref());
        w!(out, "  static const String {name} = {};", dart_str(&e.method));
    }
    w!(out, "}}");
    for (group, title) in [(Group::Read, "Reads and session methods."), (Group::Host, "Writes only the host can do."), (Group::Command, "Domain commands.")] {
        w!(out);
        w!(out, "// {title}");
        for e in entries.iter().filter(|e| e.group == group) {
            w!(out);
            doc(&mut out, "", e.doc.as_deref());
            let name = ident(&camel(&e.method));
            let method_entry = format!("'method': Methods.{name}");
            match &e.body {
                Body::Unit => {
                    w!(out, "Map<String, dynamic> {name}() => {};", map_lit(&[method_entry], ""));
                }
                Body::Payload(ty) => {
                    let (param, entry) = match ty {
                        Ty::Nullable(_) => (format!("{} params", dart_type(ty)), format!("if (params != null) 'params': {}", to_expr(ty, "params", true, 0))),
                        _ => (format!("{} params", dart_type(ty)), format!("'params': {}", to_expr(ty, "params", false, 0))),
                    };
                    w!(out, "Map<String, dynamic> {name}({param}) => {};", map_lit(&[method_entry, entry], ""));
                }
                Body::Fields(f) => {
                    let plans = plans(f, &e.method);
                    let entries: Vec<String> = plans.iter().map(Plan::entry).collect();
                    let params = if plans.is_empty() {
                        String::new()
                    } else {
                        format!("{{{}}}", plans.iter().map(|p| p.param(true)).collect::<Vec<_>>().join(", "))
                    };
                    w!(out, "Map<String, dynamic> {name}({params}) => {};", map_lit(&[method_entry, format!("'params': {}", map_lit(&entries, "  "))], ""));
                }
            }
        }
    }
    out
}

/// `protocol.dart`: the one import for the rest of the app.
pub fn barrel() -> String {
    format!("{GENERATED}\nexport 'methods.dart';\nexport 'models.dart';\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse;
    use serde_json::json;

    fn model(defs: serde_json::Value) -> Model {
        parse(defs.as_object().unwrap())
    }

    #[test]
    fn a_struct_with_an_optional_field() {
        let dart = models(&model(json!({
            "Project": { "type": "object", "description": "A project.", "properties": {
                "name": { "type": "string" },
                "folder": { "type": ["string", "null"] },
                "tags": { "type": "array", "items": { "type": "string" } },
                "max_runs": { "type": "integer", "default": 3 }
            }, "required": ["name"] }
        })));
        assert!(dart.starts_with("// GENERATED CODE - DO NOT EDIT.\n// Run: cargo xtask gen-dart\n"));
        assert!(dart.contains("/// A project.\nfinal class Project {"));
        assert!(dart.contains("    required this.name,\n"));
        assert!(dart.contains("    this.folder,\n"));
        assert!(dart.contains("    this.maxRuns = 3,\n"));
        assert!(dart.contains("    this.tags = const [],\n"));
        assert!(dart.contains("  final String? folder;"));
        assert!(dart.contains("  final List<String> tags;"));
        assert!(dart.contains("name: (json['name'] as String),"));
        assert!(dart.contains("folder: (json['folder'] == null ? null : (json['folder'] as String)),"));
        assert!(dart.contains("'name': name,"));
        assert!(dart.contains("if (folder != null) 'folder': folder,"));
        assert!(dart.contains("'max_runs': maxRuns,"));
        assert!(!dart.contains('\r'));
    }

    #[test]
    fn a_string_enum() {
        let dart = models(&model(json!({ "Status": { "type": "string", "enum": ["open", "task.closed"] } })));
        assert!(dart.contains("enum Status {\n  open('open'),\n  taskClosed('task.closed');\n"));
        assert!(dart.contains("String toJson() => wire;"));
        assert!(dart.contains("static Status fromJson(String value)"));
    }

    #[test]
    fn an_internally_tagged_union() {
        let dart = models(&model(json!({
            "Create": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
            "Cmd": { "oneOf": [
                { "type": "object", "properties": { "type": { "const": "create" } }, "allOf": [{ "$ref": "#/$defs/Create" }], "required": ["type"] },
                { "type": "object", "properties": { "type": { "const": "ping" } }, "required": ["type"] }
            ] }
        })));
        assert!(dart.contains("sealed class Cmd {"));
        assert!(dart.contains("final tag = json['type'] as String;"));
        assert!(dart.contains("return CmdCreate(value: Create.fromJson(json as Map<String, dynamic>));"));
        assert!(dart.contains("return const CmdPing();"));
        assert!(dart.contains("final class CmdCreate extends Cmd {"));
        assert!(dart.contains("...value.toJson(),"));
        assert!(dart.contains("'type': 'ping',"));
    }

    #[test]
    fn an_adjacently_tagged_union_and_its_builders() {
        let m = model(json!({
            "Params": { "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"] },
            "Method": { "oneOf": [
                { "type": "object", "properties": { "method": { "const": "x.get" } }, "required": ["method"] },
                { "type": "object", "properties": { "method": { "const": "x.set" }, "params": { "$ref": "#/$defs/Params" } }, "required": ["method", "params"] },
                { "type": "object", "properties": { "method": { "const": "x.list" }, "params": { "type": "object", "properties": { "project_id": { "type": ["string", "null"] }, "n": { "type": "integer" } }, "required": ["n"] } }, "required": ["method", "params"] }
            ] },
            "HostWrite": { "oneOf": [
                { "type": "object", "properties": { "method": { "const": "h.do" } }, "required": ["method"] },
                { "type": "object", "properties": { "method": { "const": "h.set" }, "params": { "$ref": "#/$defs/Params" } }, "required": ["method", "params"] }
            ] },
            "Command": { "oneOf": [
                { "type": "object", "properties": { "type": { "const": "make" } }, "allOf": [{ "$ref": "#/$defs/Params" }], "required": ["type"] }
            ] }
        }));
        let dart = models(&m);
        assert!(dart.contains("return MethodXSet(params: Params.fromJson(json['params'] as Map<String, dynamic>));"));
        assert!(dart.contains("final src = (json['params'] as Map<String, dynamic>?) ?? const <String, dynamic>{};"));
        assert!(dart.contains("final Params params;"));
        assert!(dart.contains("'method': 'x.set',"));
        let entries = crate::catalog::build_with(&m, &[("make", "thing.make")]);
        let methods = methods(&entries, 2);
        assert!(methods.contains("const int protocolMajor = 2;"));
        assert!(methods.contains("static const String xSet = 'x.set';"));
        assert!(methods.contains("Map<String, dynamic> xGet() => <String, dynamic>{\n  'method': Methods.xGet,\n};"));
        assert!(methods.contains("Map<String, dynamic> xSet(Params params) => "));
        assert!(methods.contains("'params': params.toJson(),"));
        assert!(methods.contains("Map<String, dynamic> xList({required int n, String? projectId}) => "));
        assert!(methods.contains("Map<String, dynamic> thingMake(Params params) => "));
    }

    #[test]
    fn maps_tuples_and_dynamic_values() {
        let dart = models(&model(json!({
            "Bag": { "type": "object", "properties": {
                "env": { "type": "object", "additionalProperties": { "type": "string" } },
                "pair": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "integer" }] },
                "raw": true,
                "ratio": { "type": "number", "format": "float" }
            }, "required": ["env", "pair", "ratio"] }
        })));
        assert!(dart.contains("  final Map<String, String> env;"));
        assert!(dart.contains("  final List<dynamic> pair;"));
        assert!(dart.contains("  final dynamic raw;"));
        assert!(dart.contains("ratio: (json['ratio'] as num).toDouble(),"));
        assert!(dart.contains("env: (json['env'] as Map<String, dynamic>).map((k0, v0) => MapEntry(k0, (v0 as String))),"));
    }

    #[test]
    fn an_externally_tagged_union() {
        let dart = models(&model(json!({
            "Outcome": { "oneOf": [
                { "type": "object", "properties": { "ok": { "type": "string" } }, "required": ["ok"], "additionalProperties": false },
                { "type": "object", "properties": { "error": { "type": "integer" } }, "required": ["error"], "additionalProperties": false }
            ] }
        })));
        assert!(dart.contains("factory Outcome.fromJson(Object? json) {"));
        assert!(dart.contains("json.containsKey('ok')"));
        assert!(dart.contains("return OutcomeOk(value: (json['ok'] as String));"));
        assert!(dart.contains("  Object toJson();"));
    }
}
