//! Trade grading calculator for evaluating trading performance quality
//!
//! This module provides functionality to grade completed trades based on four key components:
//! - Process Adherence (40%): Following predefined entry criteria and rules
//! - Risk Management (30%): Stop loss placement and risk compliance
//! - Execution Quality (20%): Entry/exit timing and slippage management  
//! - Documentation (10%): Trade thesis and context completeness

use chrono::Utc;
use model::{Grade, GradingWeights, Trade, TradeGrade};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

/// Calculator for evaluating trade quality and assigning grades
#[derive(Debug)]
pub struct TradeGradingCalculator;

impl TradeGradingCalculator {
    /// Calculate a comprehensive grade for a completed trade
    ///
    /// # Arguments
    /// * `trade` - The completed trade to grade
    /// * `weights` - Optional custom weights (uses defaults if None)
    ///
    /// # Returns
    /// * `TradeGrade` - Complete grading evaluation with scores and recommendations
    pub fn calculate_trade_grade(
        trade: &Trade,
        weights: Option<GradingWeights>,
    ) -> Result<TradeGrade, GradingError> {
        let weights = weights.unwrap_or_else(Self::default_weights);

        // Validate weights sum to 1.0
        Self::validate_weights(&weights)?;

        // Calculate component scores
        let process_score = Self::calculate_process_score(trade)?;
        let risk_score = Self::calculate_risk_score(trade)?;
        let execution_score = Self::calculate_execution_score(trade)?;
        let documentation_score = Self::calculate_documentation_score(trade)?;

        // Calculate weighted overall score
        let overall_score = Self::calculate_weighted_score(
            process_score,
            risk_score,
            execution_score,
            documentation_score,
            &weights,
        )?;

        // Convert to letter grade
        let overall_grade = Self::score_to_grade(overall_score);

        // Generate recommendations
        let recommendations = Self::generate_recommendations(
            trade,
            process_score,
            risk_score,
            execution_score,
            documentation_score,
        );

        Ok(TradeGrade {
            id: Uuid::new_v4(),
            trade_id: trade.id,
            overall_score,
            overall_grade,
            process_score,
            risk_score,
            execution_score,
            documentation_score,
            recommendations,
            graded_at: Utc::now().naive_utc(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
        })
    }

    /// Get default grading weights as specified in requirements
    pub fn default_weights() -> GradingWeights {
        GradingWeights {
            process_weight: dec!(0.40),
            risk_weight: dec!(0.30),
            execution_weight: dec!(0.20),
            documentation_weight: dec!(0.10),
        }
    }

    /// Calculate process adherence score (0-100)
    fn calculate_process_score(trade: &Trade) -> Result<u8, GradingError> {
        let mut score = 100u8;

        // Check if trade has basic structure (entry, stop, target)
        if trade.entry.unit_price <= dec!(0)
            || trade.safety_stop.unit_price <= dec!(0)
            || trade.target.unit_price <= dec!(0)
        {
            score = score.saturating_sub(30); // Major deduction for invalid prices
        }

        // Check risk/reward ratio (should be at least 1:1)
        let risk_reward_ratio = Self::calculate_risk_reward_ratio(trade);
        if risk_reward_ratio < dec!(1.0) {
            score = score.saturating_sub(20); // Deduction for poor risk/reward
        }

        // More sophisticated process adherence checks would go here in full implementation
        // For now, basic validation

        Ok(score)
    }

    /// Calculate risk management score (0-100)
    fn calculate_risk_score(trade: &Trade) -> Result<u8, GradingError> {
        let mut score = 100u8;

        // Check if stop loss was set
        if trade.safety_stop.unit_price <= dec!(0) {
            return Ok(0); // No stop loss = failing risk management
        }

        // Check if stop loss makes sense relative to entry
        let stop_risk_percentage = Self::calculate_stop_risk_percentage(trade);

        // Ideal risk is typically 1-3% per trade
        if stop_risk_percentage > dec!(5.0) {
            score = score.saturating_sub(30); // High risk deduction
        } else if stop_risk_percentage > dec!(3.0) {
            score = score.saturating_sub(15); // Moderate risk deduction
        }

        if stop_risk_percentage < dec!(0.5) {
            score = score.saturating_sub(10); // Too tight stops can be problematic
        }

        Ok(score)
    }

    /// Calculate execution quality score (0-100)
    fn calculate_execution_score(_trade: &Trade) -> Result<u8, GradingError> {
        let score = 100u8;

        // For basic implementation, assume good execution
        // In full implementation, would check:
        // - Entry slippage vs intended price
        // - Exit slippage vs target/stop
        // - Timing of entries and exits
        // - Market condition awareness

        Ok(score)
    }

    /// Calculate documentation score (0-100)
    fn calculate_documentation_score(trade: &Trade) -> Result<u8, GradingError> {
        let mut score = 100u8;

        // Check thesis documentation
        if trade.thesis.is_none() || trade.thesis.as_ref().unwrap().trim().is_empty() {
            score = score.saturating_sub(40); // Major deduction for no thesis
        } else if trade.thesis.as_ref().unwrap().len() < 20 {
            score = score.saturating_sub(20); // Deduction for minimal thesis
        }

        // Check sector classification
        if trade.sector.is_none() || trade.sector.as_ref().unwrap().trim().is_empty() {
            score = score.saturating_sub(15);
        }

        // Check asset class documentation
        if trade.asset_class.is_none() || trade.asset_class.as_ref().unwrap().trim().is_empty() {
            score = score.saturating_sub(15);
        }

        // Check trading context
        if trade.context.is_none() || trade.context.as_ref().unwrap().trim().is_empty() {
            score = score.saturating_sub(30); // Context is important for learning
        }

        Ok(score)
    }

    /// Calculate weighted overall score from component scores
    fn calculate_weighted_score(
        process_score: u8,
        risk_score: u8,
        execution_score: u8,
        documentation_score: u8,
        weights: &GradingWeights,
    ) -> Result<u8, GradingError> {
        let weighted_sum = Decimal::from(process_score)
            .checked_mul(weights.process_weight)
            .ok_or(GradingError::ArithmeticError)?
            .checked_add(
                Decimal::from(risk_score)
                    .checked_mul(weights.risk_weight)
                    .ok_or(GradingError::ArithmeticError)?,
            )
            .ok_or(GradingError::ArithmeticError)?
            .checked_add(
                Decimal::from(execution_score)
                    .checked_mul(weights.execution_weight)
                    .ok_or(GradingError::ArithmeticError)?,
            )
            .ok_or(GradingError::ArithmeticError)?
            .checked_add(
                Decimal::from(documentation_score)
                    .checked_mul(weights.documentation_weight)
                    .ok_or(GradingError::ArithmeticError)?,
            )
            .ok_or(GradingError::ArithmeticError)?;

        // Round to nearest integer and clamp to 0-100 range
        let score = weighted_sum.round().to_u64().unwrap_or(0).min(100) as u8;
        Ok(score)
    }

    /// Convert numeric score to letter grade
    fn score_to_grade(score: u8) -> Grade {
        match score {
            90..=100 => Grade::A,
            80..=89 => Grade::B,
            70..=79 => Grade::C,
            60..=69 => Grade::D,
            _ => Grade::F,
        }
    }

    /// Generate actionable recommendations based on component scores
    fn generate_recommendations(
        trade: &Trade,
        process_score: u8,
        risk_score: u8,
        execution_score: u8,
        documentation_score: u8,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        if process_score < 80 {
            recommendations.push("Review and refine trade setup criteria".to_string());
            recommendations.push("Ensure proper risk/reward ratio before entry".to_string());
        }

        if risk_score < 80 {
            recommendations.push("Review position sizing and risk management rules".to_string());
            if Self::calculate_stop_risk_percentage(trade) > dec!(3.0) {
                recommendations
                    .push("Consider tighter stop losses to reduce risk per trade".to_string());
            }
        }

        if execution_score < 80 {
            recommendations.push("Focus on improving entry and exit timing".to_string());
            recommendations.push("Consider using limit orders to reduce slippage".to_string());
        }

        if documentation_score < 80 {
            if trade.thesis.is_none() || trade.thesis.as_ref().unwrap().trim().is_empty() {
                recommendations.push("Always document trade thesis before entry".to_string());
            }
            if trade.context.is_none() || trade.context.as_ref().unwrap().trim().is_empty() {
                recommendations
                    .push("Add technical analysis context for future reference".to_string());
            }
        }

        recommendations
    }

    /// Helper to calculate risk/reward ratio
    fn calculate_risk_reward_ratio(trade: &Trade) -> Decimal {
        let entry = trade.entry.unit_price;
        let stop = trade.safety_stop.unit_price;
        let target = trade.target.unit_price;

        let risk = (entry - stop).abs();
        let reward = (target - entry).abs();

        if risk > dec!(0) {
            reward.checked_div(risk).unwrap_or(dec!(0))
        } else {
            dec!(0)
        }
    }

    /// Helper to calculate stop risk as percentage
    fn calculate_stop_risk_percentage(trade: &Trade) -> Decimal {
        let entry = trade.entry.unit_price;
        let stop = trade.safety_stop.unit_price;

        if entry > dec!(0) {
            ((entry - stop).abs().checked_div(entry).unwrap_or(dec!(0))) * dec!(100)
        } else {
            dec!(0)
        }
    }

    /// Validate that grading weights sum to approximately 1.0
    fn validate_weights(weights: &GradingWeights) -> Result<(), GradingError> {
        let sum = weights.process_weight
            + weights.risk_weight
            + weights.execution_weight
            + weights.documentation_weight;

        // Allow for small floating point errors
        if (sum - dec!(1.0)).abs() > dec!(0.01) {
            return Err(GradingError::InvalidWeights(format!(
                "Weights sum to {} but must sum to 1.0",
                sum
            )));
        }

        Ok(())
    }
}

/// Errors that can occur during trade grading
#[derive(Debug, thiserror::Error)]
pub enum GradingError {
    /// Invalid grading weights (weights don't sum to 1.0)
    #[error("Invalid grading weights: {0}")]
    InvalidWeights(String),
    /// Arithmetic overflow or underflow in grading calculation
    #[error("Arithmetic error in grading calculation")]
    ArithmeticError,
}

impl From<rust_decimal::Error> for GradingError {
    fn from(_: rust_decimal::Error) -> Self {
        GradingError::ArithmeticError
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use model::{
        Currency, Order, OrderAction, OrderCategory, OrderStatus, Status, TimeInForce,
        TradeBalance, TradeCategory, TradingVehicle, TradingVehicleCategory,
    };
    use uuid::Uuid;

    fn create_test_trade(
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        thesis: Option<String>,
        context: Option<String>,
    ) -> Trade {
        let now = Utc::now().naive_utc();
        let trade_id = Uuid::new_v4();

        Trade {
            id: trade_id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trading_vehicle: TradingVehicle {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                symbol: "AAPL".to_string(),
                isin: "US0378331005".to_string(),
                category: TradingVehicleCategory::Stock,
                broker: "TEST".to_string(),
            },
            category: TradeCategory::Long,
            status: Status::ClosedTarget,
            currency: Currency::USD,
            safety_stop: Order {
                id: Uuid::new_v4(),
                broker_order_id: None,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                unit_price: stop_price,
                currency: Currency::USD,
                quantity: 100,
                category: OrderCategory::Stop,
                trading_vehicle_id: Uuid::new_v4(),
                action: OrderAction::Sell,
                status: OrderStatus::New,
                time_in_force: TimeInForce::UntilCanceled,
                trailing_percent: None,
                trailing_price: None,
                filled_quantity: 0,
                average_filled_price: None,
                extended_hours: false,
                submitted_at: None,
                filled_at: None,
                expired_at: None,
                cancelled_at: None,
                closed_at: None,
            },
            entry: Order {
                id: Uuid::new_v4(),
                broker_order_id: None,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                unit_price: entry_price,
                currency: Currency::USD,
                quantity: 100,
                category: OrderCategory::Market,
                trading_vehicle_id: Uuid::new_v4(),
                action: OrderAction::Buy,
                status: OrderStatus::Filled,
                time_in_force: TimeInForce::Day,
                trailing_percent: None,
                trailing_price: None,
                filled_quantity: 100,
                average_filled_price: Some(entry_price),
                extended_hours: false,
                submitted_at: Some(now),
                filled_at: Some(now),
                expired_at: None,
                cancelled_at: None,
                closed_at: None,
            },
            target: Order {
                id: Uuid::new_v4(),
                broker_order_id: None,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                unit_price: target_price,
                currency: Currency::USD,
                quantity: 100,
                category: OrderCategory::Limit,
                trading_vehicle_id: Uuid::new_v4(),
                action: OrderAction::Sell,
                status: OrderStatus::New,
                time_in_force: TimeInForce::UntilCanceled,
                trailing_percent: None,
                trailing_price: None,
                filled_quantity: 0,
                average_filled_price: None,
                extended_hours: false,
                submitted_at: None,
                filled_at: None,
                expired_at: None,
                cancelled_at: None,
                closed_at: None,
            },
            account_id: Uuid::new_v4(),
            balance: TradeBalance {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                currency: Currency::USD,
                funding: dec!(10000),
                capital_in_market: dec!(10000),
                capital_out_market: dec!(0),
                taxed: dec!(0),
                total_performance: dec!(500), // $500 profit
            },
            thesis,
            sector: Some("Technology".to_string()),
            asset_class: Some("Stock".to_string()),
            context,
        }
    }

    #[test]
    fn test_default_weights() {
        let weights = TradeGradingCalculator::default_weights();
        assert_eq!(weights.process_weight, dec!(0.40));
        assert_eq!(weights.risk_weight, dec!(0.30));
        assert_eq!(weights.execution_weight, dec!(0.20));
        assert_eq!(weights.documentation_weight, dec!(0.10));
    }

    #[test]
    fn test_validate_weights_success() {
        let weights = TradeGradingCalculator::default_weights();
        assert!(TradeGradingCalculator::validate_weights(&weights).is_ok());
    }

    #[test]
    fn test_validate_weights_failure() {
        let weights = GradingWeights {
            process_weight: dec!(0.50),
            risk_weight: dec!(0.30),
            execution_weight: dec!(0.20),
            documentation_weight: dec!(0.10),
        };
        assert!(TradeGradingCalculator::validate_weights(&weights).is_err());
    }

    #[test]
    fn test_score_to_grade() {
        assert_eq!(TradeGradingCalculator::score_to_grade(95), Grade::A);
        assert_eq!(TradeGradingCalculator::score_to_grade(90), Grade::A);
        assert_eq!(TradeGradingCalculator::score_to_grade(85), Grade::B);
        assert_eq!(TradeGradingCalculator::score_to_grade(80), Grade::B);
        assert_eq!(TradeGradingCalculator::score_to_grade(75), Grade::C);
        assert_eq!(TradeGradingCalculator::score_to_grade(70), Grade::C);
        assert_eq!(TradeGradingCalculator::score_to_grade(65), Grade::D);
        assert_eq!(TradeGradingCalculator::score_to_grade(60), Grade::D);
        assert_eq!(TradeGradingCalculator::score_to_grade(55), Grade::F);
        assert_eq!(TradeGradingCalculator::score_to_grade(0), Grade::F);
    }

    #[test]
    fn test_calculate_risk_reward_ratio() {
        let trade = create_test_trade(
            dec!(100), // entry
            dec!(95),  // stop (5% risk)
            dec!(110), // target (10% reward)
            Some("Test thesis".to_string()),
            Some("Test context".to_string()),
        );

        let ratio = TradeGradingCalculator::calculate_risk_reward_ratio(&trade);
        assert_eq!(ratio, dec!(2.0)); // 10% reward / 5% risk = 2:1
    }

    #[test]
    fn test_calculate_stop_risk_percentage() {
        let trade = create_test_trade(
            dec!(100), // entry
            dec!(95),  // stop
            dec!(110), // target
            Some("Test thesis".to_string()),
            Some("Test context".to_string()),
        );

        let risk_pct = TradeGradingCalculator::calculate_stop_risk_percentage(&trade);
        assert_eq!(risk_pct, dec!(5.0)); // 5% risk
    }

    #[test]
    fn test_process_score_good_setup() {
        let trade = create_test_trade(
            dec!(100), // entry
            dec!(95),  // stop
            dec!(110), // target (good 2:1 ratio)
            Some("Well documented thesis".to_string()),
            Some("Strong technical setup".to_string()),
        );

        let score = TradeGradingCalculator::calculate_process_score(&trade).unwrap();
        assert!(score >= 80); // Should be good score for proper setup
    }

    #[test]
    fn test_risk_score_no_stop_loss() {
        let mut trade = create_test_trade(
            dec!(100),
            dec!(0), // No stop loss
            dec!(110),
            Some("Test thesis".to_string()),
            Some("Test context".to_string()),
        );
        trade.safety_stop.unit_price = dec!(0);

        let score = TradeGradingCalculator::calculate_risk_score(&trade).unwrap();
        assert_eq!(score, 0); // Should be failing score
    }

    #[test]
    fn test_documentation_score_complete() {
        let trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Some("Comprehensive trade thesis with detailed analysis".to_string()),
            Some("Strong technical indicators and market context".to_string()),
        );

        let score = TradeGradingCalculator::calculate_documentation_score(&trade).unwrap();
        assert!(score >= 90); // Should be excellent score
    }

