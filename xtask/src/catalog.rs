//! The method catalogue: every wire method with the shape of its parameters,
//! read from the unions `Method` and `HostWrite` (adjacent `{method, params}`)
//! and `Command` (internal tag `type`, named on the wire by `COMMAND_METHODS`).

use crate::schema::{Body, Item, Model, Tagging, Union};
use aurelius_domain::COMMAND_METHODS;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Reads and session methods (`Method`).
    Read,
    /// Writes only the host can do (`HostWrite`).
    Host,
    /// Domain commands (`Command`).
    Command,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `<area>.<verb>`.
    pub method: String,
    pub group: Group,
    pub doc: Option<String>,
    pub body: Body,
}

fn union<'a>(model: &'a Model, name: &str) -> &'a Union {
    match model.items.get(name) {
        Some(Item::Union(u)) => u,
        other => panic!("{name} must be a tagged union in the schema, found {other:?}"),
    }
}

/// Sorted by method name; panics when a method name is used twice or the
/// command table and the `Command` schema disagree.
pub fn build(model: &Model) -> Vec<Entry> {
    build_with(model, COMMAND_METHODS)
}

pub fn build_with(model: &Model, commands: &[(&str, &str)]) -> Vec<Entry> {
    let mut out = Vec::new();
    for (root, group) in [("Method", Group::Read), ("HostWrite", Group::Host), ("Command", Group::Command)] {
        let u = union(model, root);
        match (&u.tagging, group) {
            (Tagging::Adjacent { tag, content }, Group::Read | Group::Host) => {
                assert!(tag == "method" && content == "params", "{root}: expected tag `method` and content `params`, found `{tag}`/`{content}`");
            }
            (Tagging::Internal { tag }, Group::Command) => assert!(tag == "type", "{root}: expected tag `type`, found `{tag}`"),
            (other, _) => panic!("{root}: unexpected tagging {other:?}"),
        }
        let mut seen = BTreeSet::new();
        for v in &u.variants {
            let method = if group == Group::Command {
                seen.insert(v.tag.clone());
                commands.iter().find(|(t, _)| *t == v.tag).unwrap_or_else(|| panic!("Command `{}` is not in COMMAND_METHODS", v.tag)).1.to_owned()
            } else {
                v.tag.clone()
            };
            out.push(Entry { method, group, doc: v.doc.clone(), body: v.body.clone() });
        }
        if group == Group::Command {
            for (t, _) in commands {
                assert!(seen.contains(*t), "COMMAND_METHODS lists `{t}`, which is not a variant of Command");
            }
        }
    }
    out.sort_by(|a, b| a.method.cmp(&b.method));
    for pair in out.windows(2) {
        assert!(pair[0].method != pair[1].method, "the method `{}` is defined twice", pair[0].method);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse;
    use serde_json::json;

    fn model() -> Model {
        let defs = json!({
            "P": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
            "Method": { "oneOf": [
                { "type": "object", "properties": { "method": { "const": "x.get" } }, "required": ["method"] },
                { "type": "object", "properties": { "method": { "const": "x.set" }, "params": { "$ref": "#/$defs/P" } }, "required": ["method", "params"] }
            ] },
            "HostWrite": { "oneOf": [{ "type": "object", "properties": { "method": { "const": "y.do" }, "params": { "$ref": "#/$defs/P" } }, "required": ["method", "params"] }] },
            "Command": { "oneOf": [{ "type": "object", "properties": { "type": { "const": "make_p" } }, "allOf": [{ "$ref": "#/$defs/P" }], "required": ["type"] }] }
        });
        parse(defs.as_object().unwrap())
    }

    #[test]
    fn the_catalogue_names_commands_through_the_table() {
        let entries = build_with(&model(), &[("make_p", "p.make")]);
        let names: Vec<&str> = entries.iter().map(|e| e.method.as_str()).collect();
        assert_eq!(names, ["p.make", "x.get", "x.set", "y.do"]);
        assert_eq!(entries[0].group, Group::Command);
    }

    #[test]
    #[should_panic(expected = "not in COMMAND_METHODS")]
    fn a_command_without_a_method_name_fails() {
        build_with(&model(), &[]);
    }
}
