use chrono::{NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use uuid::Uuid;

/// Trading level for an account and its corresponding risk profile.
#[derive(PartialEq, Debug, Clone)]
pub struct Level {
    /// Unique level row id.
    pub id: Uuid,
    /// Created timestamp.
    pub created_at: NaiveDateTime,
    /// Last updated timestamp.
    pub updated_at: NaiveDateTime,
    /// Soft-delete marker.
    pub deleted_at: Option<NaiveDateTime>,
    /// Owner account id.
    pub account_id: Uuid,
    /// Level value in range [0, 4].
    pub current_level: u8,
    /// Position-size multiplier derived from level.
    pub risk_multiplier: Decimal,
    /// Current level status.
    pub status: LevelStatus,
    /// Number of trades executed at current level.
    pub trades_at_level: u32,
    /// Date when current level started.
    pub level_start_date: NaiveDate,
}

/// Configurable rules used by level transition policies.
#[derive(PartialEq, Debug, Clone)]
pub struct LevelAdjustmentRules {
    /// Monthly loss threshold (%) to trigger downgrade.
    pub monthly_loss_downgrade_pct: Decimal,
    /// Largest single-trade loss threshold (%) to trigger downgrade.
    pub single_loss_downgrade_pct: Decimal,
    /// Profitable trade count needed to upgrade.
    pub upgrade_profitable_trades: u32,
    /// Win-rate threshold (%) needed to upgrade.
    pub upgrade_win_rate_pct: Decimal,
    /// Consecutive wins needed to upgrade.
    pub upgrade_consecutive_wins: u32,
    /// Profitable trade count for cooldown trigger.
    pub cooldown_profitable_trades: u32,
    /// Win-rate threshold (%) for cooldown trigger.
    pub cooldown_win_rate_pct: Decimal,
    /// Consecutive wins for cooldown trigger.
    pub cooldown_consecutive_wins: u32,
    /// Profitable trades needed to recover from cooldown.
    pub recovery_profitable_trades: u32,
    /// Win-rate threshold (%) needed for cooldown recovery.
    pub recovery_win_rate_pct: Decimal,
    /// Consecutive wins needed for cooldown recovery.
    pub recovery_consecutive_wins: u32,
    /// Minimum trades at level before allowing upgrades.
    pub min_trades_at_level_for_upgrade: u32,
    /// Maximum number of level changes allowed in rolling 30d.
    pub max_changes_in_30_days: u32,
}

/// Immutable audit record for a level change event.
#[derive(PartialEq, Debug, Clone)]
pub struct LevelChange {
    /// Unique level change id.
    pub id: Uuid,
    /// Created timestamp.
    pub created_at: NaiveDateTime,
    /// Last updated timestamp.
    pub updated_at: NaiveDateTime,
    /// Soft-delete marker.
    pub deleted_at: Option<NaiveDateTime>,
    /// Owner account id.
    pub account_id: Uuid,
    /// Previous level.
    pub old_level: u8,
    /// New level.
    pub new_level: u8,
    /// Human-readable reason.
    pub change_reason: String,
    /// Machine-readable trigger type.
    pub trigger_type: LevelTrigger,
    /// Event timestamp.
    pub changed_at: NaiveDateTime,
}

/// Current account-level trading state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelStatus {
    /// Normal operation.
    Normal,
    /// Temporary restricted operation due to performance issues.
    Probation,
    /// Cooling period after a transition.
    Cooldown,
}

/// Direction for a level transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelDirection {
    /// Level is increased.
    Upgrade,
    /// Level is reduced.
    Downgrade,
}

/// Typed trigger for level-change events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelTrigger {
    /// Manual operator intervention.
    ManualOverride,
    /// Manual review by risk manager.
    ManualReview,
    /// Monthly loss threshold breach.
    MonthlyLoss,
    /// Large single loss threshold breach.
    LargeLoss,
    /// Composite risk breach.
    RiskBreach,
    /// Positive performance threshold met.
    PerformanceUpgrade,
    /// Temporary downshift after exceptional performance to prevent overconfidence.
    PerformanceCooldown,
    /// Consecutive wins threshold met.
    ConsecutiveWins,
    /// Initial account bootstrap.
    AccountCreation,
    /// Any non-standard trigger value.
    Custom(String),
}

