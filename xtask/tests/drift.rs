//! The Dart files in `clients/mobile/lib/data/protocol/` are checked in; they must be
//! exactly what the generator produces from the protocol's schemas now.

#[test]
fn the_checked_in_dart_matches_the_generator() {
    let mut problems = Vec::new();
    for (name, content) in xtask::generate() {
        match xtask::read_checked_in(&name) {
            None => problems.push(format!("{name} is missing")),
            Some(on_disk) if on_disk != content => problems.push(format!("{name} differs")),
            Some(_) => {}
        }
    }
    assert!(
        problems.is_empty(),
        "the generated Dart is out of date ({}): run `cargo xtask gen-dart` and commit the result",
        problems.join(", ")
    );
}
