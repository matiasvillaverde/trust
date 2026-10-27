//! Broker-neutral service composition. Financial calculations remain in core.
use crate::{
    calculators_advanced_metrics::AdvancedMetricsCalculator as Advanced,
    calculators_drawdown::RealizedDrawdownCalculator as Drawdown,
    calculators_performance::PerformanceCalculator as Performance, TrustFacade,
};
use chrono::{DateTime, Utc};
use model::{Account, BarTimeframe, Status, Trade};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error};
use uuid::Uuid;

impl TrustFacade {
    /// Require an explicit policy before service-driven capital commitment. The
    /// legacy local accounting API can still represent accounts without policies.
    pub fn require_trading_policy(&mut self, account: Uuid) -> Result<(), Box<dyn Error>> {
        let rules = self.search_rules(account)?;
        let per_trade = rules
            .iter()
            .any(|rule| matches!(rule.name, model::RuleName::RiskPerTrade(_)));
        let per_month = rules
            .iter()
            .any(|rule| matches!(rule.name, model::RuleName::RiskPerMonth(_)));
        if !(per_trade && per_month) {
            return Err(Box::new(crate::validation_errors::FundValidationError {
                code: crate::validation_errors::FundValidationErrorCode::RiskPolicyMissing,
                message: "Configure both per-trade and monthly risk caps before funding or submitting through the service".into(),
            }));
        }
        Ok(())
    }
    /// Resolve broker-native identity and persist normalized instrument metadata.
    pub fn resolve_instrument(
        &mut self,
        account: &Account,
        symbol: &str,
        category: model::TradingVehicleCategory,
    ) -> Result<model::TradingVehicle, Box<dyn Error>> {
        let mut metadata = self.broker.instrument_metadata(account, symbol, category)?;
        if let Some(existing) = self
            .search_trading_vehicles()?
            .into_iter()
            .find(|v| v.broker == metadata.broker && v.symbol == metadata.symbol)
        {
            metadata.isin = metadata.isin.or(existing.isin);
            metadata.fixed_income = metadata.fixed_income.or(existing.fixed_income);
        }
        self.upsert_trading_vehicle(metadata)
    }

    /// Read authoritative account identity for service authorization.
    pub fn account_by_id(&self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        self.factory.account_read().id(id)
    }

    /// Read all lifecycle states without suppressing persistence failures.
    pub fn account_trades(&mut self, account: Uuid) -> Result<Vec<Trade>, Box<dyn Error>> {
        let mut trades = Vec::new();
        for status in Status::all() {
            trades.extend(self.search_trades(account, status)?);
        }
        trades.sort_by_key(|trade| (trade.created_at, trade.id));
        Ok(trades)
    }

    /// Broker cost/margin estimates do not reserve funds or authorize submission.
    pub fn preview_broker_order(
        &self,
        account: &Account,
        trade: &Trade,
    ) -> Result<model::broker::BrokerOrderPreview, Box<dyn Error>> {
        if trade.account_id != account.id {
            return Err("Trade/account mismatch".into());
        }
        self.broker.preview_order(trade, account)
    }

    /// Return a quote or a typed broker error; no inferred replacement prices.
    pub fn latest_quote(
        &self,
        account: &Account,
        symbol: &str,
    ) -> Result<model::MarketQuote, Box<dyn Error>> {
        self.broker.get_latest_quote(symbol, account)
    }

    /// Return the last broker tick without treating missing data as zero.
    pub fn latest_tick(
        &self,
        account: &Account,
        symbol: &str,
    ) -> Result<model::MarketTradeTick, Box<dyn Error>> {
        self.broker.get_latest_trade(symbol, account)
    }

