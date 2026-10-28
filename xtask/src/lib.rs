//! `cargo xtask gen-dart`: Dart types for the mobile app, generated from the JSON
//! Schemas of the protocol crates (`schemars`). Pure: `generate` returns the files,
//! `main` writes or checks them, and `tests/drift.rs` fails when the checked-in
//! output differs.

pub mod catalog;
pub mod dart;
pub mod schema;

use schemars::JsonSchema;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Where the generated Dart files are checked in.
pub fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("clients").join("mobile").join("lib").join("data").join("protocol")
}

/// Adds the schema of `T` and every definition it refers to. The root itself is
/// stored under its schema name, so `$ref`s to it resolve like any other.
fn add_root<T: JsonSchema>(defs: &mut Map<String, Value>) {
    let name = T::schema_name().into_owned();
    let Value::Object(mut root) = serde_json::to_value(schemars::schema_for!(T)).expect("a schema is JSON") else {
        panic!("the schema of {name} is not an object");
    };
    root.remove("$schema");
    if let Some(Value::Object(nested)) = root.remove("$defs") {
        for (k, v) in nested {
            defs.insert(k, v);
        }
    }
    defs.insert(name, Value::Object(root));
}

/// Every definition reachable from the protocol's roots, by schema name.
pub fn schemas() -> Map<String, Value> {
    use aurelius_domain::{AppError, Command, ErrorCode, EventKind};
    use aurelius_protocol::{Event, HelloResult, HostResult, HostWrite, Method, Outcome, Receipt, ResponseBody, ServerMessage, WriteResult};
    let mut defs = Map::new();
    add_root::<Method>(&mut defs);
    add_root::<HostWrite>(&mut defs);
    add_root::<Command>(&mut defs);
    add_root::<ResponseBody>(&mut defs);
    add_root::<ServerMessage>(&mut defs);
    add_root::<Outcome>(&mut defs);
    add_root::<Receipt>(&mut defs);
    add_root::<WriteResult>(&mut defs);
    add_root::<HostResult>(&mut defs);
    add_root::<Event>(&mut defs);
    add_root::<EventKind>(&mut defs);
    add_root::<HelloResult>(&mut defs);
    add_root::<AppError>(&mut defs);
    add_root::<ErrorCode>(&mut defs);
    defs
}

/// File name to content of every generated file.
pub fn generate() -> BTreeMap<String, String> {
    let model = schema::parse(&schemas());
    let entries = catalog::build(&model);
    BTreeMap::from([
        ("models.dart".to_owned(), dart::models(&model)),
        ("methods.dart".to_owned(), dart::methods(&entries, aurelius_protocol::PROTOCOL_MAJOR)),
        ("protocol.dart".to_owned(), dart::barrel()),
    ])
}

/// The file on disk with Windows line endings (a git checkout) normalised.
pub fn read_checked_in(name: &str) -> Option<String> {
    std::fs::read_to_string(output_dir().join(name)).ok().map(|s| s.replace("\r\n", "\n"))
}

/// The files whose checked-in content differs from `generate()` (or is missing).
pub fn drifted() -> Vec<String> {
    generate().into_iter().filter(|(name, content)| read_checked_in(name).as_deref() != Some(content.as_str())).map(|(name, _)| name).collect()
}