    #[test]
    fn test_documentation_score_missing_thesis() {
        let trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            None, // No thesis
            Some("Some context".to_string()),
        );

        let score = TradeGradingCalculator::calculate_documentation_score(&trade).unwrap();
        assert!(score <= 60); // Should be heavily penalized
    }

    #[test]
    fn test_calculate_trade_grade_comprehensive() {
        let trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Some("Well researched trade with strong fundamentals".to_string()),
            Some("Technical breakout with volume confirmation".to_string()),
        );

        let result = TradeGradingCalculator::calculate_trade_grade(&trade, None);
        assert!(result.is_ok());

        let grade = result.unwrap();
        assert_eq!(grade.trade_id, trade.id);
        assert!(grade.overall_score > 0);
        assert!(matches!(
            grade.overall_grade,
            Grade::A | Grade::B | Grade::C
        ));
        assert!(!grade.recommendations.is_empty() || grade.overall_score >= 80);
    }

    #[test]
    fn test_weighted_score_calculation() {
        let weights = TradeGradingCalculator::default_weights();

        let result = TradeGradingCalculator::calculate_weighted_score(
            90, // process
            85, // risk
            80, // execution
            75, // documentation
            &weights,
        );

        assert!(result.is_ok());
        let score = result.unwrap();

        // Manual calculation: 90*0.4 + 85*0.3 + 80*0.2 + 75*0.1 = 36 + 25.5 + 16 + 7.5 = 85
        assert_eq!(score, 85);
    }

    #[test]
    fn test_recommendations_generated() {
        let poor_trade = create_test_trade(
            dec!(100),
            dec!(85),  // High risk (15%)
            dec!(105), // Poor reward (5%)
            None,      // No thesis
            None,      // No context
        );

        let grade = TradeGradingCalculator::calculate_trade_grade(&poor_trade, None).unwrap();
        assert!(!grade.recommendations.is_empty());
        assert!(grade.recommendations.iter().any(|r| r.contains("thesis")));
        assert!(grade.recommendations.iter().any(|r| r.contains("risk")));
    }
}
