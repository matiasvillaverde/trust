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
            let marker = Self::change_marker(change.old_level, change.new_level);
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

    fn change_marker(old_level: u8, new_level: u8) -> &'static str {
        if new_level > old_level {
            "+"
        } else if new_level < old_level {
            "-"
        } else {
            "="
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LevelView;
    use chrono::Utc;
    use model::{Level, LevelChange, LevelStatus, LevelTrigger};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn change_marker_covers_up_down_and_same() {
        assert_eq!(LevelView::change_marker(1, 2), "+");
        assert_eq!(LevelView::change_marker(2, 1), "-");
        assert_eq!(LevelView::change_marker(2, 2), "=");
    }

    #[test]
    fn status_and_history_render_without_panics() {
        let level = Level {
            account_id: Uuid::new_v4(),
            current_level: 3,
            risk_multiplier: dec!(1.0),
            status: LevelStatus::Normal,
            trades_at_level: 5,
            level_start_date: Utc::now().date_naive(),
            ..Level::default_for_account(Uuid::new_v4())
        };
        LevelView::status(&level);

        let now = Utc::now().naive_utc();
        let changes = vec![
            LevelChange {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                account_id: Uuid::new_v4(),
                old_level: 2,
                new_level: 3,
                change_reason: "upgrade".to_string(),
                trigger_type: LevelTrigger::ManualOverride,
                changed_at: now,
            },
            LevelChange {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                account_id: Uuid::new_v4(),
                old_level: 3,
                new_level: 2,
                change_reason: "downgrade".to_string(),
                trigger_type: LevelTrigger::MonthlyLoss,
                changed_at: now,
            },
        ];

        LevelView::history(&changes);
        LevelView::history(&[]);
    }
}
