use chrono::{Duration, Utc};
use model::{DatabaseFactory, Level, LevelChange, LevelTrigger};
use std::error::Error;
use tracing::{debug, info};
use uuid::Uuid;

const IDEMPOTENCY_WINDOW_SECONDS: i64 = 120;

/// Applies a manual level change atomically and records the audit event.
/// If the exact same change was already applied recently, this operation is idempotent.
pub fn change(
    database: &mut dyn DatabaseFactory,
    account_id: Uuid,
    target_level: u8,
    reason: &str,
    trigger_type: LevelTrigger,
) -> Result<(Level, LevelChange), Box<dyn Error>> {
    let current = database.level_read().level_for_account(account_id)?;
    let reason = reason.trim();
    let trigger_type = normalize_trigger(trigger_type)?;
    let now = Utc::now().naive_utc();

    if current.current_level == target_level {
        let recent = database.level_read().recent_level_changes(account_id, 1)?;
        if let Some(latest) = recent.first() {
            if is_idempotent_retry(latest, target_level, reason, &trigger_type, now) {
                info!(
                    account_id = %account_id,
                    level = target_level,
                    trigger = %trigger_type,
                    "idempotent manual level change retry detected"
                );
                return Ok((current, latest.clone()));
            }
        }
    }

    let (updated, event) = current.transition_to(target_level, reason, trigger_type, now)?;
    with_savepoint(database, "manual_level_change", |db| {
        let persisted = db.level_write().update_level(&updated)?;
        let persisted_change = db.level_write().create_level_change(&event)?;
        debug!(
            account_id = %account_id,
            from_level = event.old_level,
            to_level = event.new_level,
            trigger = %event.trigger_type,
            "manual level change persisted"
        );
        Ok((persisted, persisted_change))
    })
}

fn with_savepoint<T>(
    database: &mut dyn DatabaseFactory,
    name: &str,
    operation: impl FnOnce(&mut dyn DatabaseFactory) -> Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>> {
    database.begin_savepoint(name)?;
    match operation(database) {
        Ok(value) => {
            database.release_savepoint(name)?;
            Ok(value)
        }
        Err(operation_error) => {
            let rollback_error = database.rollback_to_savepoint(name).err();
            let release_error = database.release_savepoint(name).err();
            if rollback_error.is_none() && release_error.is_none() {
                return Err(operation_error);
            }

            let mut message = format!(
                "operation failed inside savepoint '{name}': {}",
                operation_error
            );
            if let Some(error) = rollback_error {
                message.push_str(&format!("; rollback failed: {error}"));
            }
            if let Some(error) = release_error {
                message.push_str(&format!("; release failed: {error}"));
            }
            Err(message.into())
        }
    }
}

fn normalize_trigger(trigger_type: LevelTrigger) -> Result<LevelTrigger, Box<dyn Error>> {
    match trigger_type {
        LevelTrigger::Custom(value) => {
            let normalized = value.trim().to_lowercase();
            if normalized.is_empty() {
                return Err("Level change trigger cannot be empty".into());
            }
            Ok(LevelTrigger::Custom(normalized))
        }
        value => Ok(value),
    }
}

fn is_idempotent_retry(
    latest: &LevelChange,
    target_level: u8,
    reason: &str,
    trigger_type: &LevelTrigger,
    now: chrono::NaiveDateTime,
) -> bool {
    latest.new_level == target_level
        && latest.change_reason == reason
        && latest.trigger_type == *trigger_type
        && now.signed_duration_since(latest.changed_at)
            <= Duration::seconds(IDEMPOTENCY_WINDOW_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{LevelDirection, LevelStatus};
    use rust_decimal_macros::dec;

    #[test]
    fn test_idempotent_retry_match() {
        let now = Utc::now().naive_utc();
        let change = LevelChange {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: Uuid::new_v4(),
            old_level: 3,
            new_level: 2,
            change_reason: "risk review".to_string(),
            trigger_type: LevelTrigger::ManualReview,
            changed_at: now,
        };
        assert!(is_idempotent_retry(
            &change,
            2,
            "risk review",
            &LevelTrigger::ManualReview,
            now
        ));
    }

    #[test]
    fn test_idempotent_retry_rejects_old_or_mismatch() {
        let now = Utc::now().naive_utc();
        let old = now - Duration::seconds(IDEMPOTENCY_WINDOW_SECONDS + 1);
        let change = LevelChange {
            id: Uuid::new_v4(),
            created_at: old,
            updated_at: old,
            deleted_at: None,
            account_id: Uuid::new_v4(),
            old_level: 2,
            new_level: 3,
            change_reason: "upgrade".to_string(),
            trigger_type: LevelTrigger::ManualOverride,
            changed_at: old,
        };

        assert!(!is_idempotent_retry(
            &change,
            3,
            "upgrade",
            &LevelTrigger::ManualOverride,
            now
        ));
        assert!(!is_idempotent_retry(
            &change,
            3,
            "different",
            &LevelTrigger::ManualOverride,
            old
        ));
    }

    #[test]
    fn test_normalize_trigger_custom() {
        let trigger = normalize_trigger(LevelTrigger::Custom("  AbC  ".to_string())).unwrap();
        assert_eq!(trigger, LevelTrigger::Custom("abc".to_string()));
    }

    #[test]
    fn test_level_transition_direction_unchanged_guard() {
        let mut level = Level::default_for_account(Uuid::new_v4());
        level.current_level = 3;
        level.risk_multiplier = dec!(1.00);
        let direction = level.direction_to(2).unwrap();
        assert_eq!(direction, LevelDirection::Downgrade);
        assert_eq!(level.status, LevelStatus::Normal);
    }
}
