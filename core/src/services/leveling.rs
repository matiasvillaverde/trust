use chrono::Utc;
use model::{DatabaseFactory, Level, LevelDirection, LevelTrigger};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use tracing::{debug, info};
use uuid::Uuid;

/// Performance snapshot used to evaluate automatic level transitions.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelPerformanceSnapshot {
    /// Number of profitable trades in the recent window.
    pub profitable_trades: u32,
    /// Win rate in percentage points, e.g. 70 means 70%.
    pub win_rate_percentage: Decimal,
    /// Monthly loss percentage (negative values represent loss).
    pub monthly_loss_percentage: Decimal,
    /// Largest single-trade loss percentage (negative values represent loss).
    pub largest_loss_percentage: Decimal,
    /// Number of consecutive winning trades.
    pub consecutive_wins: u32,
}

/// A policy decision to change levels.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDecision {
    /// Target level.
    pub target_level: u8,
    /// Human-readable reason.
    pub reason: String,
    /// Machine-readable trigger.
    pub trigger_type: LevelTrigger,
    /// Transition direction.
    pub direction: LevelDirection,
}

/// Strategy interface for level transition policies (GoF: Strategy).
pub trait LevelTransitionPolicy {
    /// Evaluate whether the current level should transition.
    fn evaluate(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelDecision>;
}

/// Default risk-first policy for automatic level transitions.
#[derive(Debug, Clone, Copy)]
pub struct DefaultLevelTransitionPolicy;

impl DefaultLevelTransitionPolicy {
    fn downgrade_target(level: u8) -> Option<u8> {
        level.checked_sub(1)
    }

    fn upgrade_target(level: u8) -> Option<u8> {
        level.checked_add(1).filter(|candidate| *candidate <= 4)
    }

    fn is_exceptional_performance(snapshot: &LevelPerformanceSnapshot) -> bool {
        snapshot.profitable_trades >= 20
            && snapshot.win_rate_percentage >= dec!(85)
            && snapshot.consecutive_wins >= 8
    }

    fn is_recovery_ready(snapshot: &LevelPerformanceSnapshot) -> bool {
        snapshot.profitable_trades >= 5
            && snapshot.win_rate_percentage >= dec!(65)
            && snapshot.consecutive_wins >= 2
    }
}

impl LevelTransitionPolicy for DefaultLevelTransitionPolicy {
    fn evaluate(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelDecision> {
        if current.status == model::LevelStatus::Cooldown && Self::is_recovery_ready(snapshot) {
            let target = Self::upgrade_target(current.current_level)?;
            return Some(LevelDecision {
                target_level: target,
                reason: "Cooldown recovery: restoring level after controlled reset".to_string(),
                trigger_type: LevelTrigger::PerformanceUpgrade,
                direction: LevelDirection::Upgrade,
            });
        }

        if current.status != model::LevelStatus::Cooldown
            && Self::is_exceptional_performance(snapshot)
        {
            let target = Self::downgrade_target(current.current_level)?;
            return Some(LevelDecision {
                target_level: target,
                reason: format!(
                    "Performance cooldown: profitable_trades={}, win_rate={}%, consecutive_wins={}",
                    snapshot.profitable_trades,
                    snapshot.win_rate_percentage,
                    snapshot.consecutive_wins
                ),
                trigger_type: LevelTrigger::PerformanceCooldown,
                direction: LevelDirection::Downgrade,
            });
        }

        if snapshot.monthly_loss_percentage <= dec!(-5)
            || snapshot.largest_loss_percentage <= dec!(-2)
        {
            let target = Self::downgrade_target(current.current_level)?;
            return Some(LevelDecision {
                target_level: target,
                reason: format!(
                    "Risk breach: monthly_loss={}%, largest_loss={}%",
                    snapshot.monthly_loss_percentage, snapshot.largest_loss_percentage
                ),
                trigger_type: LevelTrigger::RiskBreach,
                direction: LevelDirection::Downgrade,
            });
        }

        if snapshot.profitable_trades >= 10
            && snapshot.win_rate_percentage >= dec!(70)
            && snapshot.consecutive_wins >= 3
        {
            let target = Self::upgrade_target(current.current_level)?;
            return Some(LevelDecision {
                target_level: target,
                reason: format!(
                    "Performance upgrade: profitable_trades={}, win_rate={}%, consecutive_wins={}",
                    snapshot.profitable_trades,
                    snapshot.win_rate_percentage,
                    snapshot.consecutive_wins
                ),
                trigger_type: LevelTrigger::PerformanceUpgrade,
                direction: LevelDirection::Upgrade,
            });
        }

        None
    }
}

/// Result for evaluate-and-apply workflows.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelEvaluationOutcome {
    /// Current level before any optional apply.
    pub current_level: Level,
    /// Decision from active policy.
    pub decision: Option<LevelDecision>,
    /// Persisted level after apply, when requested.
    pub applied_level: Option<Level>,
}

/// Service coordinating level reads, policy evaluation, and writes.
#[derive(Debug)]
pub struct LevelingService<P: LevelTransitionPolicy> {
    policy: P,
}

impl<P: LevelTransitionPolicy> LevelingService<P> {
    /// Build service with explicit policy implementation.
    pub fn new(policy: P) -> Self {
        Self { policy }
    }

