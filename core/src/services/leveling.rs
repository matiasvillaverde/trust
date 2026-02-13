use crate::events::trade::TradeClosed;
use chrono::Utc;
use model::{
    DatabaseFactory, Level, LevelAdjustmentRules, LevelDirection, LevelStatus, LevelTrigger,
};
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

/// Progress for one criterion in a transition path.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelCriterionProgress {
    /// Criterion key, stable for machine consumption.
    pub key: &'static str,
    /// Comparator semantics (`>=` or `<=`).
    pub comparator: &'static str,
    /// Actual observed value.
    pub actual: Decimal,
    /// Threshold value.
    pub threshold: Decimal,
    /// Remaining distance to satisfy threshold.
    pub missing: Decimal,
    /// True when criterion is currently met.
    pub met: bool,
}

/// Progress for a potential next transition.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelPathProgress {
    /// Stable path identifier.
    pub path: &'static str,
    /// Trigger corresponding to this path.
    pub trigger_type: LevelTrigger,
    /// Direction of transition.
    pub direction: LevelDirection,
    /// Adjacent target level, if level bounds allow a transition.
    pub target_level: Option<u8>,
    /// Criterion-level progress for this path.
    pub criteria: Vec<LevelCriterionProgress>,
    /// True when all criteria for this path are currently met.
    pub all_met: bool,
}

/// Progress report that explains what is missing to move up/down.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelProgressReport {
    /// Current level.
    pub current_level: u8,
    /// Current level status.
    pub status: LevelStatus,
    /// Paths that can lead to an immediate level upgrade.
    pub upgrade_paths: Vec<LevelPathProgress>,
    /// Paths that can lead to an immediate level downgrade.
    pub downgrade_paths: Vec<LevelPathProgress>,
}

/// Strategy interface for level transition policies (GoF: Strategy).
pub trait LevelTransitionPolicy {
    /// Evaluate whether the current level should transition.
    fn evaluate(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelDecision>;

    /// Explain progress versus policy thresholds for adjacent up/down transitions.
    fn progress(&self, current: &Level, snapshot: &LevelPerformanceSnapshot)
        -> LevelProgressReport;
}

/// Default risk-first policy for automatic level transitions.
#[derive(Debug, Clone)]
pub struct DefaultLevelTransitionPolicy {
    rules: LevelAdjustmentRules,
}

impl DefaultLevelTransitionPolicy {
    /// Build a policy with explicit, persisted transition rules.
    pub fn new(rules: LevelAdjustmentRules) -> Self {
        Self { rules }
    }

    fn downgrade_target(level: u8) -> Option<u8> {
        level.checked_sub(1)
    }

    fn upgrade_target(level: u8) -> Option<u8> {
        level.checked_add(1).filter(|candidate| *candidate <= 4)
    }

    fn is_exceptional_performance(&self, snapshot: &LevelPerformanceSnapshot) -> bool {
        snapshot.profitable_trades >= self.rules.cooldown_profitable_trades
            && snapshot.win_rate_percentage >= self.rules.cooldown_win_rate_pct
            && snapshot.consecutive_wins >= self.rules.cooldown_consecutive_wins
    }

    fn is_recovery_ready(&self, snapshot: &LevelPerformanceSnapshot) -> bool {
        snapshot.profitable_trades >= self.rules.recovery_profitable_trades
            && snapshot.win_rate_percentage >= self.rules.recovery_win_rate_pct
            && snapshot.consecutive_wins >= self.rules.recovery_consecutive_wins
    }

    fn at_least(key: &'static str, actual: Decimal, threshold: Decimal) -> LevelCriterionProgress {
        let met = actual >= threshold;
        let missing = if met {
            Decimal::ZERO
        } else {
            threshold.checked_sub(actual).unwrap_or(Decimal::ZERO)
        };
        LevelCriterionProgress {
            key,
            comparator: ">=",
            actual,
            threshold,
            missing,
            met,
        }
    }

    fn at_most(key: &'static str, actual: Decimal, threshold: Decimal) -> LevelCriterionProgress {
        let met = actual <= threshold;
        let missing = if met {
            Decimal::ZERO
        } else {
            actual.checked_sub(threshold).unwrap_or(Decimal::ZERO)
        };
        LevelCriterionProgress {
            key,
            comparator: "<=",
            actual,
            threshold,
            missing,
            met,
        }
    }