/// Domain-level validation and transition errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelError {
    /// Level must be between 0 and 4.
    InvalidLevel(u8),
    /// Requested transition kept same level.
    UnchangedLevel(u8),
    /// Change reason must be non-empty.
    EmptyReason,
    /// Trigger type must be non-empty.
    EmptyTrigger,
}

/// Validation errors for configurable level-adjustment rules.
#[derive(Debug, Clone, PartialEq)]
pub enum LevelRulesError {
    /// Field must be strictly negative (loss threshold).
    LossThresholdMustBeNegative(&'static str),
    /// Percentage field must be in range [0, 100].
    PercentageOutOfRange(&'static str, Decimal),
    /// Field must be greater than zero.
    MustBeGreaterThanZero(&'static str),
}

impl std::error::Error for LevelError {}
impl std::error::Error for LevelRulesError {}

impl Display for LevelError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LevelError::InvalidLevel(level) => write!(f, "Invalid level {level}; expected 0..=4"),
            LevelError::UnchangedLevel(level) => {
                write!(f, "Requested level transition keeps current level {level}")
            }
            LevelError::EmptyReason => write!(f, "Level change reason cannot be empty"),
            LevelError::EmptyTrigger => write!(f, "Level change trigger cannot be empty"),
        }
    }
}

impl Display for LevelRulesError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LevelRulesError::LossThresholdMustBeNegative(field) => {
                write!(f, "{field} must be negative")
            }
            LevelRulesError::PercentageOutOfRange(field, value) => {
                write!(f, "{field} must be between 0 and 100, got {value}")
            }
            LevelRulesError::MustBeGreaterThanZero(field) => {
                write!(f, "{field} must be greater than zero")
            }
        }
    }
}

impl Level {
    /// Create the default Level 3 profile for a newly created account.
    pub fn default_for_account(account_id: Uuid) -> Self {
        let now = Utc::now().naive_utc();
        Level {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id,
            current_level: 3,
            risk_multiplier: dec!(1.0),
            status: LevelStatus::Normal,
            trades_at_level: 0,
            level_start_date: now.date(),
        }
    }

    /// Validate level value is inside the supported range.
    pub fn validate_level(level: u8) -> Result<(), LevelError> {
        if level > 4 {
            return Err(LevelError::InvalidLevel(level));
        }
        Ok(())
    }

    /// Risk multiplier for the provided level.
    pub fn multiplier_for_level(level: u8) -> Result<Decimal, LevelError> {
        Self::validate_level(level)?;
        match level {
            0 => Ok(dec!(0.10)),
            1 => Ok(dec!(0.25)),
            2 => Ok(dec!(0.50)),
            3 => Ok(dec!(1.00)),
            4 => Ok(dec!(1.50)),
            _ => Err(LevelError::InvalidLevel(level)),
        }
    }

    /// Human-readable description of a level.
    pub fn level_description(level: u8) -> &'static str {
        match level {
            0 => "Restricted Trading",
            1 => "Limited Trading",
            2 => "Partial Size Trading",
            3 => "Full Size Trading",
            4 => "Enhanced Trading",
            _ => "Invalid Level",
        }
    }

    /// Determine transition direction when changing from current to target level.
    pub fn direction_to(&self, target_level: u8) -> Result<LevelDirection, LevelError> {
        Self::validate_level(target_level)?;
        if target_level == self.current_level {
            return Err(LevelError::UnchangedLevel(target_level));
        }

        if target_level > self.current_level {
            return Ok(LevelDirection::Upgrade);
        }

        Ok(LevelDirection::Downgrade)
    }

    /// Build an updated level and audit event.
    pub fn transition_to(
        &self,
        target_level: u8,
        reason: &str,
        trigger_type: LevelTrigger,
        changed_at: NaiveDateTime,
    ) -> Result<(Level, LevelChange), LevelError> {
        let direction = self.direction_to(target_level)?;

        if reason.trim().is_empty() {
            return Err(LevelError::EmptyReason);
        }
        let trigger = match trigger_type {
            LevelTrigger::Custom(value) => {
                if value.trim().is_empty() {
                    return Err(LevelError::EmptyTrigger);
                }
                LevelTrigger::Custom(value.trim().to_string())
            }
            value => value,
        };

        let updated = Level {
            id: self.id,
            created_at: self.created_at,
            updated_at: changed_at,
            deleted_at: self.deleted_at,
            account_id: self.account_id,
            current_level: target_level,
            risk_multiplier: Self::multiplier_for_level(target_level)?,
            status: match direction {
                LevelDirection::Upgrade => LevelStatus::Normal,
                LevelDirection::Downgrade => {
                    if trigger == LevelTrigger::PerformanceCooldown {
                        LevelStatus::Cooldown
                    } else {
                        LevelStatus::Probation
                    }
                }
            },
            trades_at_level: 0,
            level_start_date: changed_at.date(),
        };

        let event = LevelChange {
            id: Uuid::new_v4(),
            created_at: changed_at,
            updated_at: changed_at,
            deleted_at: None,
            account_id: self.account_id,
            old_level: self.current_level,
            new_level: target_level,
            change_reason: reason.trim().to_string(),
            trigger_type: trigger,
            changed_at,
        };

        Ok((updated, event))
    }
}

