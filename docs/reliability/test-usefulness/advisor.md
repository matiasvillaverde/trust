# Test usefulness: advisor

Baseline `3e8718f31bf0` · 24 entries · [Method and score definitions](../test-usefulness.md) · [Complete sortable CSV](catalog.csv).

**Estimates are review judgments, not measured defect-detection rates.** Low-confidence indexed rows need deeper review. Ignored/external cases contribute no default-run evidence. See the CSV for full IDs, source links, helper provenance, and status.

Actions are defined in the [catalog guide](../test-usefulness.md#actions). `Indexed` is automated source-assisted triage; `Family review` covers a shared oracle, not a separate proof of every case.

## [advisor/src/catalyst.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/catalyst.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| catalyst::tests::missing_calendar_api_key_skips_with_warning [L611](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/catalyst.rs#L611) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( result.warning, Some("calendar API key missing; catalyst scan skipped".to_string()) )</code> | READ |
| catalyst::tests::classification_matches_risk_table [L626](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/catalyst.rs#L626) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(severity_for(classified), severity)</code> | READ |
| catalyst::tests::fmp_scan_persists_windowed_events_from_mock_http [L664](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/catalyst.rs#L664) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(result .events .iter() .all(&#124;event&#124; event.source == TradeEventSource::CalendarApi))</code> | READ |
| catalyst::tests::polygon_scan_reads_results_array [L716](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/catalyst.rs#L716) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( result.events.first().unwrap().severity, TradeEventSeverity::Medium )</code> | READ |

## [advisor/src/config.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| config::tests::calendar_provider_display_and_parse_roundtrip [L324](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs#L324) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(CalendarProvider::from_str("unknown").is_err())</code> | READ |
| config::tests::advisor_config_update_stores_redacted_status [L346](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs#L346) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(config.claude_api_key_display(), "[REDACTED]")</code> | READ |
| config::tests::read_with_store_defaults_missing_values [L372](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs#L372) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(config.claude_api_key_display(), "missing")</code> | READ |
| config::tests::update_rejects_blank_secrets_and_bad_provider_rows [L383](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs#L383) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(read_with_store(&store) .unwrap_err() .to_string() .contains("unsupported calendar provider"))</code> | READ |
| config::update_redaction_tests::config_update_debug_redacts_both_keys [L405](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/config.rs#L405) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(debug.contains("configured: true"))</code> | READ |

## [advisor/src/correlation.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| correlation::tests::correlation_calculator_uses_broker_bars_and_detects_candidate_cluster [L748](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs#L748) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(advisory.cluster, Some("AAPL,MSFT".to_string()))</code> | READ |
| correlation::tests::clustering_uses_single_linkage_transitivity [L783](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs#L783) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(clusters, vec![symbols])</code> | READ |
| correlation::tests::equal_weight_heat_is_used_when_position_heat_is_missing [L797](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs#L797) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(heat, dec!(100))</code> | READ |
| correlation::tests::invalid_threshold_is_rejected [L806](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs#L806) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(error.to_string().contains("invalid correlation threshold"))</code> | READ |
| correlation::tests::decimal_ln_matches_known_identity [L813](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/correlation.rs#L813) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(result, Decimal::ZERO)</code> | READ |

## [advisor/src/error.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/error.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| error::redaction_tests::http_errors_omit_credential_bearing_urls [L83](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/error.rs#L83) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(!format!("{public} {public:?}").contains("secret-audit-marker"))</code> | READ |

## [advisor/src/lib.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/lib.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| tests::advisor_builds_from_explicit_config [L81](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/lib.rs#L81) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(advisor.config(), &config)</code> | READ |

## [advisor/src/regime.rs](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs)

| Test / source | Usefulness | Confidence / basis | Evidence and limitation | Action |
| --- | --- | --- | --- | --- |
| regime::tests::regime_filter_uses_broker_bars_and_permits_trending_breakouts [L814](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L814) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(snapshot.composite, CompositeRegime::NormalTrending)</code> | READ |
| regime::tests::elevated_choppy_regime_blocks_breakouts [L834](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L834) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(snapshot.composite, CompositeRegime::ElevatedChoppy)</code> | READ |
| regime::tests::narrow_breadth_is_most_cautious_axis_for_constructive_index [L850](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L850) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(snapshot.composite, CompositeRegime::NarrowBreadth)</code> | READ |
| regime::tests::missing_default_vix_symbol_degrades_to_atr_signal [L875](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L875) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(snapshot.composite, CompositeRegime::NormalTrending)</code> | READ |
| regime::tests::vix_level_can_force_elevated_volatility [L890](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L890) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(snapshot.composite, CompositeRegime::ElevatedTrending)</code> | READ |
| regime::tests::atr14_is_decimal_true_range_average [L906](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L906) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!(atr_values, vec![dec!(2)])</code> | READ |
| regime::tests::percentile_classification_uses_current_atr_rank [L915](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L915) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert_eq!( classify_volatility(dec!(2), &values).unwrap(), VolRegime::Normal )</code> | READ |
| regime::tests::invalid_config_is_rejected [L933](https://github.com/matiasvillaverde/trust/blob/3e8718f31bf0627e51c26123c06e4f0fc6224abe/advisor/src/regime.rs#L933) | 61–80% Useful | Low / Indexed | Checks an explicit result; exact assertion and source are linked for independent review.<br><code>assert!(error.to_string().contains("lookback_days"))</code> | READ |
