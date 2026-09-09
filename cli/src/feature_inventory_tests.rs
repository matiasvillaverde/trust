//! Keeps the acceptance checklist aligned with the real CLI, not a second command tree.
use std::collections::BTreeSet;

fn leaf_commands(command: &clap::Command, prefix: &str, leaves: &mut BTreeSet<String>) {
    for child in command.get_subcommands() {
        let path = if prefix.is_empty() {
            child.get_name().to_owned()
        } else {
            format!("{prefix} {}", child.get_name())
        };
        if child.get_subcommands().next().is_none() {
            assert!(leaves.insert(path));
        } else {
            leaf_commands(child, &path, leaves);
        }
    }
}

#[test]
fn every_cli_leaf_has_an_acceptance_scenario() {
    let mut actual = BTreeSet::new();
    leaf_commands(&super::build_cli(), "", &mut actual);
    let mut documented = BTreeSet::new();
    for line in include_str!("../../docs/ibkr-feature-scenarios.md").lines() {
        if let Some(row) = line.strip_prefix("| `") {
            let (name, remainder) = row.split_once("` | ").expect("scenario row syntax");
            let (lane, scenario) = remainder.split_once(" | ").expect("scenario lane");
            assert!(["L", "I", "P", "M"].contains(&lane), "{name}: invalid lane");
            assert!(
                scenario.trim_end_matches('|').trim().len() > 20,
                "{name}: missing acceptance criteria"
            );
            assert!(
                documented.insert(name.to_owned()),
                "duplicate scenario: {name}"
            );
        }
    }
    assert_eq!(
        actual, documented,
        "Update the acceptance scenarios when commands change"
    );
}

#[test]
fn every_leaf_help_parses_without_credentials_or_database() {
    let mut leaves = BTreeSet::new();
    leaf_commands(&super::build_cli(), "", &mut leaves);
    for path in leaves {
        let args = std::iter::once("trust")
            .chain(path.split_whitespace())
            .chain(std::iter::once("--help"));
        let error = super::build_cli()
            .try_get_matches_from(args)
            .expect_err("help exits before dispatch");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp, "{path}");
    }
}