impl Default for LevelAdjustmentRules {
    fn default() -> Self {
        Self {
            monthly_loss_downgrade_pct: dec!(-5.0),
            single_loss_downgrade_pct: dec!(-2.0),
            upgrade_profitable_trades: 10,
            upgrade_win_rate_pct: dec!(70.0),
            upgrade_consecutive_wins: 3,
            cooldown_profitable_trades: 20,
            cooldown_win_rate_pct: dec!(85.0),
            cooldown_consecutive_wins: 8,
            recovery_profitable_trades: 5,
            recovery_win_rate_pct: dec!(65.0),
            recovery_consecutive_wins: 2,
            min_trades_at_level_for_upgrade: 5,
            max_changes_in_30_days: 2,
        }
    }
}

impl LevelAdjustmentRules {
    /// Validate safety invariants for persisted transition rules.
    pub fn validate(&self) -> Result<(), LevelRulesError> {
        if self.monthly_loss_downgrade_pct >= dec!(0) {
            return Err(LevelRulesError::LossThresholdMustBeNegative(
                "monthly_loss_downgrade_pct",
            ));
        }
        if self.single_loss_downgrade_pct >= dec!(0) {
            return Err(LevelRulesError::LossThresholdMustBeNegative(
                "single_loss_downgrade_pct",
            ));
        }

        for (field, value) in [
            ("upgrade_win_rate_pct", self.upgrade_win_rate_pct),
            ("cooldown_win_rate_pct", self.cooldown_win_rate_pct),
            ("recovery_win_rate_pct", self.recovery_win_rate_pct),
        ] {
            if value < dec!(0) || value > dec!(100) {
                return Err(LevelRulesError::PercentageOutOfRange(field, value));
            }
        }

        for (field, value) in [
            ("upgrade_profitable_trades", self.upgrade_profitable_trades),
            ("upgrade_consecutive_wins", self.upgrade_consecutive_wins),
            (
                "cooldown_profitable_trades",
                self.cooldown_profitable_trades,
            ),
            ("cooldown_consecutive_wins", self.cooldown_consecutive_wins),
            (
                "recovery_profitable_trades",
                self.recovery_profitable_trades,
            ),
            ("recovery_consecutive_wins", self.recovery_consecutive_wins),
            (
                "min_trades_at_level_for_upgrade",
                self.min_trades_at_level_for_upgrade,
            ),
            ("max_changes_in_30_days", self.max_changes_in_30_days),
        ] {
            if value == 0 {
                return Err(LevelRulesError::MustBeGreaterThanZero(field));
            }
        }

        Ok(())
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "L{} {} ({:.2}x, {})",
            self.current_level,
            Self::level_description(self.current_level),
            self.risk_multiplier,
            self.status,
        )
    }
}

impl Display for LevelStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            LevelStatus::Normal => write!(f, "normal"),
            LevelStatus::Probation => write!(f, "probation"),
            LevelStatus::Cooldown => write!(f, "cooldown"),
        }
    }
}

