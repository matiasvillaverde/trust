use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

/// Error returned when parsing an invalid Munger tendency tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MungerTendencyParseError;

/// Error returned when parsing an invalid comma-separated tendency tag list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MungerTendencyListParseError;

/// Trading-relevant Munger tendency tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MungerTendency {
    /// #1 Reward/Punishment Superresponse.
    RewardPunishment,
    /// #5 Inconsistency-Avoidance.
    InconsistencyAvoidance,
    /// #10 Mere-Association.
    MereAssociation,
    /// #11 Pain-Avoiding Psychological Denial.
    PainAvoidingDenial,
    /// #12 Excessive Self-Regard.
    ExcessiveSelfRegard,
    /// #13 Overoptimism.
    Overoptimism,
    /// #14 Deprival Superreaction.
    DeprivalSuperreaction,
    /// #15 Social-Proof.
    SocialProof,
    /// #16 Contrast-Misreaction.
    ContrastMisreaction,
    /// #17 Stress-Influence.
    StressInfluence,
    /// #18 Availability-Misweighing.
    AvailabilityMisweighing,
    /// #22 Authority-Misinfluence.
    AuthorityMisinfluence,
    /// #25 Lollapalooza tendency.
    Lollapalooza,
}

impl MungerTendency {
    /// Return all supported trading-relevant tendency tags.
    pub fn all() -> [MungerTendency; 13] {
        [
            MungerTendency::RewardPunishment,
            MungerTendency::InconsistencyAvoidance,
            MungerTendency::MereAssociation,
            MungerTendency::PainAvoidingDenial,
            MungerTendency::ExcessiveSelfRegard,
            MungerTendency::Overoptimism,
            MungerTendency::DeprivalSuperreaction,
            MungerTendency::SocialProof,
            MungerTendency::ContrastMisreaction,
            MungerTendency::StressInfluence,
            MungerTendency::AvailabilityMisweighing,
            MungerTendency::AuthorityMisinfluence,
            MungerTendency::Lollapalooza,
        ]
    }

    /// Return the canonical Munger tendency number.
    pub fn number(self) -> u8 {
        match self {
            MungerTendency::RewardPunishment => 1,
            MungerTendency::InconsistencyAvoidance => 5,
            MungerTendency::MereAssociation => 10,
            MungerTendency::PainAvoidingDenial => 11,
            MungerTendency::ExcessiveSelfRegard => 12,
            MungerTendency::Overoptimism => 13,
            MungerTendency::DeprivalSuperreaction => 14,
            MungerTendency::SocialProof => 15,
            MungerTendency::ContrastMisreaction => 16,
            MungerTendency::StressInfluence => 17,
            MungerTendency::AvailabilityMisweighing => 18,
            MungerTendency::AuthorityMisinfluence => 22,
            MungerTendency::Lollapalooza => 25,
        }
    }
}

impl std::fmt::Display for MungerTendency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.number())
    }
}

impl FromStr for MungerTendency {
    type Err = MungerTendencyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "reward_punishment" | "reward-punishment" => Ok(MungerTendency::RewardPunishment),
            "5" | "inconsistency_avoidance" | "inconsistency-avoidance" => {
                Ok(MungerTendency::InconsistencyAvoidance)
            }
            "10" | "mere_association" | "mere-association" => Ok(MungerTendency::MereAssociation),
            "11" | "pain_avoiding_denial" | "pain-avoiding-denial" => {
                Ok(MungerTendency::PainAvoidingDenial)
            }
            "12" | "excessive_self_regard" | "excessive-self-regard" => {
                Ok(MungerTendency::ExcessiveSelfRegard)
            }
            "13" | "overoptimism" => Ok(MungerTendency::Overoptimism),
            "14" | "deprival_superreaction" | "deprival-superreaction" => {
                Ok(MungerTendency::DeprivalSuperreaction)
            }
            "15" | "social_proof" | "social-proof" => Ok(MungerTendency::SocialProof),
            "16" | "contrast_misreaction" | "contrast-misreaction" => {
                Ok(MungerTendency::ContrastMisreaction)
            }
            "17" | "stress_influence" | "stress-influence" => Ok(MungerTendency::StressInfluence),
            "18" | "availability_misweighing" | "availability-misweighing" => {
                Ok(MungerTendency::AvailabilityMisweighing)
            }
            "22" | "authority_misinfluence" | "authority-misinfluence" => {
                Ok(MungerTendency::AuthorityMisinfluence)
            }
            "25" | "lollapalooza" => Ok(MungerTendency::Lollapalooza),
            _ => Err(MungerTendencyParseError),
        }
    }
}

/// Format tendency tags as the stable comma-separated database representation.
pub fn format_munger_tendencies(tags: &[MungerTendency]) -> String {
    tags.iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Parse comma-separated Munger tendency numbers.
pub fn parse_munger_tendencies(
    value: &str,
) -> Result<Vec<MungerTendency>, MungerTendencyListParseError> {
    let mut parsed = Vec::new();
    for token in value.split(',') {
        let tendency = token
            .parse::<MungerTendency>()
            .map_err(|_| MungerTendencyListParseError)?;
        if parsed.contains(&tendency) {
            return Err(MungerTendencyListParseError);
        }
        parsed.push(tendency);
    }

    if parsed.is_empty() {
        return Err(MungerTendencyListParseError);
    }

    Ok(parsed)
}

/// Error returned when parsing an invalid mistake error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MistakeErrorTypeParseError;

/// Whether the mistake was an act of commission or omission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MistakeErrorType {
    /// A trade/action was taken that should not have been taken.
    Commission,
    /// A trade/action was skipped that should have been taken.
    Omission,
}

