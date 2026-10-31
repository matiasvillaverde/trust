#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use core::calculators_concentration::ConcentrationGroup;
use core::calculators_concentration::ConcentrationWarning;
use core::calculators_concentration::WarningLevel;
use rust_decimal_macros::dec;

#[test]
fn test_concentration_helpers_sort_and_encode_warnings() {
    let groups = vec![
        ConcentrationGroup {
            name: "Tech".to_string(),
            trade_count: 2,
            total_capital_deployed: dec!(2000),
            realized_pnl: dec!(100),
            current_open_risk: dec!(1000),
        },
        ConcentrationGroup {
            name: "Energy".to_string(),
            trade_count: 1,
            total_capital_deployed: dec!(1000),
            realized_pnl: dec!(50),
            current_open_risk: dec!(500),
        },
    ];
    let groups_payload =
        crate::dispatcher::concentration_report::concentration_groups_json(&groups, dec!(1500));
    let arr = groups_payload.as_slice();
    assert_eq!(arr[0]["name"], "Tech");
    assert_eq!(arr[0]["open_risk_share_percentage"], "66.66666667");
    assert_eq!(arr[1]["name"], "Energy");
    assert_eq!(arr[1]["open_risk_share_percentage"], "33.33333333");

    let warnings = vec![ConcentrationWarning {
        group_name: "Tech".to_string(),
        risk_percentage: dec!(66.67),
        level: WarningLevel::High,
    }];
    let warnings_payload = crate::dispatcher::concentration_report::warnings_json(&warnings);
    assert_eq!(warnings_payload[0]["group_name"], "Tech");
    assert_eq!(warnings_payload[0]["risk_percentage"], "66.67");
    assert_eq!(warnings_payload[0]["level"], "High");
}
