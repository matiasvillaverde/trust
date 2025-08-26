use chrono::{NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use uuid::Uuid;

/// Level entity representing account trading level and risk multiplier
#[derive(PartialEq, Debug, Clone)]
pub struct Level {
    /// Unique identifier for the level
    pub id: Uuid,
    /// When the level was created
    pub created_at: NaiveDateTime,
    /// When the level was last updated
    pub updated_at: NaiveDateTime,
    /// When the level was deleted, if applicable
    pub deleted_at: Option<NaiveDateTime>,
    /// ID of the account this level belongs to
    pub account_id: Uuid,
    /// Current level (0-4)
    pub current_level: u8,
    /// Risk multiplier for position sizing
    pub risk_multiplier: Decimal,
    /// Current status of the level
    pub status: LevelStatus,
    /// Number of trades at current level
    pub trades_at_level: u32,
    /// Date when current level started
    pub level_start_date: NaiveDate,
}

/// Level change audit record
#[derive(PartialEq, Debug, Clone)]
pub struct LevelChange {
    /// Unique identifier for the level change
    pub id: Uuid,
    /// When the change was created
    pub created_at: NaiveDateTime,
    /// When the change was last updated
    pub updated_at: NaiveDateTime,
    /// When the change was deleted, if applicable
    pub deleted_at: Option<NaiveDateTime>,
    /// ID of the account this change applies to
    pub account_id: Uuid,
    /// Previous level
    pub old_level: u8,
    /// New level after change
    pub new_level: u8,
    /// Human-readable reason for the change
    pub change_reason: String,
    /// Type of trigger that caused the change
    pub trigger_type: String,
    /// When the change occurred
    pub changed_at: NaiveDateTime,
}

/// Status of a trading level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LevelStatus {
    /// Normal trading status
    Normal,
    /// On probation due to performance issues
    Probation,
    /// Cooldown period after level change
    Cooldown,
}

/// Level multiplier constants for position sizing
pub const LEVEL_MULTIPLIERS: [Decimal; 5] = [
    Decimal::from_parts(1, 0, 0, false, 1),  // 0.1 (10%)
    Decimal::from_parts(25, 0, 0, false, 2), // 0.25 (25%)
    Decimal::from_parts(5, 0, 0, false, 1),  // 0.5 (50%)
    Decimal::from_parts(1, 0, 0, false, 0),  // 1.0 (100%)
    Decimal::from_parts(15, 0, 0, false, 1), // 1.5 (150%)
];

impl Level {
    /// Get the risk multiplier for a given level
    pub fn multiplier_for_level(level: u8) -> Result<Decimal, String> {
        if level > 4 {
            return Err(format!("Invalid level: {level}. Must be 0-4"));
        }
        LEVEL_MULTIPLIERS
            .get(level as usize)
            .copied()
            .ok_or_else(|| format!("Invalid level index: {level}"))
    }

    /// Get human-readable level description
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
}

impl Default for Level {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        let today = now.date();
        Level {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: Uuid::new_v4(),
            current_level: 3, // Default to Level 3 (Full Size Trading)
            risk_multiplier: LEVEL_MULTIPLIERS[3], // 1.0x
            status: LevelStatus::Normal,
            trades_at_level: 0,
            level_start_date: today,
        }
    }
}

impl Default for LevelChange {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        LevelChange {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: Uuid::new_v4(),
            old_level: 0,
            new_level: 3,
            change_reason: "Initial level assignment".to_string(),
            trigger_type: "account_creation".to_string(),
            changed_at: now,
        }
    }
}

impl LevelStatus {
    /// Returns all possible status values
    pub fn all() -> Vec<LevelStatus> {
        vec![
            LevelStatus::Normal,
            LevelStatus::Probation,
            LevelStatus::Cooldown,
        ]
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Level {} ({}) - {}x multiplier - {}",
            self.current_level,
            Level::level_description(self.current_level),
            self.risk_multiplier,
            self.status
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

/// Error when parsing level status from string fails
#[derive(Debug, Clone, Copy)]
pub struct LevelStatusParseError;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_multipliers() {
        assert_eq!(Level::multiplier_for_level(0).unwrap(), Decimal::new(1, 1)); // 0.1
        assert_eq!(Level::multiplier_for_level(1).unwrap(), Decimal::new(25, 2)); // 0.25
        assert_eq!(Level::multiplier_for_level(2).unwrap(), Decimal::new(5, 1)); // 0.5
        assert_eq!(Level::multiplier_for_level(3).unwrap(), Decimal::new(1, 0)); // 1.0
        assert_eq!(Level::multiplier_for_level(4).unwrap(), Decimal::new(15, 1));
        // 1.5
    }

    #[test]
    fn test_invalid_level() {
        assert!(Level::multiplier_for_level(5).is_err());
    }

    #[test]
    fn test_level_descriptions() {
        assert_eq!(Level::level_description(0), "Restricted Trading");
        assert_eq!(Level::level_description(3), "Full Size Trading");
        assert_eq!(Level::level_description(4), "Enhanced Trading");
    }

    #[test]
    fn test_level_status_display() {
        assert_eq!(LevelStatus::Normal.to_string(), "normal");
        assert_eq!(LevelStatus::Probation.to_string(), "probation");
        assert_eq!(LevelStatus::Cooldown.to_string(), "cooldown");
    }

    #[test]
    fn test_level_status_from_str() {
        assert_eq!(
            "normal".parse::<LevelStatus>().unwrap(),
            LevelStatus::Normal
        );
        assert_eq!(
            "probation".parse::<LevelStatus>().unwrap(),
            LevelStatus::Probation
        );
        assert_eq!(
            "cooldown".parse::<LevelStatus>().unwrap(),
            LevelStatus::Cooldown
        );
        assert!("invalid".parse::<LevelStatus>().is_err());
    }

    #[test]
    fn test_default_level() {
        let level = Level::default();
        assert_eq!(level.current_level, 3);
        assert_eq!(level.risk_multiplier, Decimal::new(1, 0));
        assert_eq!(level.status, LevelStatus::Normal);
        assert_eq!(level.trades_at_level, 0);
    }

    #[test]
    fn test_level_display() {
        let level = Level::default();
        let display = level.to_string();
        assert!(display.contains("Level 3"));
        assert!(display.contains("Full Size Trading"));
        assert!(display.contains("1"));
        assert!(display.contains("normal"));
    }
}