    /// Evaluate current account level.
    pub fn evaluate(
        &self,
        factory: &mut dyn DatabaseFactory,
        account_id: Uuid,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Result<LevelEvaluationOutcome, Box<dyn Error>> {
        let current = factory.level_read().level_for_account(account_id)?;
        let decision = self.policy.evaluate(&current, snapshot);
        debug!(
            account_id = %account_id,
            current_level = current.current_level,
            has_decision = decision.is_some(),
            "evaluated level transition policy"
        );

        Ok(LevelEvaluationOutcome {
            current_level: current,
            decision,
            applied_level: None,
        })
    }

    /// Evaluate and optionally apply a transition atomically.
    pub fn evaluate_and_apply(
        &self,
        factory: &mut dyn DatabaseFactory,
        account_id: Uuid,
        snapshot: &LevelPerformanceSnapshot,
        apply: bool,
    ) -> Result<LevelEvaluationOutcome, Box<dyn Error>> {
        let mut outcome = self.evaluate(factory, account_id, snapshot)?;

        if !apply {
            return Ok(outcome);
        }

        let decision = match outcome.decision.clone() {
            Some(decision) => decision,
            None => return Ok(outcome),
        };
        info!(
            account_id = %account_id,
            from_level = outcome.current_level.current_level,
            to_level = decision.target_level,
            trigger = %decision.trigger_type,
            "applying level transition"
        );

        let savepoint = "level_transition";
        factory.begin_savepoint(savepoint)?;

        let now = Utc::now().naive_utc();
        let transition = outcome.current_level.transition_to(
            decision.target_level,
            &decision.reason,
            decision.trigger_type,
            now,
        );

        let (updated_level, change) = match transition {
            Ok(data) => data,
            Err(error) => {
                let _ = factory.rollback_to_savepoint(savepoint);
                return Err(Box::new(error));
            }
        };

        let persisted_level = match factory.level_write().update_level(&updated_level) {
            Ok(level) => level,
            Err(error) => {
                let _ = factory.rollback_to_savepoint(savepoint);
                return Err(error);
            }
        };

        if let Err(error) = factory.level_write().create_level_change(&change) {
            let _ = factory.rollback_to_savepoint(savepoint);
            return Err(error);
        }

        factory.release_savepoint(savepoint)?;
        outcome.applied_level = Some(persisted_level);
        Ok(outcome)
    }
}

impl Default for LevelingService<DefaultLevelTransitionPolicy> {
    fn default() -> Self {
        Self::new(DefaultLevelTransitionPolicy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::LevelStatus;

    #[test]
    fn test_default_policy_upgrade() {
        let mut current = Level::default_for_account(Uuid::new_v4());
        current.current_level = 2;
        current.status = LevelStatus::Normal;

        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 12,
            win_rate_percentage: dec!(75),
            monthly_loss_percentage: dec!(-1),
            largest_loss_percentage: dec!(-0.5),
            consecutive_wins: 4,
        };

        let decision = DefaultLevelTransitionPolicy
            .evaluate(&current, &snapshot)
            .expect("expected decision");
        assert_eq!(decision.target_level, 3);
        assert_eq!(decision.direction, LevelDirection::Upgrade);
    }

    #[test]
    fn test_default_policy_downgrade_priority() {
        let current = Level::default_for_account(Uuid::new_v4());
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 20,
            win_rate_percentage: dec!(90),
            monthly_loss_percentage: dec!(-5.5),
            largest_loss_percentage: dec!(-0.4),
            consecutive_wins: 8,
        };

        let decision = DefaultLevelTransitionPolicy
            .evaluate(&current, &snapshot)
            .expect("expected decision");
        assert_eq!(decision.target_level, 2);
        assert_eq!(decision.direction, LevelDirection::Downgrade);
    }

