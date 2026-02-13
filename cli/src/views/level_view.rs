use chrono::Utc;
use model::{Level, LevelChange};

/// Rendering helpers for level commands.
pub struct LevelView;

impl LevelView {
    pub fn status(level: &Level) {
        println!("Mario Bros Level Status");
        println!("======================");
        println!(
            "Current Level: {} ({})",
            level.current_level,
            Level::level_description(level.current_level)
        );
        println!(
            "Risk Multiplier: {}x ({}% position sizing)",
            level.risk_multiplier,
            level
                .risk_multiplier
                .checked_mul(rust_decimal_macros::dec!(100))
                .unwrap_or_default()
        );
        println!("Status: {}", level.status);
        println!("Trades at Current Level: {}", level.trades_at_level);
        let days = Utc::now()
            .date_naive()
            .signed_duration_since(level.level_start_date)
            .num_days();
        println!("Time at Level: {} day(s)", days.max(0));
    }

    pub fn history(changes: &[LevelChange]) {
        println!("Level Change History");
        println!("===================");

        if changes.is_empty() {
            println!("No level changes recorded yet.");
            return;
        }

        for change in changes {
            let marker = if change.new_level > change.old_level {
                "+"
            } else if change.new_level < change.old_level {
                "-"
            } else {
                "="
            };
            println!(
                "{}: Level {}->{} ({}) {} [{}]",
                change.changed_at.format("%Y-%m-%d"),
                change.old_level,
                change.new_level,
                marker,
                change.change_reason,
                change.trigger_type,
            );
        }
    }
}
