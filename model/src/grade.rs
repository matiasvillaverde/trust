//! Trade grading domain models for evaluating trading performance quality
//!
//! This module defines the core types for the trade grading system which evaluates
//! completed trades on process adherence, risk management, execution quality, and documentation.

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Letter grade representation (A, B, C, D, F)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    /// Excellent performance (90-100)
    A,
    /// Good performance (80-89)
    B,
    /// Fair performance (70-79)
    C,
    /// Poor performance (60-69)
    D,
    /// Failing performance (<60)
    F,
}

impl Grade {
    /// Convert grade to numeric value for comparison (higher is better)
    fn to_numeric(self) -> u8 {
        match self {
            Grade::A => 4,
            Grade::B => 3,
            Grade::C => 2,
            Grade::D => 1,
            Grade::F => 0,
        }
    }
}

impl PartialOrd for Grade {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Grade {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_numeric().cmp(&other.to_numeric())
    }
}

/// Configurable weights for different grading components
#[derive(Debug, Clone, PartialEq)]
pub struct GradingWeights {
    /// Weight for process adherence component (default: 0.40)
    pub process_weight: Decimal,
    /// Weight for risk management component (default: 0.30)
    pub risk_weight: Decimal,
    /// Weight for execution quality component (default: 0.20)
    pub execution_weight: Decimal,
    /// Weight for documentation component (default: 0.10)
    pub documentation_weight: Decimal,
}

/// Complete grading evaluation for a single trade
#[derive(Debug, Clone, PartialEq)]
pub struct TradeGrade {
    /// Unique identifier for this grade
    pub id: Uuid,
    /// Trade that was graded
    pub trade_id: Uuid,
    /// Overall score (0-100)
    pub overall_score: u8,
    /// Overall letter grade
    pub overall_grade: Grade,
    /// Process adherence score (0-100)
    pub process_score: u8,
    /// Risk management score (0-100)
    pub risk_score: u8,
    /// Execution quality score (0-100)
    pub execution_score: u8,
    /// Documentation score (0-100)
    pub documentation_score: u8,
    /// Actionable recommendations for improvement
    pub recommendations: Vec<String>,
    /// When this grade was calculated
    pub graded_at: NaiveDateTime,
    /// When this grade record was created
    pub created_at: NaiveDateTime,
    /// When this grade record was last updated
    pub updated_at: NaiveDateTime,
    /// Soft delete timestamp
    pub deleted_at: Option<NaiveDateTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[test]
    fn test_grade_ordering() {
        assert!(Grade::A > Grade::B);
        assert!(Grade::B > Grade::C);
        assert!(Grade::C > Grade::D);
        assert!(Grade::D > Grade::F);
    }

    #[test]
    fn test_grading_weights_creation() {
        let weights = GradingWeights {
            process_weight: dec!(0.40),
            risk_weight: dec!(0.30),
            execution_weight: dec!(0.20),
            documentation_weight: dec!(0.10),
        };

        assert_eq!(weights.process_weight, dec!(0.40));
        assert_eq!(weights.risk_weight, dec!(0.30));
        assert_eq!(weights.execution_weight, dec!(0.20));
        assert_eq!(weights.documentation_weight, dec!(0.10));
    }

    #[test]
    fn test_trade_grade_creation() {
        let now = Utc::now().naive_utc();
        let trade_id = Uuid::new_v4();

        let grade = TradeGrade {
            id: Uuid::new_v4(),
            trade_id,
            overall_score: 87,
            overall_grade: Grade::B,
            process_score: 90,
            risk_score: 95,
            execution_score: 80,
            documentation_score: 75,
            recommendations: vec![
                "Improve order timing to reduce slippage".to_string(),
                "Add more detailed trade context".to_string(),
            ],
            graded_at: now,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        assert_eq!(grade.trade_id, trade_id);
        assert_eq!(grade.overall_score, 87);
        assert_eq!(grade.overall_grade, Grade::B);
        assert_eq!(grade.process_score, 90);
        assert_eq!(grade.risk_score, 95);
        assert_eq!(grade.execution_score, 80);
        assert_eq!(grade.documentation_score, 75);
        assert_eq!(grade.recommendations.len(), 2);
    }

    #[test]
    fn test_grade_score_ranges() {
        // Test boundary values for scores
        let grade_with_max_scores = TradeGrade {
            id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(),
            overall_score: 100,
            overall_grade: Grade::A,
            process_score: 100,
            risk_score: 100,
            execution_score: 100,
            documentation_score: 100,
            recommendations: vec![],
            graded_at: Utc::now().naive_utc(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
        };

        assert_eq!(grade_with_max_scores.overall_score, 100);
        assert_eq!(grade_with_max_scores.process_score, 100);
        assert_eq!(grade_with_max_scores.risk_score, 100);
        assert_eq!(grade_with_max_scores.execution_score, 100);
        assert_eq!(grade_with_max_scores.documentation_score, 100);

        let grade_with_min_scores = TradeGrade {
            id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(),
            overall_score: 0,
            overall_grade: Grade::F,
            process_score: 0,
            risk_score: 0,
            execution_score: 0,
            documentation_score: 0,
            recommendations: vec![],
            graded_at: Utc::now().naive_utc(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
        };

        assert_eq!(grade_with_min_scores.overall_score, 0);
        assert_eq!(grade_with_min_scores.overall_grade, Grade::F);
    }
}
