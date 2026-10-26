//! Rust-internal half of the IPC drift gate.
//!
//! Every name in `generate_handler!` must resolve to exactly one
//! `#[tauri::command]` function, and every command function must be registered:
//! a handler/function mismatch fails `cargo test -p headway-desktop` even when
//! nobody touches the frontend. The TypeScript↔Rust half is
//! `scripts/check-ipc-contract.mjs`.

use std::collections::{BTreeMap, BTreeSet};

const LIB: &str = include_str!("../lib.rs");
const COMMAND_FILES: [(&str, &str); 7] = [
    ("apps", include_str!("../commands/apps.rs")),
    ("management", include_str!("../commands/management.rs")),
    ("ops", include_str!("../commands/ops.rs")),
    ("revisions", include_str!("../commands/revisions.rs")),
    ("settings", include_str!("../commands/settings.rs")),
    ("system", include_str!("../commands/system.rs")),
    ("self_update", include_str!("../self_update.rs")),
];

/// Names registered in `generate_handler![…]`.
fn handlers() -> Vec<String> {
    let start = LIB
        .find("generate_handler![")
        .expect("handler macro in lib.rs");
    let body = &LIB[start..];
    let end = body.find(']').expect("handler list terminator");
    body[..end]
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(',');
            // Only `module::name` entries; the macro line itself has no `::`.
            let (_, name) = line.rsplit_once("::")?;
            let name = name.trim();
            (!name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .then(|| name.to_owned())
        })
        .collect()
}

/// `#[tauri::command]` functions and the module that defines them.
fn commands() -> BTreeMap<String, &'static str> {
    let mut found = BTreeMap::new();
    for (module, source) in COMMAND_FILES {
        let mut next_is_command = false;
        for line in source.lines() {
            let line = line.trim();
            if line == "#[tauri::command]" {
                next_is_command = true;
                continue;
            }
            if !next_is_command {
                continue;
            }
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let Some((_, signature)) = line.split_once("fn ") else {
                next_is_command = false;
                continue;
            };
            let name: String = signature
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            found.insert(name, module);
            next_is_command = false;
        }
    }
    found
}

#[test]
fn every_registered_handler_resolves_to_one_command_function() {
    let registered = handlers();
    let unique: BTreeSet<_> = registered.iter().cloned().collect();
    assert_eq!(
        unique.len(),
        registered.len(),
        "duplicate handler entries: {registered:?}"
    );
    let functions = commands();
    for name in &registered {
        assert!(
            functions.contains_key(name),
            "`{name}` is registered but has no #[tauri::command] function"
        );
    }
}

#[test]
fn every_command_function_is_registered() {
    let registered: BTreeSet<_> = handlers().into_iter().collect();
    for (name, module) in commands() {
        assert!(
            registered.contains(&name),
            "`{module}::{name}` is a #[tauri::command] but is missing from generate_handler!"
        );
    }
}

#[test]
fn adding_a_command_needs_a_contract_case() {
    // The handler list is the public window surface. A new command must come with
    // an `ipc_tests` case (AGENTS parity rule) and with a frontend binding, so
    // changing this number is a deliberate review action.
    assert_eq!(
        handlers().len(),
        56,
        "a command was added or removed without updating the IPC contract tests"
    );
}