    #[test]
    fn test_default_policy_at_level_four_enters_cooldown_on_exceptional_performance() {
        let mut current = Level::default_for_account(Uuid::new_v4());
        current.current_level = 4;
        current.risk_multiplier = dec!(1.50);
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 25,
            win_rate_percentage: dec!(95),
            monthly_loss_percentage: dec!(-0.1),
            largest_loss_percentage: dec!(-0.1),
            consecutive_wins: 10,
        };

        let decision = DefaultLevelTransitionPolicy
            .evaluate(&current, &snapshot)
            .expect("expected cooldown decision");
        assert_eq!(decision.direction, LevelDirection::Downgrade);
        assert_eq!(decision.target_level, 3);
        assert_eq!(decision.trigger_type, LevelTrigger::PerformanceCooldown);
    }

    #[test]
    fn test_default_policy_does_not_downgrade_below_zero() {
        let mut current = Level::default_for_account(Uuid::new_v4());
        current.current_level = 0;
        current.risk_multiplier = dec!(0.10);
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 0,
            win_rate_percentage: dec!(10),
            monthly_loss_percentage: dec!(-50),
            largest_loss_percentage: dec!(-10),
            consecutive_wins: 0,
        };

        let decision = DefaultLevelTransitionPolicy.evaluate(&current, &snapshot);
        assert!(decision.is_none());
    }

    #[test]
    fn test_default_policy_enters_cooldown_on_exceptional_performance() {
        let current = Level::default_for_account(Uuid::new_v4());
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 30,
            win_rate_percentage: dec!(92),
            monthly_loss_percentage: dec!(-0.2),
            largest_loss_percentage: dec!(-0.2),
            consecutive_wins: 12,
        };

        let decision = DefaultLevelTransitionPolicy
            .evaluate(&current, &snapshot)
            .expect("expected cooldown decision");
        assert_eq!(decision.direction, LevelDirection::Downgrade);
        assert_eq!(decision.target_level, 2);
        assert_eq!(decision.trigger_type, LevelTrigger::PerformanceCooldown);
    }

    #[test]
    fn test_default_policy_recovers_from_cooldown_quickly() {
        let mut current = Level::default_for_account(Uuid::new_v4());
        current.current_level = 2;
        current.risk_multiplier = dec!(0.50);
        current.status = LevelStatus::Cooldown;

        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 6,
            win_rate_percentage: dec!(70),
            monthly_loss_percentage: dec!(-1),
            largest_loss_percentage: dec!(-0.7),
            consecutive_wins: 3,
        };

        let decision = DefaultLevelTransitionPolicy
            .evaluate(&current, &snapshot)
            .expect("expected recovery decision");
        assert_eq!(decision.direction, LevelDirection::Upgrade);
        assert_eq!(decision.target_level, 3);
        assert_eq!(decision.trigger_type, LevelTrigger::PerformanceUpgrade);
    }
}