impl Display for LevelTrigger {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LevelTrigger::ManualOverride => write!(f, "manual_override"),
            LevelTrigger::ManualReview => write!(f, "manual_review"),
            LevelTrigger::MonthlyLoss => write!(f, "monthly_loss"),
            LevelTrigger::LargeLoss => write!(f, "large_loss"),
            LevelTrigger::RiskBreach => write!(f, "risk_breach"),
            LevelTrigger::PerformanceUpgrade => write!(f, "performance_upgrade"),
            LevelTrigger::PerformanceCooldown => write!(f, "performance_cooldown"),
            LevelTrigger::ConsecutiveWins => write!(f, "consecutive_wins"),
            LevelTrigger::AccountCreation => write!(f, "account_creation"),
            LevelTrigger::Custom(value) => write!(f, "{value}"),
        }
    }
}

impl LevelTrigger {
    /// Known built-in trigger identifiers accepted by the CLI.
    pub const fn known_values() -> &'static [&'static str] {
        &[
            "manual_override",
            "manual_review",
            "monthly_loss",
            "large_loss",
            "risk_breach",
            "performance_upgrade",
            "performance_cooldown",
            "consecutive_wins",
            "account_creation",
        ]
    }
}

/// Parsing failure for [`LevelStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelStatusParseError;

impl std::error::Error for LevelStatusParseError {}

impl Display for LevelStatusParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid level status")
    }
}

impl FromStr for LevelStatus {
    type Err = LevelStatusParseError;

    fn from_str(status: &str) -> Result<Self, Self::Err> {
        match status {
            "normal" => Ok(LevelStatus::Normal),
            "probation" => Ok(LevelStatus::Probation),
            "cooldown" => Ok(LevelStatus::Cooldown),
            _ => Err(LevelStatusParseError),
        }
    }
}

/// Parsing failure for [`LevelTrigger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelTriggerParseError;

impl std::error::Error for LevelTriggerParseError {}

impl Display for LevelTriggerParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid level trigger")
    }
}

