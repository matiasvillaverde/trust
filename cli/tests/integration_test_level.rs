use cli::commands::LevelCommandBuilder;
use model::Level;
use rust_decimal_macros::dec;

#[test]
fn test_level_command_structure() {
    // Test that the LevelCommandBuilder builds correctly with subcommands
    let builder = LevelCommandBuilder::new();
    let command = builder.status_command().history_command().build();

    // Verify command name
    assert_eq!(command.get_name(), "level");

    // Verify subcommands exist
    let subcommands: Vec<&str> = command.get_subcommands().map(|c| c.get_name()).collect();
    assert!(subcommands.contains(&"status"));
    assert!(subcommands.contains(&"history"));
    assert_eq!(subcommands.len(), 2);

    // Verify history command has days argument with correct default
    let history_cmd = command
        .get_subcommands()
        .find(|c| c.get_name() == "history")
        .expect("History command should exist");

    let days_arg = history_cmd
        .get_arguments()
        .find(|a| a.get_id() == "days")
        .expect("Days argument should exist");

    assert_eq!(days_arg.get_default_values(), &["90"]);
}

#[test]
fn test_level_multipliers() {
    // Test the level multiplier constants are correct
    assert_eq!(Level::multiplier_for_level(0).unwrap(), dec!(0.1)); // 10%
    assert_eq!(Level::multiplier_for_level(1).unwrap(), dec!(0.25)); // 25%
    assert_eq!(Level::multiplier_for_level(2).unwrap(), dec!(0.5)); // 50%
    assert_eq!(Level::multiplier_for_level(3).unwrap(), dec!(1.0)); // 100%
    assert_eq!(Level::multiplier_for_level(4).unwrap(), dec!(1.5)); // 150%
}

#[test]
fn test_level_descriptions() {
    // Test level descriptions are correct
    assert_eq!(Level::level_description(0), "Restricted Trading");
    assert_eq!(Level::level_description(1), "Limited Trading");
    assert_eq!(Level::level_description(2), "Partial Size Trading");
    assert_eq!(Level::level_description(3), "Full Size Trading");
    assert_eq!(Level::level_description(4), "Enhanced Trading");
}
