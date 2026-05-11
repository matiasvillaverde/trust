use chrono::NaiveDateTime;
use std::collections::BTreeSet;
use std::str::FromStr;
use uuid::Uuid;

/// Error returned when parsing an invalid session regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRegimeParseError;

/// Pre-session market risk regime selected by the trader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRegime {
    /// Calm market conditions.
    Calm,
    /// Normal market conditions.
    Normal,
    /// Elevated-volatility or elevated-risk market conditions.
    Elevated,
}

impl std::fmt::Display for SessionRegime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            SessionRegime::Calm => "calm",
            SessionRegime::Normal => "normal",
            SessionRegime::Elevated => "elevated",
        };
        write!(f, "{value}")
    }
}

impl FromStr for SessionRegime {
    type Err = SessionRegimeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "calm" => Ok(SessionRegime::Calm),
            "normal" => Ok(SessionRegime::Normal),
            "elevated" => Ok(SessionRegime::Elevated),
            _ => Err(SessionRegimeParseError),
        }
    }
}

/// Error returned when parsing or formatting an invalid setup list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSetupListError {
    /// No setup names were supplied.
    Empty,
    /// A setup name was blank after trimming.
    BlankSetup,
    /// A setup name contained a comma, which is reserved as the storage delimiter.
    CommaInSetup,
    /// A setup name appeared more than once.
    DuplicateSetup,
}

fn normalize_setup(setup: &str) -> Result<String, SessionSetupListError> {
    let normalized = setup.trim();
    if normalized.is_empty() {
        return Err(SessionSetupListError::BlankSetup);
    }
    if normalized.contains(',') {
        return Err(SessionSetupListError::CommaInSetup);
    }
    Ok(normalized.to_string())
}

/// Formats setup names into the stable comma-separated storage representation.
pub fn format_session_setups(setups: &[String]) -> Result<String, SessionSetupListError> {
    if setups.is_empty() {
        return Err(SessionSetupListError::Empty);
    }

    let mut seen = BTreeSet::new();
    let mut normalized_setups = Vec::new();
    for setup in setups {
        let normalized = normalize_setup(setup)?;
        let duplicate_key = normalized.to_ascii_lowercase();
        if !seen.insert(duplicate_key) {
            return Err(SessionSetupListError::DuplicateSetup);
        }
        normalized_setups.push(normalized);
    }

    Ok(normalized_setups.join(","))
}

/// Parses setup names from the stable comma-separated storage representation.
pub fn parse_session_setups(value: &str) -> Result<Vec<String>, SessionSetupListError> {
    if value.trim().is_empty() {
        return Err(SessionSetupListError::Empty);
    }

    let mut seen = BTreeSet::new();
    let mut setups = Vec::new();
    for setup in value.split(',') {
        let normalized = normalize_setup(setup)?;
        let duplicate_key = normalized.to_ascii_lowercase();
        if !seen.insert(duplicate_key) {
            return Err(SessionSetupListError::DuplicateSetup);
        }
        setups.push(normalized);
    }

    Ok(setups)
}

/// Pre-session plan and post-session review for a single trading session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    /// Unique session plan identifier.
    pub id: Uuid,
    /// Session plan record creation timestamp.
    pub created_at: NaiveDateTime,
    /// Session plan record update timestamp.
    pub updated_at: NaiveDateTime,
    /// Soft-delete timestamp for the session plan record.
    pub deleted_at: Option<NaiveDateTime>,
    /// Account this plan belongs to.
    pub account_id: Uuid,
    /// Time the plan was opened. Plan fields are immutable after this point.
    pub opened_at: NaiveDateTime,
    /// Time the plan was closed, if the session review has been completed.
    pub closed_at: Option<NaiveDateTime>,
    /// Market regime selected before the session.
    pub regime: SessionRegime,
    /// Setups permitted for the session.
    pub permitted_setups: Vec<String>,
    /// Maximum number of simultaneous positions allowed by the plan.
    pub max_positions: i32,
    /// Pre-session market and behavior hypothesis. Limited to 500 Unicode scalar values.
    pub hypothesis: String,
    /// What must happen for the session to count as successful.
    pub success_criteria: String,
    /// What must happen for the session to count as failed.
    pub failure_criteria: String,
    /// Optional grade assigned when the session is closed.
    pub session_grade: Option<String>,
    /// Optional adherence notes written when the session is closed.
    pub adherence_notes: Option<String>,
}

/// Review data applied when closing an open session plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlanClose {
    /// Session plan being closed.
    pub session_plan_id: Uuid,
    /// Session close timestamp.
    pub closed_at: NaiveDateTime,
    /// Optional grade assigned at close.
    pub session_grade: Option<String>,
    /// Optional adherence notes written at close.
    pub adherence_notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        format_session_setups, parse_session_setups, SessionRegime, SessionRegimeParseError,
        SessionSetupListError,
    };

    #[test]
    fn session_regime_display_and_parse_roundtrip() {
        for (regime, text) in [
            (SessionRegime::Calm, "calm"),
            (SessionRegime::Normal, "normal"),
            (SessionRegime::Elevated, "elevated"),
        ] {
            assert_eq!(regime.to_string(), text);
            assert_eq!(text.parse::<SessionRegime>(), Ok(regime));
            assert_eq!(
                text.to_ascii_uppercase().parse::<SessionRegime>(),
                Ok(regime)
            );
            assert_eq!(format!(" {text} ").parse::<SessionRegime>(), Ok(regime));
        }

        assert_eq!(
            "volatile".parse::<SessionRegime>(),
            Err(SessionRegimeParseError)
        );
    }

    #[test]
    fn session_setup_list_formats_and_parses_stably() {
        let setups = vec![
            " opening range ".to_string(),
            "pullback".to_string(),
            "trend day".to_string(),
        ];

        let formatted = format_session_setups(&setups).unwrap();
        assert_eq!(formatted, "opening range,pullback,trend day");
        assert_eq!(
            parse_session_setups(&formatted).unwrap(),
            vec![
                "opening range".to_string(),
                "pullback".to_string(),
                "trend day".to_string()
            ]
        );
    }

    #[test]
    fn session_setup_list_rejects_invalid_values() {
        assert_eq!(
            format_session_setups(&[]),
            Err(SessionSetupListError::Empty)
        );
        assert_eq!(parse_session_setups(""), Err(SessionSetupListError::Empty));
        assert_eq!(
            parse_session_setups("breakout,,pullback"),
            Err(SessionSetupListError::BlankSetup)
        );
        assert_eq!(
            format_session_setups(&["break,out".to_string()]),
            Err(SessionSetupListError::CommaInSetup)
        );
        assert_eq!(
            parse_session_setups("breakout,BREAKOUT"),
            Err(SessionSetupListError::DuplicateSetup)
        );
    }
}