impl FromStr for LevelTrigger {
    type Err = LevelTriggerParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(LevelTriggerParseError);
        }
        match normalized.as_str() {
            "manual_override" => Ok(LevelTrigger::ManualOverride),
            "manual_review" => Ok(LevelTrigger::ManualReview),
            "monthly_loss" => Ok(LevelTrigger::MonthlyLoss),
            "large_loss" => Ok(LevelTrigger::LargeLoss),
            "risk_breach" => Ok(LevelTrigger::RiskBreach),
            "performance_upgrade" => Ok(LevelTrigger::PerformanceUpgrade),
            "performance_cooldown" => Ok(LevelTrigger::PerformanceCooldown),
            "consecutive_wins" => Ok(LevelTrigger::ConsecutiveWins),
            "account_creation" => Ok(LevelTrigger::AccountCreation),
            _ => Ok(LevelTrigger::Custom(normalized)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn assert_same_level_rejected(current: &Level, level: u8, now: NaiveDateTime) {
        assert!(current
            .transition_to(level, "same-level", LevelTrigger::ManualOverride, now)
            .is_err());
    }

    fn assert_transition_invariants(current: &Level, to: u8, account_id: Uuid, now: NaiveDateTime) {
        let from = current.current_level;
        let (updated, event) = current
            .transition_to(to, "  test reason  ", LevelTrigger::ManualReview, now)
            .unwrap();

        assert_eq!(updated.account_id, account_id);
        assert_eq!(updated.current_level, to);
        assert_eq!(
            updated.risk_multiplier,
            Level::multiplier_for_level(to).unwrap()
        );
        assert_eq!(updated.trades_at_level, 0);
        assert_eq!(updated.level_start_date, now.date());

        if to > from {
            assert_eq!(updated.status, LevelStatus::Normal);
        } else {
            assert_eq!(updated.status, LevelStatus::Probation);
        }

        assert_eq!(event.account_id, account_id);
        assert_eq!(event.old_level, from);
        assert_eq!(event.new_level, to);
        assert_eq!(event.change_reason, "test reason");
        assert_eq!(event.trigger_type, LevelTrigger::ManualReview);
        assert_eq!(event.changed_at, now);
    }

    #[test]
    fn test_level_multipliers() {
        assert_eq!(Level::multiplier_for_level(0).unwrap(), dec!(0.10));
        assert_eq!(Level::multiplier_for_level(1).unwrap(), dec!(0.25));
        assert_eq!(Level::multiplier_for_level(2).unwrap(), dec!(0.50));
        assert_eq!(Level::multiplier_for_level(3).unwrap(), dec!(1.00));
        assert_eq!(Level::multiplier_for_level(4).unwrap(), dec!(1.50));
    }

    #[test]
    fn test_transition_upgrade_and_downgrade() {
        let mut level = Level::default_for_account(Uuid::new_v4());
        level.current_level = 2;
        level.risk_multiplier = dec!(0.50);

        let now = Utc::now().naive_utc();
        let (up, up_change) = level
            .transition_to(3, "Strong performance", LevelTrigger::ConsecutiveWins, now)
            .unwrap();
        assert_eq!(up.current_level, 3);
        assert_eq!(up.status, LevelStatus::Normal);
        assert_eq!(up_change.old_level, 2);
        assert_eq!(up_change.new_level, 3);

        let (down, down_change) = up
            .transition_to(
                2,
                "Monthly loss exceeded threshold",
                LevelTrigger::MonthlyLoss,
                now,
            )
            .unwrap();
        assert_eq!(down.current_level, 2);
        assert_eq!(down.status, LevelStatus::Probation);
        assert_eq!(down_change.old_level, 3);
        assert_eq!(down_change.new_level, 2);
    }

    #[test]
    fn test_transition_downgrade_for_performance_cooldown_sets_cooldown_status() {
        let level = Level::default_for_account(Uuid::new_v4());
        let now = Utc::now().naive_utc();
        let (updated, change) = level
            .transition_to(
                2,
                "Exceptional streak cooldown",
                LevelTrigger::PerformanceCooldown,
                now,
            )
            .unwrap();
        assert_eq!(updated.status, LevelStatus::Cooldown);
        assert_eq!(change.trigger_type, LevelTrigger::PerformanceCooldown);
    }

    #[test]
    fn test_reject_invalid_and_unchanged_transition() {
        let level = Level::default_for_account(Uuid::new_v4());
        assert!(matches!(
            level.direction_to(6),
            Err(LevelError::InvalidLevel(6))
        ));
        assert!(matches!(
            level.direction_to(3),
            Err(LevelError::UnchangedLevel(3))
        ));
    }

    #[test]
    fn test_level_trigger_roundtrip() {
        let parsed = "manual_review".parse::<LevelTrigger>().unwrap();
        assert_eq!(parsed, LevelTrigger::ManualReview);
        assert_eq!(parsed.to_string(), "manual_review");

        let custom = "my_custom_signal".parse::<LevelTrigger>().unwrap();
        assert_eq!(custom.to_string(), "my_custom_signal");
    }

    #[test]
    fn test_transition_invariants_across_all_level_pairs() {
        let account_id = Uuid::new_v4();
        let base = Level::default_for_account(account_id);
        let now = Utc::now().naive_utc();

        for from in 0_u8..=4 {
            for to in 0_u8..=4 {
                let mut current = base.clone();
                current.current_level = from;
                current.risk_multiplier = Level::multiplier_for_level(from).unwrap();
                current.status = LevelStatus::Normal;

                if from == to {
                    assert_same_level_rejected(&current, to, now);
                    continue;
                }

                assert_transition_invariants(&current, to, account_id, now);
            }
        }
    }

    #[test]
    fn test_default_adjustment_rules_are_valid() {
        let rules = LevelAdjustmentRules::default();
        assert!(rules.validate().is_ok());
    }

    #[test]
    fn test_adjustment_rules_reject_invalid_thresholds() {
        let rules = LevelAdjustmentRules {
            monthly_loss_downgrade_pct: dec!(0),
            ..LevelAdjustmentRules::default()
        };
        assert!(matches!(
            rules.validate(),
            Err(LevelRulesError::LossThresholdMustBeNegative(
                "monthly_loss_downgrade_pct"
            ))
        ));

        let rules = LevelAdjustmentRules {
            upgrade_win_rate_pct: dec!(101),
            ..LevelAdjustmentRules::default()
        };
        assert!(matches!(
            rules.validate(),
            Err(LevelRulesError::PercentageOutOfRange(
                "upgrade_win_rate_pct",
                _
            ))
        ));

        let rules = LevelAdjustmentRules {
            max_changes_in_30_days: 0,
            ..LevelAdjustmentRules::default()
        };
        assert!(matches!(
            rules.validate(),
            Err(LevelRulesError::MustBeGreaterThanZero(
                "max_changes_in_30_days"
            ))
        ));
    }
}
