use chrono::NaiveDateTime;
use uuid::Uuid;

/// Error returned when parsing an invalid grade string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradeParseError;

/// Letter grade representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    APlus,
    A,
    AMinus,
    BPlus,
    B,
    BMinus,
    CPlus,
    C,
    CMinus,
    D,
    F,
}

impl Grade {
    /// Convert a 0-100 score to a grade.
    pub fn from_score(score: u8) -> Grade {
        match score {
            97..=100 => Grade::APlus,
            93..=96 => Grade::A,
            90..=92 => Grade::AMinus,
            87..=89 => Grade::BPlus,
            83..=86 => Grade::B,
            80..=82 => Grade::BMinus,
            77..=79 => Grade::CPlus,
            73..=76 => Grade::C,
            70..=72 => Grade::CMinus,
            60..=69 => Grade::D,
            _ => Grade::F,
        }
    }
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Grade::APlus => "A+",
            Grade::A => "A",
            Grade::AMinus => "A-",
            Grade::BPlus => "B+",
            Grade::B => "B",
            Grade::BMinus => "B-",
            Grade::CPlus => "C+",
            Grade::C => "C",
            Grade::CMinus => "C-",
            Grade::D => "D",
            Grade::F => "F",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for Grade {
    type Err = GradeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "A+" => Ok(Grade::APlus),
            "A" => Ok(Grade::A),
            "A-" => Ok(Grade::AMinus),
            "B+" => Ok(Grade::BPlus),
            "B" => Ok(Grade::B),
            "B-" => Ok(Grade::BMinus),
            "C+" => Ok(Grade::CPlus),
            "C" => Ok(Grade::C),
            "C-" => Ok(Grade::CMinus),
            "D" => Ok(Grade::D),
            "F" => Ok(Grade::F),
            _ => Err(GradeParseError),
        }
    }
}

/// Trade grade entity stored in DB.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeGrade {
    pub id: Uuid,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    pub trade_id: Uuid,
    pub overall_score: u8,
    pub overall_grade: Grade,
    pub process_score: u8,
    pub risk_score: u8,
    pub execution_score: u8,
    pub documentation_score: u8,
    pub recommendations: Vec<String>,
    pub graded_at: NaiveDateTime,

    /// Weights used to compute the overall score (permille, sum=1000).
    pub process_weight_permille: u16,
    pub risk_weight_permille: u16,
    pub execution_weight_permille: u16,
    pub documentation_weight_permille: u16,
}
