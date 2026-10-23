//! `headway commands`: the whole command tree as data, so an agent can learn the
//! CLI in one call instead of paging through `--help`.

use clap::{Arg, Command};
use serde_json::{Value, json};

/// The fully built command tree (clap fills in arities and defaults on build).
pub fn tree() -> Value {
    let mut command = <crate::args::Cli as clap::CommandFactory>::command();
    command.build();
    describe(&command)
}

fn describe(command: &Command) -> Value {
    let args: Vec<Value> = command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set() && !matches!(arg.get_id().as_str(), "help" | "version"))
        .filter(|arg| !arg.is_global_set() || command.get_name() == "headway")
        .map(describe_arg)
        .collect();
    let subcommands: Vec<Value> = command
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
        .map(describe)
        .collect();
    let mut value = json!({
        "name": command.get_name(),
        "about": command.get_about().map(ToString::to_string),
        "args": args,
    });
    if !subcommands.is_empty()
        && let Value::Object(map) = &mut value
    {
        map.insert("subcommands".to_owned(), Value::Array(subcommands));
    }
    value
}

fn describe_arg(arg: &Arg) -> Value {
    let possible: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_owned())
        .collect();
    let takes_value = arg.get_num_args().is_some_and(|range| range.takes_values());
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    let possible = (!possible.is_empty() && possible != ["true", "false"]).then_some(possible);
    json!({
        "name": arg.get_id().as_str(),
        "flag": arg.get_long().map(|long| format!("--{long}")),
        "short": arg.get_short().map(|short| format!("-{short}")),
        "positional": arg.is_positional(),
        "required": arg.is_required_set(),
        "takes_value": takes_value,
        "repeatable": takes_value && arg.get_num_args().is_some_and(|range| range.max_values() > 1),
        "possible_values": possible,
        "default": defaults.first(),
        "env": arg.get_env().map(|env| env.to_string_lossy().into_owned()),
        "help": arg.get_help().map(ToString::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_every_subcommand_and_flag() {
        let tree = tree();
        assert_eq!(tree["name"], "headway");
        let subcommands = tree["subcommands"].as_array().unwrap();
        let names: Vec<&str> = subcommands
            .iter()
            .map(|sub| sub["name"].as_str().unwrap())
            .collect();
        for expected in [
            "list", "check", "update", "add", "edit", "mark", "token", "commands",
        ] {
            assert!(names.contains(&expected), "{expected} missing");
        }
        let update = subcommands
            .iter()
            .find(|sub| sub["name"] == "update")
            .unwrap();
        let flags: Vec<&str> = update["args"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|arg| arg["flag"].as_str())
            .collect();
        assert!(
            flags.contains(&"--next") && flags.contains(&"--dry-run") && flags.contains(&"--to")
        );
        let output = tree["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|arg| arg["name"] == "output")
            .unwrap();
        assert_eq!(output["env"], "HEADWAY_OUTPUT");
        assert_eq!(output["takes_value"], true);
        let json = tree["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|arg| arg["name"] == "json")
            .unwrap();
        assert_eq!(json["takes_value"], false);
        let ids = update["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|arg| arg["name"] == "ids")
            .unwrap();
        assert_eq!(ids["repeatable"], true);
        assert_eq!(output["possible_values"], json!(["human", "json", "xml"]));
    }
}