impl std::fmt::Display for MistakeErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_type = match self {
            MistakeErrorType::Commission => "commission",
            MistakeErrorType::Omission => "omission",
        };
        write!(f, "{error_type}")
    }
}

impl FromStr for MistakeErrorType {
    type Err = MistakeErrorTypeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "commission" => Ok(MistakeErrorType::Commission),
            "omission" => Ok(MistakeErrorType::Omission),
            _ => Err(MistakeErrorTypeParseError),
        }
    }
}

/// Structured post-trade bias analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Mistake {
    /// Unique mistake record identifier.
    pub id: Uuid,
    /// Mistake record creation timestamp.
    pub created_at: NaiveDateTime,
    /// Mistake record update timestamp.
    pub updated_at: NaiveDateTime,
    /// Soft-delete timestamp for the mistake record.
    pub deleted_at: Option<NaiveDateTime>,

    /// Associated trade identifier.
    pub trade_id: Uuid,
    /// Munger tendency tags active in the decision.
    pub bias_tags: Vec<MungerTendency>,
    /// Whether multiple tendencies reinforced each other.
    pub lollapalooza: bool,
    /// Commission or omission classification.
    pub error_type: MistakeErrorType,
    /// Optional process rule that was violated.
    pub rule_violated: Option<String>,
    /// Counterfactual R-multiple had the rule been followed.
    pub counterfactual_r: Decimal,
    /// One-sentence takeaway.
    pub lesson: String,
}

#[cfg(test)]
mod tests {
    use super::{
        format_munger_tendencies, parse_munger_tendencies, MistakeErrorType,
        MistakeErrorTypeParseError, MungerTendency, MungerTendencyListParseError,
        MungerTendencyParseError,
    };

    #[test]
    fn munger_tendency_display_and_parse_roundtrip_all_variants() {
        let cases = [
            (MungerTendency::RewardPunishment, "1"),
            (MungerTendency::InconsistencyAvoidance, "5"),
            (MungerTendency::MereAssociation, "10"),
            (MungerTendency::PainAvoidingDenial, "11"),
            (MungerTendency::ExcessiveSelfRegard, "12"),
            (MungerTendency::Overoptimism, "13"),
            (MungerTendency::DeprivalSuperreaction, "14"),
            (MungerTendency::SocialProof, "15"),
            (MungerTendency::ContrastMisreaction, "16"),
            (MungerTendency::StressInfluence, "17"),
            (MungerTendency::AvailabilityMisweighing, "18"),
            (MungerTendency::AuthorityMisinfluence, "22"),
            (MungerTendency::Lollapalooza, "25"),
        ];

        assert_eq!(MungerTendency::all().len(), cases.len());
        for (tendency, text) in cases {
            assert_eq!(tendency.to_string(), text);
            assert_eq!(text.parse::<MungerTendency>(), Ok(tendency));
            assert_eq!(format!(" {text} ").parse::<MungerTendency>(), Ok(tendency));
        }
        assert_eq!("2".parse::<MungerTendency>(), Err(MungerTendencyParseError));
    }

    #[test]
    fn munger_tendency_parse_accepts_readable_aliases() {
        assert_eq!(
            "reward-punishment".parse::<MungerTendency>(),
            Ok(MungerTendency::RewardPunishment)
        );
        assert_eq!(
            "SOCIAL_PROOF".parse::<MungerTendency>(),
            Ok(MungerTendency::SocialProof)
        );
        assert_eq!(
            "availability-misweighing".parse::<MungerTendency>(),
            Ok(MungerTendency::AvailabilityMisweighing)
        );
    }

    #[test]
    fn munger_tendency_list_roundtrips_and_rejects_bad_lists() {
        let tags = vec![
            MungerTendency::InconsistencyAvoidance,
            MungerTendency::DeprivalSuperreaction,
            MungerTendency::Lollapalooza,
        ];

        let formatted = format_munger_tendencies(&tags);

        assert_eq!(formatted, "5,14,25");
        assert_eq!(parse_munger_tendencies(&formatted), Ok(tags));
        assert_eq!(
            parse_munger_tendencies("5,5"),
            Err(MungerTendencyListParseError)
        );
        assert_eq!(
            parse_munger_tendencies("5,2"),
            Err(MungerTendencyListParseError)
        );
        assert_eq!(
            parse_munger_tendencies(""),
            Err(MungerTendencyListParseError)
        );
    }

    #[test]
    fn mistake_error_type_display_and_parse_roundtrip() {
        let cases = [
            (MistakeErrorType::Commission, "commission"),
            (MistakeErrorType::Omission, "omission"),
        ];

        for (error_type, text) in cases {
            assert_eq!(error_type.to_string(), text);
            assert_eq!(text.parse::<MistakeErrorType>(), Ok(error_type));
            assert_eq!(
                text.to_ascii_uppercase().parse::<MistakeErrorType>(),
                Ok(error_type)
            );
        }
        assert_eq!(
            "accident".parse::<MistakeErrorType>(),
            Err(MistakeErrorTypeParseError)
        );
    }
}
