use chrono::{NaiveDate, NaiveDateTime};
use std::str::FromStr;
use uuid::Uuid;

/// Error returned when parsing an invalid trade event type string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeEventTypeParseError;

/// Scheduled or discretionary catalyst associated with a trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeEventType {
    /// Company earnings report.
    Earnings,
    /// Federal Reserve decision, minutes, speech, or related policy event.
    Fed,
    /// Consumer Price Index release.
    Cpi,
    /// Nonfarm Payrolls release.
    Nfp,
    /// Ex-dividend date.
    ExDividend,
    /// Company guidance update.
    Guidance,
    /// Any catalyst that does not fit the known categories.
    Other,
}

impl std::fmt::Display for TradeEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let event_type = match self {
            TradeEventType::Earnings => "earnings",
            TradeEventType::Fed => "fed",
            TradeEventType::Cpi => "cpi",
            TradeEventType::Nfp => "nfp",
            TradeEventType::ExDividend => "ex_dividend",
            TradeEventType::Guidance => "guidance",
            TradeEventType::Other => "other",
        };
        write!(f, "{event_type}")
    }
}

impl FromStr for TradeEventType {
    type Err = TradeEventTypeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "earnings" => Ok(TradeEventType::Earnings),
            "fed" => Ok(TradeEventType::Fed),
            "cpi" => Ok(TradeEventType::Cpi),
            "nfp" => Ok(TradeEventType::Nfp),
            "ex_dividend" | "ex-dividend" => Ok(TradeEventType::ExDividend),
            "guidance" => Ok(TradeEventType::Guidance),
            "other" => Ok(TradeEventType::Other),
            _ => Err(TradeEventTypeParseError),
        }
    }
}

/// Error returned when parsing an invalid trade event severity string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeEventSeverityParseError;

/// Risk impact of a trade event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeEventSeverity {
    /// Low expected impact.
    Low,
    /// Medium expected impact.
    Medium,
    /// High expected impact.
    High,
}

impl std::fmt::Display for TradeEventSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self {
            TradeEventSeverity::Low => "low",
            TradeEventSeverity::Medium => "medium",
            TradeEventSeverity::High => "high",
        };
        write!(f, "{severity}")
    }
}

impl FromStr for TradeEventSeverity {
    type Err = TradeEventSeverityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(TradeEventSeverity::Low),
            "medium" => Ok(TradeEventSeverity::Medium),
            "high" => Ok(TradeEventSeverity::High),
            _ => Err(TradeEventSeverityParseError),
        }
    }
}

/// Error returned when parsing an invalid trade event source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeEventSourceParseError;

/// Source of a trade event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeEventSource {
    /// User-entered event.
    Manual,
    /// Imported from a market calendar API.
    CalendarApi,
}

impl std::fmt::Display for TradeEventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source = match self {
            TradeEventSource::Manual => "manual",
            TradeEventSource::CalendarApi => "calendar_api",
        };
        write!(f, "{source}")
    }
}

impl FromStr for TradeEventSource {
    type Err = TradeEventSourceParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "manual" => Ok(TradeEventSource::Manual),
            "calendar_api" | "calendarapi" | "calendar-api" => Ok(TradeEventSource::CalendarApi),
            _ => Err(TradeEventSourceParseError),
        }
    }
}

/// Trade event entity stored in DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeEvent {
    /// Unique event record identifier.
    pub id: Uuid,
    /// Event record creation timestamp.
    pub created_at: NaiveDateTime,
    /// Event record update timestamp.
    pub updated_at: NaiveDateTime,
    /// Soft-delete timestamp for the event record.
    pub deleted_at: Option<NaiveDateTime>,

    /// Associated trade identifier.
    pub trade_id: Uuid,
    /// Symbol affected by the event.
    pub symbol: String,
    /// Event category.
    pub event_type: TradeEventType,
    /// Calendar date of the event.
    pub event_date: NaiveDate,
    /// Expected risk impact of the event.
    pub severity: TradeEventSeverity,
    /// Optional user or source notes.
    pub notes: Option<String>,
    /// Source of the event.
    pub source: TradeEventSource,
}

#[cfg(test)]
mod tests {
    use super::{
        TradeEventSeverity, TradeEventSeverityParseError, TradeEventSource,
        TradeEventSourceParseError, TradeEventType, TradeEventTypeParseError,
    };

    #[test]
    fn event_type_display_and_parse_roundtrip_all_variants() {
        let cases = [
            (TradeEventType::Earnings, "earnings"),
            (TradeEventType::Fed, "fed"),
            (TradeEventType::Cpi, "cpi"),
            (TradeEventType::Nfp, "nfp"),
            (TradeEventType::ExDividend, "ex_dividend"),
            (TradeEventType::Guidance, "guidance"),
            (TradeEventType::Other, "other"),
        ];

        for (event_type, text) in cases {
            assert_eq!(event_type.to_string(), text);
            assert_eq!(text.parse::<TradeEventType>(), Ok(event_type));
            assert_eq!(
                format!(" {text} ").parse::<TradeEventType>(),
                Ok(event_type)
            );
        }
        assert_eq!(
            "dividend".parse::<TradeEventType>(),
            Err(TradeEventTypeParseError)
        );
    }

    #[test]
    fn event_type_parse_accepts_calendar_aliases() {
        assert_eq!(
            "ex-dividend".parse::<TradeEventType>(),
            Ok(TradeEventType::ExDividend)
        );
        assert_eq!("CPI".parse::<TradeEventType>(), Ok(TradeEventType::Cpi));
        assert_eq!("NFP".parse::<TradeEventType>(), Ok(TradeEventType::Nfp));
    }

    #[test]
    fn severity_display_and_parse_roundtrip_all_variants() {
        let cases = [
            (TradeEventSeverity::Low, "low"),
            (TradeEventSeverity::Medium, "medium"),
            (TradeEventSeverity::High, "high"),
        ];

        for (severity, text) in cases {
            assert_eq!(severity.to_string(), text);
            assert_eq!(text.parse::<TradeEventSeverity>(), Ok(severity));
            assert_eq!(
                text.to_ascii_uppercase().parse::<TradeEventSeverity>(),
                Ok(severity)
            );
        }
        assert_eq!(
            "critical".parse::<TradeEventSeverity>(),
            Err(TradeEventSeverityParseError)
        );
    }

    #[test]
    fn source_display_and_parse_roundtrip_all_variants() {
        let cases = [
            (TradeEventSource::Manual, "manual"),
            (TradeEventSource::CalendarApi, "calendar_api"),
        ];

        for (source, text) in cases {
            assert_eq!(source.to_string(), text);
            assert_eq!(text.parse::<TradeEventSource>(), Ok(source));
        }
        assert_eq!(
            "calendar-api".parse::<TradeEventSource>(),
            Ok(TradeEventSource::CalendarApi)
        );
        assert_eq!(
            "import".parse::<TradeEventSource>(),
            Err(TradeEventSourceParseError)
        );
    }
}
