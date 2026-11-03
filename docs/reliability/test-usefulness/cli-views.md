# Test usefulness: cli-views

Baseline `3e8718f31bf0` · 44 entries · [Method and score definitions](../test-usefulness.md) · [Complete sortable CSV](catalog.csv).

**Estimates are review judgments, not measured defect-detection rates.** Low-confidence indexed rows need deeper review. Ignored/external cases contribute no default-run evidence. See the CSV for full IDs, source links, helper provenance, and status.

Actions are defined in the [catalog guide](../test-usefulness.md#actions). `Indexed` is automated source-assisted triage; `Family review` covers a shared oracle, not a separate proof of every case.

## [cli/src/views.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::tests::uppercase_first_uppercases_first_character_only [L51](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views.rs#L51) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(uppercase_first("aCCOUNT"), "ACCOUNT")</code> | READ |
| views::tests::uppercase_first_handles_empty_and_non_ascii_safely [L57](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views.rs#L57) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(uppercase_first("1abc"), "1abc")</code> | READ |

## [cli/src/views/account_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::account_view::tests::account_view_new_maps_fields [L89](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs#L89) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.broker_account, "-")</code> | READ |
| views::account_view::tests::account_balance_view_new_formats_and_capitalizes [L106](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs#L106) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.total_balance, "1000")</code> | READ |
| views::account_view::tests::account_balance_view_new_uses_percentages_in_zen_mode [L125](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs#L125) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.total_balance, "100.0%")</code> | READ |
| views::account_view::tests::display_functions_run_for_smoke_coverage [L144](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/account_view.rs#L144) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/advanced_metrics_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::advanced_metrics_view::tests::test_filter_closed_trades [L323](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L323) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( closed_trades.get(1).map(&#124;trade&#124; trade.status), Some(Status::ClosedStopLoss) )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_profit_factor [L343](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L343) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(AdvancedMetricsView::rate_profit_factor(dec!(0.8)), "Poor")</code> | READ |
| views::advanced_metrics_view::tests::test_rate_win_rate_thresholds [L361](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L361) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(AdvancedMetricsView::rate_win_rate(dec!(39.9)), "Poor")</code> | READ |
| views::advanced_metrics_view::tests::test_rate_sharpe_ratio_thresholds [L370](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L370) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( AdvancedMetricsView::rate_sharpe_ratio(dec!(-0.1)), "Very Poor" )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_sortino_ratio_thresholds [L392](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L392) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( AdvancedMetricsView::rate_sortino_ratio(dec!(-0.1)), "Very Poor" )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_calmar_ratio_thresholds [L414](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L414) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( AdvancedMetricsView::rate_calmar_ratio(dec!(-0.1)), "Very Poor" )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_kelly_criterion_thresholds [L436](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L436) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( AdvancedMetricsView::rate_kelly_criterion(dec!(0.0)), "Negative (avoid)" )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_value_at_risk_thresholds [L464](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L464) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( AdvancedMetricsView::rate_value_at_risk(dec!(-15.0)), "High Risk" )</code> | READ |
| views::advanced_metrics_view::tests::test_rate_ulcer_index_thresholds [L484](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L484) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(AdvancedMetricsView::rate_ulcer_index(dec!(20.0)), "Poor")</code> | READ |
| views::advanced_metrics_view::tests::display_functions_cover_empty_and_populated_paths [L503](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/advanced_metrics_view.rs#L503) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(closed.len(), 5)</code> | READ |

## [cli/src/views/concentration_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/concentration_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::concentration_view::tests::warning_icon_maps_all_levels [L137](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/concentration_view.rs#L137) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( ConcentrationView::warning_icon(&WarningLevel::Moderate), "⚠️" )</code> | READ |

## [cli/src/views/drawdown_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/drawdown_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::drawdown_view::tests::currency_and_percentage_formatters_are_consistent [L206](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/drawdown_view.rs#L206) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( DrawdownView::format_percentage_negative(dec!(12.34)), "-12.3%" )</code> | READ |
| views::drawdown_view::tests::equity_formatter_uses_peak_percentages_in_zen_mode [L220](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/drawdown_view.rs#L220) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( DrawdownView::format_equity(dec!(9500), dec!(10000)), "95.0%" )</code> | READ |

## [cli/src/views/execution_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/execution_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::execution_view::tests::display_handles_empty_and_populated_execution_lists [L46](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/execution_view.rs#L46) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |
| views::execution_view::tests::display_handles_executions_without_trade_or_order_links [L73](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/execution_view.rs#L73) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/level_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/level_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::level_view::tests::change_marker_covers_up_down_and_same [L76](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/level_view.rs#L76) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(LevelView::change_marker(2, 2), "=")</code> | READ |
| views::level_view::tests::status_and_history_render_without_panics [L83](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/level_view.rs#L83) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/log_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/log_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::log_view::tests::helper_methods_return_expected_strings [L41](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/log_view.rs#L41) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(LogView::body(&log), "broker event")</code> | READ |
| views::log_view::tests::display_runs_for_smoke_coverage [L48](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/log_view.rs#L48) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/order_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/order_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::order_view::tests::new_maps_optional_and_scalar_fields [L72](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/order_view.rs#L72) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.submitted_at, "")</code> | READ |
| views::order_view::tests::new_hides_prices_in_zen_mode [L98](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/order_view.rs#L98) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.average_filled_price, "hidden")</code> | READ |
| views::order_view::tests::display_orders_runs_for_smoke_coverage [L113](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/order_view.rs#L113) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/risk_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/risk_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::risk_view::tests::format_currency_covers_positive_and_negative_values [L144](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/risk_view.rs#L144) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(RiskView::format_currency(dec!(-1500.5)), "-$1500.50")</code> | READ |
| views::risk_view::tests::format_currency_share_uses_percentage_in_zen_mode [L151](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/risk_view.rs#L151) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( RiskView::format_currency_share(dec!(1500), dec!(10000)), "15.0%" )</code> | READ |
| views::risk_view::tests::calculate_percentage_handles_zero_and_regular_totals [L161](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/risk_view.rs#L161) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(RiskView::calculate_percentage(dec!(25), dec!(200)), "12.5")</code> | READ |
| views::risk_view::tests::format_status_maps_known_and_unknown_statuses [L168](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/risk_view.rs#L168) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(RiskView::format_status(&Status::Canceled), "Unknown status")</code> | READ |

## [cli/src/views/rule_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/rule_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::rule_view::tests::new_formats_rule_fields [L72](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/rule_view.rs#L72) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.description, "Max risk")</code> | READ |
| views::rule_view::tests::display_rules_runs_for_smoke_coverage [L86](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/rule_view.rs#L86) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/trade_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trade_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::trade_view::tests::trade_view_new_maps_trade_snapshot_fields [L109](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trade_view.rs#L109) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.status, "submitted")</code> | READ |
| views::trade_view::tests::trade_balance_view_new_maps_balance_values [L132](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trade_view.rs#L132) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.total_performance, "120")</code> | READ |
| views::trade_view::tests::trade_views_use_percentages_in_zen_mode [L152](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trade_view.rs#L152) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(balance_view.capital_in_market, "25.0%")</code> | READ |
| views::trade_view::tests::display_helpers_run_for_smoke_coverage [L182](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trade_view.rs#L182) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/trading_vehicle_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trading_vehicle_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::trading_vehicle_view::tests::new_uppercases_symbol_broker_and_isin [L74](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trading_vehicle_view.rs#L74) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.isin, "US0378331005")</code> | READ |
| views::trading_vehicle_view::tests::new_uses_dash_for_missing_isin [L95](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trading_vehicle_view.rs#L95) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.isin, "-")</code> | READ |
| views::trading_vehicle_view::tests::display_table_runs_for_smoke_coverage [L105](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/trading_vehicle_view.rs#L105) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |

## [cli/src/views/transaction_view.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/transaction_view.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| views::transaction_view::tests::new_maps_transaction_fields [L82](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/transaction_view.rs#L82) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(view.currency, "USD")</code> | READ |
| views::transaction_view::tests::new_uses_percentage_or_hidden_amount_in_zen_mode [L100](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/transaction_view.rs#L100) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(without_basis.amount, "hidden")</code> | READ |
| views::transaction_view::tests::display_transactions_runs_for_smoke_coverage [L118](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/cli/src/views/transaction_view.rs#L118) | 1–20% Minimal | Medium / Family review | Exercises rendering/routing without checking displayed values or resulting state.<br><code>Renderer/route invoked; output, state and return outcome are not asserted beyond any success/no-panic smoke.</code> | OUTPUT |