    /// Compute level policy from persisted closed trades rather than agent assertions.
    pub fn service_level_snapshot(
        &mut self,
        account: Uuid,
    ) -> Result<crate::services::leveling::LevelPerformanceSnapshot, Box<dyn Error>> {
        let all = self.account_trades(account)?;
        let closed = Performance::filter_closed_trades(&all);
        let recent = Performance::filter_trades_by_days(&closed, 30);
        let points = recent
            .iter()
            .map(|trade| (trade.updated_at, trade.balance.total_performance))
            .collect();
        let mut cache = crate::LevelSnapshotCache::new();
        cache.seed_from_points(points);
        let balances = self.search_all_balances(account)?;
        let active: Vec<_> = balances
            .iter()
            .filter(|balance| balance.total_balance != Decimal::ZERO)
            .collect();
        if active.len() != 1 {
            return Err("Level evaluation requires exactly one non-zero currency balance".into());
        }
        let balance = active.first().ok_or("No account balance")?;
        if recent
            .iter()
            .any(|trade| trade.currency != balance.currency)
        {
            return Err("Cannot aggregate level performance across currencies".into());
        }
        Ok(cache.snapshot(balance.total_balance))
    }

    /// Core report composition shared by service clients.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn service_report(
        &mut self,
        account_id: Uuid,
        kind: &str,
        days: u32,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        by: &str,
        benchmark: Option<&str>,
    ) -> Result<Value, Box<dyn Error>> {
        let account = self.account_by_id(account_id)?;
        let all = self.account_trades(account_id)?;
        let currencies: std::collections::HashSet<_> =
            all.iter().map(|trade| trade.currency).collect();
        if currencies.len() > 1 {
            return Err("Account report cannot aggregate different currencies".into());
        }
        let closed = Performance::filter_closed_trades(&all);
        let mut recent = if from.is_some() {
            closed.clone()
        } else {
            Performance::filter_trades_by_days(&closed, days)
        };
        recent.retain(|trade| {
            from.is_none_or(|date| trade.updated_at >= date.naive_utc())
                && to.is_none_or(|date| trade.updated_at <= date.naive_utc())
        });
        let transactions = self.get_account_transactions(account_id)?;
        let transaction_currencies: std::collections::HashSet<_> = transactions
            .iter()
            .map(|transaction| transaction.currency)
            .chain(all.iter().map(|trade| trade.currency))
            .collect();
        if transaction_currencies.len() > 1 {
            return Err("Account report cannot aggregate different currencies".into());
        }
        match kind {
            "performance" => Ok(serde_json::to_value(
                Performance::calculate_performance_stats(&recent),
            )?),
            "risk" => Ok(
                json!({"positions":self.calculate_open_positions(Some(account_id))?,"rules":self.search_all_rules(account_id)?}),
            ),
            "concentration" => Ok(serde_json::to_value(
                crate::calculators_concentration::ConcentrationCalculator::analyze_by_metadata(
                    &all,
                    crate::calculators_concentration::MetadataField::AssetClass,
                )
                .groups,
            )?),
            "summary" => Ok(
                json!({"account":account,"balances":self.search_all_balances(account_id)?,"performance":Performance::calculate_performance_stats(&recent),"risk":self.calculate_open_positions(Some(account_id))?,"concentration":crate::calculators_concentration::ConcentrationCalculator::analyze_by_metadata(&all,crate::calculators_concentration::MetadataField::AssetClass).groups}),
            ),
            "drawdown" => {
                let curve = report_curve(&transactions, from, to)?;
                Ok(serde_json::to_value(Drawdown::calculate_drawdown_metrics(
                    &curve,
                )?)?)
            }
            "timeline" => {
                let curve = report_curve(&transactions, from, to)?;
                Ok(serde_json::to_value(curve)?)
            }
            "metrics" => Ok(json!({
                "trade_count":recent.len(),"gross_profit":Advanced::calculate_gross_profit(&recent),"gross_loss":Advanced::calculate_gross_loss(&recent),"net_profit":Advanced::calculate_net_profit(&recent),
                "profit_factor":Advanced::calculate_profit_factor(&recent),"expectancy":Advanced::calculate_expectancy(&recent),"win_rate":Advanced::calculate_win_rate(&recent),"average_r_multiple":Advanced::calculate_average_r_multiple(&recent),
                "average_trade_pnl":Advanced::calculate_average_trade_pnl(&recent),"median_trade_pnl":Advanced::calculate_median_trade_pnl(&recent),"payoff_ratio":Advanced::calculate_payoff_ratio(&recent),
                "sharpe":Advanced::calculate_sharpe_ratio(&recent,Decimal::ZERO),"sortino":Advanced::calculate_sortino_ratio(&recent,Decimal::ZERO),"risk_free_rate":"0",
                "value_at_risk_95":Advanced::calculate_value_at_risk(&recent,Decimal::new(95,2)),"expected_shortfall_95":Advanced::calculate_expected_shortfall(&recent,Decimal::new(95,2)),
                "kelly":Advanced::calculate_kelly_criterion(&recent),"execution":Advanced::calculate_execution_quality(&recent),"journal":Advanced::calculate_journal_quality(&recent),"streaks":Advanced::calculate_streak_metrics(&recent),"exposure":Advanced::calculate_exposure_metrics(&all)
            })),
            "attribution" => {
                let mut groups: BTreeMap<String, (u64, Decimal)> = BTreeMap::new();
                for trade in &recent {
                    let key = match by {
                        "sector" => trade.sector.clone().unwrap_or_else(|| "unknown".into()),
                        "asset_class" => trade
                            .asset_class
                            .clone()
                            .unwrap_or_else(|| "unknown".into()),
                        _ => trade.trading_vehicle.symbol.clone(),
                    };
                    let group = groups.entry(key).or_insert((0, Decimal::ZERO));
                    group.0 = group.0.saturating_add(1);
                    group.1 = group
                        .1
                        .checked_add(trade.balance.total_performance)
                        .ok_or("Attribution arithmetic overflow")?;
                }
                Ok(
                    json!({"by":by,"groups":groups.into_iter().map(|(key,(count,pnl))|json!({"key":key,"trades":count,"realized_pnl":pnl})).collect::<Vec<_>>()}),
                )
            }
            "benchmark" => {
                let end = to.unwrap_or_else(Utc::now);
                let start = from.unwrap_or(
                    end.checked_sub_signed(chrono::Duration::days(i64::from(days)))
                        .ok_or("Invalid benchmark period")?,
                );
                let symbol = benchmark.unwrap_or("SPY");
                let mut bars =
                    self.market_bars(&account, symbol, start, end, BarTimeframe::OneDay)?;
                bars.sort_by_key(|bar| bar.time);
                let first = bars.first().ok_or("No benchmark bars available")?;
                let last = bars.last().ok_or("No benchmark bars available")?;
                let benchmark_return = last
                    .close
                    .checked_sub(first.close)
                    .and_then(|difference| difference.checked_div(first.close))
                    .ok_or("Cannot compute benchmark return")?;
                Ok(
                    json!({"benchmark":symbol,"benchmark_return_fraction":benchmark_return,"first_bar":first.time,"last_bar":last.time,"realized_pnl":Advanced::calculate_net_profit(&recent),"performance":Performance::calculate_performance_stats(&recent)}),
                )
            }
            "bias" => Ok(serde_json::to_value(
                self.bias_aggregation_for_account(account_id, i32::try_from(days)?)?,
            )?),
            "wash_sales" => Ok(
                json!({"period":"full_ledger","report":self.wash_sale_report(Some(account_id))?}),
            ),
            "tax" => Ok(
                json!({"period":"full_ledger","report":self.tax_report(Some(account_id),crate::services::TaxRateOverrides::default())?}),
            ),
            _ => Err("Unknown report kind".into()),
        }
    }
}

fn report_curve(
    transactions: &[model::Transaction],
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<crate::calculators_drawdown::RealizedEquityCurve, Box<dyn Error>> {
    let mut curve = Drawdown::calculate_equity_curve(transactions)?;
    let baseline = from.and_then(|from| {
        curve
            .points
            .iter()
            .rev()
            .find(|point| point.timestamp < from.naive_utc())
            .cloned()
            .map(|mut point| {
                point.timestamp = from.naive_utc();
                point
            })
    });
    curve.points.retain(|point| {
        from.is_none_or(|date| point.timestamp >= date.naive_utc())
            && to.is_none_or(|date| point.timestamp <= date.naive_utc())
    });
    if let Some(point) = baseline {
        curve.points.insert(0, point);
    }
    Ok(curve)
}