    fn upgrade_progress(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelPathProgress> {
        let target = Self::upgrade_target(current.current_level)?;
        let criteria = vec![
            Self::at_least(
                "profitable_trades",
                Decimal::from(snapshot.profitable_trades),
                Decimal::from(self.rules.upgrade_profitable_trades),
            ),
            Self::at_least(
                "win_rate_percentage",
                snapshot.win_rate_percentage,
                self.rules.upgrade_win_rate_pct,
            ),
            Self::at_least(
                "consecutive_wins",
                Decimal::from(snapshot.consecutive_wins),
                Decimal::from(self.rules.upgrade_consecutive_wins),
            ),
        ];
        let all_met = criteria.iter().all(|c| c.met);
        Some(LevelPathProgress {
            path: if current.status == LevelStatus::Cooldown {
                "cooldown_recovery"
            } else {
                "performance_upgrade"
            },
            trigger_type: LevelTrigger::PerformanceUpgrade,
            direction: LevelDirection::Upgrade,
            target_level: Some(target),
            criteria,
            all_met,
        })
    }

    fn risk_breach_progress(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<Vec<LevelPathProgress>> {
        let target = Self::downgrade_target(current.current_level)?;
        let monthly = Self::at_most(
            "monthly_loss_percentage",
            snapshot.monthly_loss_percentage,
            self.rules.monthly_loss_downgrade_pct,
        );
        let largest = Self::at_most(
            "largest_loss_percentage",
            snapshot.largest_loss_percentage,
            self.rules.single_loss_downgrade_pct,
        );

        let by_monthly = LevelPathProgress {
            path: "risk_breach_monthly_loss",
            trigger_type: LevelTrigger::RiskBreach,
            direction: LevelDirection::Downgrade,
            target_level: Some(target),
            criteria: vec![monthly.clone()],
            all_met: monthly.met,
        };
        let by_largest = LevelPathProgress {
            path: "risk_breach_largest_loss",
            trigger_type: LevelTrigger::RiskBreach,
            direction: LevelDirection::Downgrade,
            target_level: Some(target),
            criteria: vec![largest.clone()],
            all_met: largest.met,
        };

        Some(vec![by_monthly, by_largest])
    }

    fn cooldown_progress(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelPathProgress> {
        if current.status == LevelStatus::Cooldown {
            return None;
        }
        let target = Self::downgrade_target(current.current_level)?;
        let criteria = vec![
            Self::at_least(
                "profitable_trades",
                Decimal::from(snapshot.profitable_trades),
                Decimal::from(self.rules.cooldown_profitable_trades),
            ),
            Self::at_least(
                "win_rate_percentage",
                snapshot.win_rate_percentage,
                self.rules.cooldown_win_rate_pct,
            ),
            Self::at_least(
                "consecutive_wins",
                Decimal::from(snapshot.consecutive_wins),
                Decimal::from(self.rules.cooldown_consecutive_wins),
            ),
        ];
        let all_met = criteria.iter().all(|c| c.met);
        Some(LevelPathProgress {
            path: "performance_cooldown",
            trigger_type: LevelTrigger::PerformanceCooldown,
            direction: LevelDirection::Downgrade,
            target_level: Some(target),
            criteria,
            all_met,
        })
    }
}

impl LevelTransitionPolicy for DefaultLevelTransitionPolicy {
    fn evaluate(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> Option<LevelDecision> {
        if current.status == model::LevelStatus::Cooldown && self.is_recovery_ready(snapshot) {
            let target = Self::upgrade_target(current.current_level)?;
            return Some(LevelDecision {
                target_level: target,
                reason: "Cooldown recovery: restoring level after controlled reset".to_string(),
                trigger_type: LevelTrigger::PerformanceUpgrade,
                direction: LevelDirection::Upgrade,
            });
        }

        if current.status != model::LevelStatus::Cooldown
            && self.is_exceptional_performance(snapshot)
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

        if snapshot.monthly_loss_percentage <= self.rules.monthly_loss_downgrade_pct
            || snapshot.largest_loss_percentage <= self.rules.single_loss_downgrade_pct
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

        if snapshot.profitable_trades >= self.rules.upgrade_profitable_trades
            && snapshot.win_rate_percentage >= self.rules.upgrade_win_rate_pct
            && snapshot.consecutive_wins >= self.rules.upgrade_consecutive_wins
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

    fn progress(
        &self,
        current: &Level,
        snapshot: &LevelPerformanceSnapshot,
    ) -> LevelProgressReport {
        let mut upgrade_paths = Vec::new();
        let mut downgrade_paths = Vec::new();

        if let Some(path) = self.upgrade_progress(current, snapshot) {
            upgrade_paths.push(path);
        }

        if let Some(paths) = self.risk_breach_progress(current, snapshot) {
            downgrade_paths.extend(paths);
        }

        if let Some(path) = self.cooldown_progress(current, snapshot) {
            downgrade_paths.push(path);
        }

        LevelProgressReport {
            current_level: current.current_level,
            status: current.status,
            upgrade_paths,
            downgrade_paths,
        }
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
    /// Progress report explaining what is missing to move level up/down.
    pub progress: LevelProgressReport,
}

/// Service coordinating level reads, policy evaluation, and writes.
#[derive(Debug)]
pub struct LevelingService<P: LevelTransitionPolicy> {
    policy: P,
    min_trades_at_level_for_upgrade: u32,
    max_level_changes_per_30_days: u32,
}

impl<P: LevelTransitionPolicy> LevelingService<P> {
    const EVALUATION_WINDOW_DAYS: i64 = 30;

    /// Build service with explicit policy implementation.
    pub fn new(policy: P) -> Self {
        Self {
            policy,
            min_trades_at_level_for_upgrade: 5,
            max_level_changes_per_30_days: 2,
        }
    }

    /// Override stabilization limits for this service instance.
    pub fn with_stabilization_rules(
        mut self,
        min_trades_at_level_for_upgrade: u32,
        max_level_changes_per_30_days: u32,
    ) -> Self {
        self.min_trades_at_level_for_upgrade = min_trades_at_level_for_upgrade;
        self.max_level_changes_per_30_days = max_level_changes_per_30_days;
        self
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
        let progress = self.policy.progress(&current, snapshot);
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
            progress,
        })
    }

    /// Process a closed-trade event, update level trade counters, and apply policy transitions.
    pub fn handle_trade_closed(
        &self,
        factory: &mut dyn DatabaseFactory,
        event: &TradeClosed,
    ) -> Result<LevelEvaluationOutcome, Box<dyn Error>> {
        let mut level = factory.level_read().level_for_account(event.account_id)?;
        level.trades_at_level = level
            .trades_at_level
            .checked_add(1)
            .ok_or_else(|| "Trade count overflow in level progression".to_string())?;
        let _ = factory.level_write().update_level(&level)?;

        let snapshot = Self::build_snapshot_from_recent_closures(factory, event)?;
        let current = factory.level_read().level_for_account(event.account_id)?;
        let raw_decision = self.policy.evaluate(&current, &snapshot);
        let decision =
            self.apply_stabilization_rules(factory, event.account_id, &current, raw_decision)?;
        let progress = self.policy.progress(&current, &snapshot);

        let mut outcome = LevelEvaluationOutcome {
            current_level: current,
            decision,
            applied_level: None,
            progress,
        };

        if let Some(decision) = outcome.decision.clone() {
            let persisted = Self::apply_decision(factory, &outcome.current_level, &decision)?;
            outcome.applied_level = Some(persisted);
        }

        Ok(outcome)
    }

    fn apply_stabilization_rules(
        &self,
        factory: &mut dyn DatabaseFactory,
        account_id: Uuid,
        current: &Level,
        decision: Option<LevelDecision>,
    ) -> Result<Option<LevelDecision>, Box<dyn Error>> {
        let Some(decision) = decision else {
            return Ok(None);
        };

        if decision.direction == LevelDirection::Upgrade
            && current.trades_at_level < self.min_trades_at_level_for_upgrade
        {
            return Ok(None);
        }

        let recent_changes = factory.level_read().recent_level_changes(
            account_id,
            u32::try_from(Self::EVALUATION_WINDOW_DAYS).unwrap_or(30),
        )?;
        let max_changes = usize::try_from(self.max_level_changes_per_30_days)
            .map_err(|_| "invalid max_level_changes_per_30_days".to_string())?;
        if recent_changes.len() >= max_changes {
            return Ok(None);
        }

        Ok(Some(decision))
    }

    fn build_snapshot_from_recent_closures(
        factory: &mut dyn DatabaseFactory,
        event: &TradeClosed,
    ) -> Result<LevelPerformanceSnapshot, Box<dyn Error>> {
        let trade = factory.trade_read().read_trade(event.trade_id)?;
        let baseline = Self::account_balance_baseline(factory, event.account_id, &trade.currency);
        let mut closed =
            Self::collect_recent_closed_trades(factory, event.account_id, event.closed_at)?;
        if closed.is_empty() {
            closed.push(trade);
        }
        closed.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Self::snapshot_from_closed_trades(&closed, baseline)
    }

    fn account_balance_baseline(
        factory: &mut dyn DatabaseFactory,
        account_id: Uuid,
        currency: &model::Currency,
    ) -> Decimal {
        let account_balance = factory
            .account_balance_read()
            .for_currency(account_id, currency)
            .map(|balance| balance.total_balance)
            .unwrap_or(dec!(1));
        if account_balance > Decimal::ZERO {
            account_balance
        } else {
            dec!(1)
        }
    }

    fn collect_recent_closed_trades(
        factory: &mut dyn DatabaseFactory,
        account_id: Uuid,
        closed_at: chrono::NaiveDateTime,
    ) -> Result<Vec<model::Trade>, Box<dyn Error>> {
        let cutoff = closed_at
            .checked_sub_signed(chrono::Duration::days(Self::EVALUATION_WINDOW_DAYS))
            .ok_or_else(|| "Invalid trade close timestamp window".to_string())?;
        let mut closed = factory
            .trade_read()
            .read_trades_with_status(account_id, model::Status::ClosedTarget)?;
        closed.extend(
            factory
                .trade_read()
                .read_trades_with_status(account_id, model::Status::ClosedStopLoss)?,
        );
        closed.retain(|trade| trade.updated_at >= cutoff);
        Ok(closed)
    }

    fn snapshot_from_closed_trades(
        closed: &[model::Trade],
        baseline: Decimal,
    ) -> Result<LevelPerformanceSnapshot, Box<dyn Error>> {
        let profitable_trades_count = closed
            .iter()
            .filter(|trade| trade.balance.total_performance > Decimal::ZERO)
            .count();
        let profitable_trades = u32::try_from(profitable_trades_count)
            .map_err(|_| "profitable trades count overflow".to_string())?;
        let total_trades = Decimal::from(
            u32::try_from(closed.len()).map_err(|_| "total trades overflow".to_string())?,
        );
        let win_rate_percentage = if total_trades > Decimal::ZERO {
            Decimal::from(profitable_trades)
                .checked_div(total_trades)
                .and_then(|ratio| ratio.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        let total_performance = closed.iter().fold(Decimal::ZERO, |acc, trade| {
            acc.checked_add(trade.balance.total_performance)
                .unwrap_or(Decimal::ZERO)
        });
        let monthly_loss_percentage = if total_performance < Decimal::ZERO {
            total_performance
                .checked_div(baseline)
                .and_then(|ratio| ratio.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        let largest_loss = closed
            .iter()
            .map(|trade| trade.balance.total_performance)
            .min()
            .unwrap_or(Decimal::ZERO);
        let largest_loss_percentage = if largest_loss < Decimal::ZERO {
            largest_loss
                .checked_div(baseline)
                .and_then(|ratio| ratio.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };

        let consecutive_wins_count = closed
            .iter()
            .take_while(|trade| trade.balance.total_performance > Decimal::ZERO)
            .count();
        let consecutive_wins = u32::try_from(consecutive_wins_count)
            .map_err(|_| "consecutive wins overflow".to_string())?;

        Ok(LevelPerformanceSnapshot {
            profitable_trades,
            win_rate_percentage,
            monthly_loss_percentage,
            largest_loss_percentage,
            consecutive_wins,
        })
    }

    fn apply_decision(
        factory: &mut dyn DatabaseFactory,
        current: &Level,
        decision: &LevelDecision,
    ) -> Result<Level, Box<dyn Error>> {
        let savepoint = "level_transition";
        factory.begin_savepoint(savepoint)?;
        let now = Utc::now().naive_utc();
        let transition = current.transition_to(
            decision.target_level,
            &decision.reason,
            decision.trigger_type.clone(),
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
        Ok(persisted_level)
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

        let persisted = Self::apply_decision(factory, &outcome.current_level, &decision)?;
        outcome.applied_level = Some(persisted);
        Ok(outcome)
    }
}

impl Default for LevelingService<DefaultLevelTransitionPolicy> {
    fn default() -> Self {
        Self::new(DefaultLevelTransitionPolicy::default())
    }
}

impl Default for DefaultLevelTransitionPolicy {
    fn default() -> Self {
        Self::new(LevelAdjustmentRules::default())
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

        let decision = DefaultLevelTransitionPolicy::default()
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

        let decision = DefaultLevelTransitionPolicy::default()
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

        let decision = DefaultLevelTransitionPolicy::default()
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

        let decision = DefaultLevelTransitionPolicy::default().evaluate(&current, &snapshot);
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

        let decision = DefaultLevelTransitionPolicy::default()
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

        let decision = DefaultLevelTransitionPolicy::default()
            .evaluate(&current, &snapshot)
            .expect("expected recovery decision");
        assert_eq!(decision.direction, LevelDirection::Upgrade);
        assert_eq!(decision.target_level, 3);
        assert_eq!(decision.trigger_type, LevelTrigger::PerformanceUpgrade);
    }

    #[test]
    fn test_progress_reports_missing_upgrade_criteria() {
        let current = Level::default_for_account(Uuid::new_v4());
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 7,
            win_rate_percentage: dec!(64),
            monthly_loss_percentage: dec!(-1),
            largest_loss_percentage: dec!(-0.7),
            consecutive_wins: 1,
        };

        let progress = DefaultLevelTransitionPolicy::default().progress(&current, &snapshot);
        let upgrade = progress
            .upgrade_paths
            .first()
            .expect("upgrade path should be available");
        assert!(!upgrade.all_met);
        assert_eq!(upgrade.path, "performance_upgrade");
        assert_eq!(upgrade.target_level, Some(4));
        assert_eq!(upgrade.criteria.len(), 3);
        let first = upgrade.criteria.first().expect("first criterion");
        let second = upgrade.criteria.get(1).expect("second criterion");
        let third = upgrade.criteria.get(2).expect("third criterion");
        assert_eq!(first.missing, dec!(3));
        assert_eq!(second.missing, dec!(6));
        assert_eq!(third.missing, dec!(2));
    }

    #[test]
    fn test_progress_reports_risk_breach_distance() {
        let current = Level::default_for_account(Uuid::new_v4());
        let snapshot = LevelPerformanceSnapshot {
            profitable_trades: 9,
            win_rate_percentage: dec!(69),
            monthly_loss_percentage: dec!(-4.1),
            largest_loss_percentage: dec!(-1.7),
            consecutive_wins: 2,
        };

        let progress = DefaultLevelTransitionPolicy::default().progress(&current, &snapshot);
        let monthly = progress
            .downgrade_paths
            .iter()
            .find(|path| path.path == "risk_breach_monthly_loss")
            .expect("monthly breach path");
        let largest = progress
            .downgrade_paths
            .iter()
            .find(|path| path.path == "risk_breach_largest_loss")
            .expect("largest breach path");

        assert!(!monthly.all_met);
        let monthly_criterion = monthly.criteria.first().expect("monthly criterion");
        assert_eq!(monthly_criterion.missing, dec!(0.9));
        assert!(!largest.all_met);
        let largest_criterion = largest.criteria.first().expect("largest criterion");
        assert_eq!(largest_criterion.missing, dec!(0.3));
    }
}
