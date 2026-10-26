use super::*;
use trust_core::services::{
    advisory::{AdvisoryThresholds, TradeProposal},
    leveling::{DefaultLevelTransitionPolicy, LevelTransitionPolicy},
};
impl Domain {
    pub(super) fn analysis(&mut self, op: Operation, args: &Value) -> Result<Value> {
        use Operation::*;
        let account = self.account(args)?;
        let account_id = account.id;
        match op {
            Reports => {
                let (from, to) = period(args)?;
                output(self.facade.service_report(
                    account_id,
                    string(args, "kind")?,
                    days(args)?,
                    Some(from),
                    Some(to),
                    args.get("by").and_then(Value::as_str).unwrap_or("symbol"),
                    args.get("benchmark").and_then(Value::as_str),
                ))
            }
            Metrics => match string(args, "kind")? {
                "bond" => output(
                    self.facade.calculate_bond_analytics(
                        trust_core::calculators_fixed_income::BondAnalyticsInput {
                            face_value: decimal(args, "face_value")?,
                            market_price: decimal(args, "price")?,
                            annual_coupon_rate_pct: decimal(args, "annual_coupon_rate_pct")?,
                            quantity: i64::try_from(uint(args, "quantity", 1))
                                .map_err(|_| ServiceError::invalid("Invalid bond quantity."))?,
                            years_to_maturity: decimal(args, "years_to_maturity")?,
                            accrued_interest: None,
                        },
                    ),
                ),
                kind => {
                    let first = self
                        .facade
                        .service_report(
                            account_id,
                            "metrics",
                            days(args)?,
                            None,
                            None,
                            "symbol",
                            None,
                        )
                        .map_err(domain_error)?;
                    if kind == "compare" {
                        let other = id(args, "other_account_id")?;
                        let second = self
                            .facade
                            .service_report(
                                other,
                                "metrics",
                                days(args)?,
                                None,
                                None,
                                "symbol",
                                None,
                            )
                            .map_err(domain_error)?;
                        Ok(
                            json!({"account_id":account_id,"metrics":first,"other_account_id":other,"other_metrics":second}),
                        )
                    } else {
                        Ok(first)
                    }
                }
            },
            LevelGet => output(self.facade.level_for_account(account_id)),
            LevelHistory => output(
                self.facade
                    .level_history_for_account(account_id, Some(days(args)?)),
            ),
            LevelRules => output(self.facade.level_adjustment_rules_for_account(account_id)),
            LevelRulesSet => {
                let mut rules = self
                    .facade
                    .level_adjustment_rules_for_account(account_id)
                    .map_err(domain_error)?;
                macro_rules! fields {($($field:ident),*)=>{match string(args,"key")? {$(stringify!($field)=>rules.$field=parse(args,"value")?,)*_=>return Err(ServiceError::invalid("Unknown level policy key; inspect level_policy_get."))}};}
                fields!(
                    monthly_loss_downgrade_pct,
                    single_loss_downgrade_pct,
                    upgrade_profitable_trades,
                    upgrade_win_rate_pct,
                    upgrade_consecutive_wins,
                    cooldown_profitable_trades,
                    cooldown_win_rate_pct,
                    cooldown_consecutive_wins,
                    recovery_profitable_trades,
                    recovery_win_rate_pct,
                    recovery_consecutive_wins,
                    min_trades_at_level_for_upgrade,
                    max_changes_in_30_days
                );
                output(self.facade.set_level_adjustment_rules(account_id, &rules))
            }
            LevelEvaluate => {
                if args.get("target_level").is_some() {
                    output(
                        self.facade.change_level(
                            account_id,
                            u8::try_from(uint(args, "target_level", 0))
                                .map_err(|_| ServiceError::invalid("Invalid level."))?,
                            string(args, "reason")?,
                            model::LevelTrigger::ManualOverride,
                        ),
                    )
                } else {
                    let snapshot = self
                        .facade
                        .service_level_snapshot(account_id)
                        .map_err(domain_error)?;
                    output(self.facade.evaluate_level_transition(
                        account_id,
                        snapshot,
                        boolean(args, "apply"),
                    ))
                }
            }
            LevelProgress => {
                let snapshot = self
                    .facade
                    .service_level_snapshot(account_id)
                    .map_err(domain_error)?;
                let rules = self
                    .facade
                    .level_adjustment_rules_for_account(account_id)
                    .map_err(domain_error)?;
                let level = self
                    .facade
                    .level_for_account(account_id)
                    .map_err(domain_error)?;
                value(DefaultLevelTransitionPolicy::new(rules).progress(&level, &snapshot))
            }
            DistributionRules => output(self.facade.distribution_rules_for_account(account_id)),
            DistributionConfigure => {
                let path = self
                    .config
                    .distribution_password_file
                    .as_ref()
                    .ok_or_else(|| {
                        ServiceError::new(
                            "configuration_required",
                            "An operator-managed distribution password file is required.",
                            "Ask the operator to configure distribution_password_file.",
                        )
                    })?;
                let password =
                    std::fs::read_to_string(path).map_err(|_| ServiceError::internal())?;
                output(self.facade.configure_distribution_with_insurance(
                    account_id,
                    decimal(args, "earnings")?,
                    decimal(args, "tax")?,
                    decimal(args, "reinvestment")?,
                    decimal(args, "insurance")?,
                    decimal(args, "minimum")?,
                    password.trim_end(),
                ))
            }
            DistributionExecute => output(self.facade.execute_distribution(
                account_id,
                decimal(args, "amount")?,
                currency(args)?,
            )),
            DistributionHistory => output(self.facade.distribution_history(account_id)),
            AdvisorStatus => Ok(
                json!({"thresholds":self.facade.advisory_thresholds(account_id).map_err(domain_error)?,"portfolio":self.facade.advisory_status_for_account(account_id).map_err(domain_error)?}),
            ),
            AdvisorCheck => output(self.facade.advisory_check_trade(TradeProposal {
                account_id,
                symbol: string(args, "symbol")?.into(),
                sector: optional_string(args, "sector"),
                asset_class: optional_string(args, "asset_class"),
                entry_price: decimal(args, "entry")?,
                quantity: decimal(args, "quantity")?,
            })),
            AdvisorConfigure => output(self.facade.configure_advisory_thresholds(
                account_id,
                AdvisoryThresholds {
                    sector_limit_pct: decimal(args, "sector_limit_pct")?,
                    asset_class_limit_pct: decimal(args, "asset_class_limit_pct")?,
                    single_position_limit_pct: decimal(args, "single_position_limit_pct")?,
                },
            )),
            AdvisorHistory => value(
                self.facade
                    .advisory_history_for_account(account_id, days(args)?),
            ),
            Correlation => {
                let symbols = self
                    .facade
                    .account_trades(account_id)
                    .map_err(domain_error)?
                    .into_iter()
                    .filter(|t| active(t.status))
                    .map(|t| t.trading_vehicle.symbol)
                    .collect();
                let request = advisor::CorrelationRequest::new(string(args, "symbol")?, symbols);
                output(self.facade.correlation_advisory(
                    &request,
                    &account,
                    advisor::CorrelationConfig::default(),
                ))
            }
            Regime => output(self.facade.regime_advisory(
                &advisor::RegimeRequest::new(string(args, "symbol")?),
                &account,
                advisor::RegimeConfig::default(),
            )),
            Catalysts => {
                let trade = self.trade(args)?;
                let start_date = Utc::now().date_naive();
                let end_date = start_date
                    .checked_add_signed(chrono::Duration::days(30))
                    .ok_or_else(ServiceError::internal)?;
                let credentials = if let Some(path) = &self.config.calendar_credentials_file {
                    let bytes = std::fs::read(path).map_err(|_| ServiceError::internal())?;
                    let credentials: CalendarSecret =
                        serde_json::from_slice(&bytes).map_err(|_| {
                            ServiceError::invalid("Invalid operator calendar credential file.")
                        })?;
                    advisor::CalendarCredentials::new(
                        credentials.provider.parse().map_err(|_| {
                            ServiceError::invalid("Invalid configured calendar provider.")
                        })?,
                        Some(credentials.api_key),
                    )
                } else {
                    advisor::CalendarCredentials::default()
                };
                output(self.facade.scan_trade_catalysts(
                    &advisor::CatalystScanRequest {
                        trade_id: trade.id,
                        symbol: trade.trading_vehicle.symbol,
                        start_date,
                        end_date,
                    },
                    credentials,
                ))
            }
            MarketSnapshot | MarketSession => output(self.facade.market_snapshot_v2(
                &account,
                string(args, "symbol")?,
                Utc::now(),
            )),
            MarketQuote => output(self.facade.latest_quote(&account, string(args, "symbol")?)),
            MarketTrade => output(self.facade.latest_tick(&account, string(args, "symbol")?)),
            MarketBars => {
                let (from, to) = period(args)?;
                let timeframe = match string(args, "timeframe")? {
                    "1min" => model::BarTimeframe::OneMinute,
                    "1h" => model::BarTimeframe::OneHour,
                    _ => model::BarTimeframe::OneDay,
                };
                let max_days = match timeframe {
                    model::BarTimeframe::OneMinute => 1,
                    model::BarTimeframe::OneHour => 30,
                    model::BarTimeframe::OneDay => 366,
                };
                if to.signed_duration_since(from) > chrono::Duration::days(max_days) {
                    return Err(ServiceError::invalid("Bar window exceeds the limit: 1 day for minutes, 30 days for hours, 366 days for daily bars."));
                }
                output(self.facade.market_bars(
                    &account,
                    string(args, "symbol")?,
                    from,
                    to,
                    timeframe,
                ))
            }
            MarketStream => {
                let channels = strings(args, "channels")?
                    .into_iter()
                    .map(|c| match c.as_str() {
                        "quotes" => Ok(model::MarketDataChannel::Quotes),
                        "trades" => Ok(model::MarketDataChannel::Trades),
                        "bars" => Ok(model::MarketDataChannel::Bars),
                        _ => Err(ServiceError::invalid(
                            "Use quotes, trades, or bars channels.",
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?;
                output(
                    self.facade.stream_market_data(
                        &account,
                        &strings(args, "symbols")?,
                        &channels,
                        usize::try_from(uint(args, "max_events", 100))
                            .map_err(|_| ServiceError::invalid("Invalid event limit."))?,
                        uint(args, "timeout_seconds", 5),
                    ),
                )
            }
            _ => Err(ServiceError::internal()),
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarSecret {
    provider: String,
    api_key: String,
}
