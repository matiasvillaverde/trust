//! Independent worked examples for tail loss, expressed in units of initial risk.
use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
use model::{Status, Trade, TradeCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn outcomes(side: TradeCategory, scale: Decimal) -> Vec<Trade> {
    [dec!(-200), dec!(-100), dec!(50), dec!(150)]
        .into_iter()
        .map(|profit| {
            let mut trade = Trade {
                category: side,
                status: Status::ClosedStopLoss,
                ..Trade::default()
            };
            trade.entry.unit_price = dec!(100);
            trade.entry.quantity = dec!(10) * scale;
            trade.safety_stop.unit_price = if side == TradeCategory::Long {
                dec!(95)
            } else {
                dec!(105)
            };
            trade.balance.total_performance = profit * scale;
            trade
        })
        .collect()
}

#[test]
fn shortfall_averages_the_loss_tail_in_risk_units_for_both_directions() {
    // Each unscaled trade risks $50, so these observations are -4R, -2R, +1R, +3R.
    // The worst half averages -3R; all four average -0.5R; the worst observation is -4R.
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for scale in [dec!(0.1), dec!(1), dec!(10)] {
            let mut trades = outcomes(side, scale);
            for (confidence, expected) in [
                (dec!(0), dec!(-0.5)),
                (dec!(0.5), dec!(-3)),
                (dec!(0.75), dec!(-4)),
                (dec!(0.95), dec!(-4)),
                (dec!(1), dec!(-4)),
            ] {
                assert_eq!(
                    AdvancedMetricsCalculator::calculate_expected_shortfall(&trades, confidence),
                    Some(expected),
                    "{side:?} x{scale}, confidence={confidence}"
                );
                trades.reverse();
                assert_eq!(
                    AdvancedMetricsCalculator::calculate_expected_shortfall(&trades, confidence),
                    Some(expected),
                    "input order must not change the loss tail"
                );
            }
        }
    }
}

#[test]
fn missing_risk_evidence_is_not_reported_as_zero_tail_loss() {
    assert_eq!(
        AdvancedMetricsCalculator::calculate_expected_shortfall(&[], dec!(0.95)),
        None
    );
    let mut trades = outcomes(TradeCategory::Long, dec!(1));
    trades.truncate(1);
    assert_eq!(
        AdvancedMetricsCalculator::calculate_expected_shortfall(&trades, dec!(0.95)),
        None
    );
    let mut invalid = outcomes(TradeCategory::Long, dec!(1));
    for trade in &mut invalid {
        trade.safety_stop.unit_price = trade.entry.unit_price;
    }
    assert_eq!(
        AdvancedMetricsCalculator::calculate_expected_shortfall(&invalid, dec!(0.95)),
        None
    );
    let mut valid = outcomes(TradeCategory::Long, dec!(1));
    valid.extend(invalid);
    assert_eq!(
        AdvancedMetricsCalculator::calculate_expected_shortfall(&valid, dec!(0.5)),
        Some(dec!(-3))
    );
}
