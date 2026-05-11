use crate::AdvisorError;
use chrono::{Duration, NaiveDate, Utc};
use model::{Account, BarTimeframe, Broker, MarketBar};
use rust_decimal::Decimal;

const DEFAULT_LOOKBACK_DAYS: i64 = 60;
const LN_TERMS: u32 = 32;
const SQRT_ITERATIONS: u32 = 50;

/// Request for broker-bar correlation analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorrelationRequest {
    /// Candidate symbol under review.
    pub symbol: String,
    /// Symbols already represented in the account portfolio.
    pub portfolio_symbols: Vec<String>,
    /// Optional per-symbol heat percentages used for adjusted heat output.
    pub position_heat_pct: Vec<PositionHeat>,
}

impl CorrelationRequest {
    /// Build a request from a candidate symbol and open position symbols.
    pub fn new(symbol: impl Into<String>, portfolio_symbols: Vec<String>) -> Self {
        Self {
            symbol: symbol.into(),
            portfolio_symbols,
            position_heat_pct: Vec::new(),
        }
    }

    /// Attach per-symbol heat percentages to the request.
    pub fn with_position_heat(mut self, position_heat_pct: Vec<PositionHeat>) -> Self {
        self.position_heat_pct = position_heat_pct;
        self
    }
}

/// Per-symbol risk heat input for correlation-adjusted heat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionHeat {
    /// Symbol associated with the heat percentage.
    pub symbol: String,
    /// Individual position heat percentage.
    pub heat_pct: Decimal,
}

impl PositionHeat {
    /// Build a per-symbol heat percentage.
    pub fn new(symbol: impl Into<String>, heat_pct: Decimal) -> Self {
        Self {
            symbol: symbol.into(),
            heat_pct,
        }
    }
}

/// Pairwise correlation result for two symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationPair {
    /// First symbol in the pair.
    pub left_symbol: String,
    /// Second symbol in the pair.
    pub right_symbol: String,
    /// Pearson correlation of daily log returns.
    pub correlation: Decimal,
    /// Number of aligned daily returns used for the calculation.
    pub observations: usize,
}

/// Correlation advisory output for a candidate trade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelationAdvisory {
    /// Highest correlation between the candidate and an open position.
    pub max_corr: Decimal,
    /// Symbol of the highest-correlated open position.
    pub corr_with: String,
    /// Candidate correlation cluster, if one is detected.
    pub cluster: Option<String>,
    /// Cluster-adjusted heat percentage for the candidate's effective position.
    pub heat_adjusted_pct: Decimal,
    /// All pairwise correlations for the candidate and open positions.
    pub pairs: Vec<CorrelationPair>,
}

/// Configuration for correlation analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrelationConfig {
    /// Correlation threshold used for clustering.
    pub threshold: Decimal,
    /// Number of calendar days of daily bars requested from the broker.
    pub lookback_days: i64,
}

impl CorrelationConfig {
    /// Build explicit correlation analysis configuration.
    pub fn new(threshold: Decimal, lookback_days: i64) -> Self {
        Self {
            threshold,
            lookback_days,
        }
    }
}

impl Default for CorrelationConfig {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            lookback_days: DEFAULT_LOOKBACK_DAYS,
        }
    }
}

/// Broker-bar correlation calculator.
#[derive(Debug, Clone, Default)]
pub struct CorrelationCalculator {
    config: CorrelationConfig,
}

impl CorrelationCalculator {
    /// Build a calculator with explicit configuration.
    pub fn new(config: CorrelationConfig) -> Result<Self, AdvisorError> {
        validate_config(config)?;
        Ok(Self { config })
    }

    /// Return the active correlation configuration.
    pub fn config(&self) -> CorrelationConfig {
        self.config
    }

    /// Compute correlation advisory data from broker daily bars.
    pub fn analyze(
        &self,
        request: &CorrelationRequest,
        broker: &dyn Broker,
        account: &Account,
    ) -> Result<CorrelationAdvisory, AdvisorError> {
        let symbols = normalized_symbols(request)?;
        if symbols.len() < 2 {
            return advisory_without_pairs(request, &symbols);
        }

        let series = self.fetch_return_series(&symbols, broker, account)?;
        let pairs = pairwise_correlations(&series)?;
        let candidate = normalized_symbol(&request.symbol)?;
        let (max_corr, corr_with) = max_candidate_correlation(&candidate, &pairs);
        let clusters = clusters_above_threshold(&symbols, &pairs, self.config.threshold);
        let candidate_cluster = candidate_cluster(&candidate, &clusters);
        let cluster_symbols = candidate_cluster.unwrap_or_else(|| vec![candidate.clone()]);
        let heat_adjusted_pct = heat_for_symbols(request, &cluster_symbols, &symbols)?;

        Ok(CorrelationAdvisory {
            max_corr,
            corr_with,
            cluster: cluster_name(&cluster_symbols),
            heat_adjusted_pct,
            pairs,
        })
    }

    fn fetch_return_series(
        &self,
        symbols: &[String],
        broker: &dyn Broker,
        account: &Account,
    ) -> Result<Vec<SymbolReturns>, AdvisorError> {
        let end = Utc::now();
        let duration = Duration::try_days(self.config.lookback_days).ok_or_else(|| {
            calculation_error("lookback_days cannot be represented as a chrono duration")
        })?;
        let start = end
            .checked_sub_signed(duration)
            .ok_or_else(|| calculation_error("lookback window starts before representable time"))?;
        let mut series = Vec::new();
        for symbol in symbols {
            let bars = broker
                .get_bars(symbol, start, end, BarTimeframe::OneDay, account)
                .map_err(|error| AdvisorError::BrokerData(error.to_string()))?;
            series.push(SymbolReturns {
                symbol: symbol.clone(),
                returns: log_returns_from_bars(bars)?,
            });
        }
        Ok(series)
    }
}

/// Backward-compatible alias for the correlation calculator.
pub type CorrelationAnalyzer = CorrelationCalculator;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolReturns {
    symbol: String,
    returns: Vec<DailyReturn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DailyReturn {
    date: NaiveDate,
    value: Decimal,
}

fn validate_config(config: CorrelationConfig) -> Result<(), AdvisorError> {
    if config.threshold <= Decimal::ZERO || config.threshold > Decimal::ONE {
        return Err(AdvisorError::InvalidCorrelationThreshold {
            value: config.threshold,
        });
    }
    if config.lookback_days <= 0 {
        return Err(calculation_error("lookback_days must be greater than zero"));
    }
    Ok(())
}

fn advisory_without_pairs(
    request: &CorrelationRequest,
    symbols: &[String],
) -> Result<CorrelationAdvisory, AdvisorError> {
    let candidate = normalized_symbol(&request.symbol)?;
    let heat_adjusted_pct = heat_for_symbols(request, std::slice::from_ref(&candidate), symbols)?;
    Ok(CorrelationAdvisory {
        max_corr: Decimal::ZERO,
        corr_with: String::new(),
        cluster: None,
        heat_adjusted_pct,
        pairs: Vec::new(),
    })
}

fn normalized_symbols(request: &CorrelationRequest) -> Result<Vec<String>, AdvisorError> {
    let mut symbols = Vec::new();
    push_unique_symbol(&mut symbols, normalized_symbol(&request.symbol)?);
    for symbol in &request.portfolio_symbols {
        push_unique_symbol(&mut symbols, normalized_symbol(symbol)?);
    }
    Ok(symbols)
}

fn push_unique_symbol(symbols: &mut Vec<String>, symbol: String) {
    if !symbols.iter().any(|existing| existing == &symbol) {
        symbols.push(symbol);
    }
}

fn normalized_symbol(symbol: &str) -> Result<String, AdvisorError> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return Err(AdvisorError::BlankValue { field: "symbol" });
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn log_returns_from_bars(mut bars: Vec<MarketBar>) -> Result<Vec<DailyReturn>, AdvisorError> {
    bars.sort_by_key(|bar| bar.time);
    let mut previous_close = None;
    let mut returns = Vec::new();
    for bar in bars {
        if bar.close <= Decimal::ZERO {
            previous_close = None;
            continue;
        }
        if let Some(previous) = previous_close {
            let ratio = checked_div(bar.close, previous, "daily close ratio")?;
            returns.push(DailyReturn {
                date: bar.time.date_naive(),
                value: decimal_ln(ratio)?,
            });
        }
        previous_close = Some(bar.close);
    }
    Ok(returns)
}

fn pairwise_correlations(series: &[SymbolReturns]) -> Result<Vec<CorrelationPair>, AdvisorError> {
    let mut pairs = Vec::new();
    for (left_index, left) in series.iter().enumerate() {
        for right in series.iter().skip(left_index.saturating_add(1)) {
            let (left_values, right_values) = aligned_returns(left, right);
            if let Some(correlation) = pearson_correlation(&left_values, &right_values)? {
                pairs.push(CorrelationPair {
                    left_symbol: left.symbol.clone(),
                    right_symbol: right.symbol.clone(),
                    correlation: clamp_correlation(correlation),
                    observations: left_values.len(),
                });
            }
        }
    }
    Ok(pairs)
}

fn aligned_returns(left: &SymbolReturns, right: &SymbolReturns) -> (Vec<Decimal>, Vec<Decimal>) {
    let mut left_values = Vec::new();
    let mut right_values = Vec::new();
    for left_return in &left.returns {
        if let Some(right_return) = right
            .returns
            .iter()
            .find(|candidate| candidate.date == left_return.date)
        {
            left_values.push(left_return.value);
            right_values.push(right_return.value);
        }
    }
    (left_values, right_values)
}

fn pearson_correlation(
    left: &[Decimal],
    right: &[Decimal],
) -> Result<Option<Decimal>, AdvisorError> {
    if left.len() != right.len() || left.len() < 2 {
        return Ok(None);
    }
    let left_mean = average(left)?;
    let right_mean = average(right)?;
    let covariance = covariance(left, right, left_mean, right_mean)?;
    let left_variance = population_variance(left, left_mean)?;
    let right_variance = population_variance(right, right_mean)?;
    if left_variance <= Decimal::ZERO || right_variance <= Decimal::ZERO {
        return Ok(None);
    }
    let denominator = checked_mul(
        decimal_sqrt(left_variance)?,
        decimal_sqrt(right_variance)?,
        "correlation denominator",
    )?;
    if denominator <= Decimal::ZERO {
        return Ok(None);
    }
    Ok(Some(checked_div(covariance, denominator, "correlation")?))
}

fn average(values: &[Decimal]) -> Result<Decimal, AdvisorError> {
    checked_div(
        sum_decimals(values)?,
        Decimal::from(values.len()),
        "average",
    )
}

fn covariance(
    left: &[Decimal],
    right: &[Decimal],
    left_mean: Decimal,
    right_mean: Decimal,
) -> Result<Decimal, AdvisorError> {
    let mut total = Decimal::ZERO;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        let left_diff = checked_sub(*left_value, left_mean, "left covariance diff")?;
        let right_diff = checked_sub(*right_value, right_mean, "right covariance diff")?;
        total = checked_add(
            total,
            checked_mul(left_diff, right_diff, "covariance term")?,
            "covariance sum",
        )?;
    }
    checked_div(total, Decimal::from(left.len()), "covariance")
}

fn population_variance(values: &[Decimal], mean: Decimal) -> Result<Decimal, AdvisorError> {
    let mut total = Decimal::ZERO;
    for value in values {
        let diff = checked_sub(*value, mean, "variance diff")?;
        total = checked_add(
            total,
            checked_mul(diff, diff, "variance term")?,
            "variance sum",
        )?;
    }
    checked_div(total, Decimal::from(values.len()), "variance")
}

fn decimal_ln(value: Decimal) -> Result<Decimal, AdvisorError> {
    if value <= Decimal::ZERO {
        return Err(calculation_error("natural log requires a positive value"));
    }
    let numerator = checked_sub(value, Decimal::ONE, "ln numerator")?;
    let denominator = checked_add(value, Decimal::ONE, "ln denominator")?;
    let z = checked_div(numerator, denominator, "ln z")?;
    let z_squared = checked_mul(z, z, "ln z squared")?;
    let mut power = z;
    let mut divisor = Decimal::ONE;
    let mut sum = Decimal::ZERO;
    let mut remaining = LN_TERMS;
    while remaining > 0 {
        sum = checked_add(sum, checked_div(power, divisor, "ln term")?, "ln sum")?;
        power = checked_mul(power, z_squared, "ln power")?;
        divisor = checked_add(divisor, Decimal::from(2u32), "ln divisor")?;
        remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| calculation_error("ln iteration underflow"))?;
    }
    checked_mul(sum, Decimal::from(2u32), "ln result")
}

fn decimal_sqrt(value: Decimal) -> Result<Decimal, AdvisorError> {
    if value < Decimal::ZERO {
        return Err(calculation_error(
            "square root requires a non-negative value",
        ));
    }
    if value == Decimal::ZERO {
        return Ok(Decimal::ZERO);
    }
    let mut x = value;
    let mut remaining = SQRT_ITERATIONS;
    while remaining > 0 {
        let previous = x;
        x = checked_div(
            checked_add(x, checked_div(value, x, "sqrt division")?, "sqrt sum")?,
            Decimal::from(2u32),
            "sqrt average",
        )?;
        if checked_sub(x.max(previous), x.min(previous), "sqrt convergence")? < sqrt_tolerance() {
            return Ok(x);
        }
        remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| calculation_error("sqrt iteration underflow"))?;
    }
    Ok(x)
}

fn clusters_above_threshold(
    symbols: &[String],
    pairs: &[CorrelationPair],
    threshold: Decimal,
) -> Vec<Vec<String>> {
    let mut clusters = Vec::new();
    let mut assigned = Vec::new();
    for symbol in symbols {
        if assigned.iter().any(|member| member == symbol) {
            continue;
        }
        let cluster = expand_cluster(symbol, pairs, threshold);
        for member in &cluster {
            push_unique_symbol(&mut assigned, member.clone());
        }
        if cluster.len() > 1 {
            clusters.push(cluster);
        }
    }
    clusters
}

fn expand_cluster(seed: &str, pairs: &[CorrelationPair], threshold: Decimal) -> Vec<String> {
    let mut cluster = vec![seed.to_string()];
    let mut changed = true;
    while changed {
        changed = false;
        for pair in pairs.iter().filter(|pair| pair.correlation >= threshold) {
            if cluster_contains(&cluster, &pair.left_symbol)
                && !cluster_contains(&cluster, &pair.right_symbol)
            {
                cluster.push(pair.right_symbol.clone());
                changed = true;
            }
            if cluster_contains(&cluster, &pair.right_symbol)
                && !cluster_contains(&cluster, &pair.left_symbol)
            {
                cluster.push(pair.left_symbol.clone());
                changed = true;
            }
        }
    }
    cluster.sort();
    cluster
}

fn cluster_contains(cluster: &[String], symbol: &str) -> bool {
    cluster.iter().any(|member| member == symbol)
}

fn candidate_cluster(candidate: &str, clusters: &[Vec<String>]) -> Option<Vec<String>> {
    clusters
        .iter()
        .find(|cluster| cluster_contains(cluster, candidate))
        .cloned()
}

fn cluster_name(cluster: &[String]) -> Option<String> {
    if cluster.len() > 1 {
        Some(cluster.join(","))
    } else {
        None
    }
}

fn max_candidate_correlation(candidate: &str, pairs: &[CorrelationPair]) -> (Decimal, String) {
    let mut best: Option<&CorrelationPair> = None;
    for pair in pairs.iter().filter(|pair| pair_involves(pair, candidate)) {
        if best
            .map(|current| pair.correlation > current.correlation)
            .unwrap_or(true)
        {
            best = Some(pair);
        }
    }
    best.map(|pair| {
        (
            pair.correlation,
            other_pair_symbol(pair, candidate).unwrap_or_default(),
        )
    })
    .unwrap_or((Decimal::ZERO, String::new()))
}

fn pair_involves(pair: &CorrelationPair, symbol: &str) -> bool {
    pair.left_symbol == symbol || pair.right_symbol == symbol
}

fn other_pair_symbol(pair: &CorrelationPair, symbol: &str) -> Option<String> {
    if pair.left_symbol == symbol {
        Some(pair.right_symbol.clone())
    } else if pair.right_symbol == symbol {
        Some(pair.left_symbol.clone())
    } else {
        None
    }
}

fn heat_for_symbols(
    request: &CorrelationRequest,
    cluster_symbols: &[String],
    all_symbols: &[String],
) -> Result<Decimal, AdvisorError> {
    let fallback = equal_weight_heat(all_symbols)?;
    let mut total = Decimal::ZERO;
    for symbol in cluster_symbols {
        let heat = configured_heat(symbol, &request.position_heat_pct)?.unwrap_or(fallback);
        total = checked_add(total, heat, "cluster heat")?;
    }
    Ok(total)
}

fn configured_heat(
    symbol: &str,
    position_heat_pct: &[PositionHeat],
) -> Result<Option<Decimal>, AdvisorError> {
    for heat in position_heat_pct {
        if normalized_symbol(&heat.symbol)? == symbol {
            if heat.heat_pct < Decimal::ZERO {
                return Err(calculation_error("position heat cannot be negative"));
            }
            return Ok(Some(heat.heat_pct));
        }
    }
    Ok(None)
}

fn equal_weight_heat(symbols: &[String]) -> Result<Decimal, AdvisorError> {
    if symbols.is_empty() {
        return Ok(Decimal::ZERO);
    }
    checked_div(
        Decimal::from(100u32),
        Decimal::from(symbols.len()),
        "equal-weight heat",
    )
}

fn sum_decimals(values: &[Decimal]) -> Result<Decimal, AdvisorError> {
    let mut total = Decimal::ZERO;
    for value in values {
        total = checked_add(total, *value, "decimal sum")?;
    }
    Ok(total)
}

fn checked_add(
    left: Decimal,
    right: Decimal,
    operation: &'static str,
) -> Result<Decimal, AdvisorError> {
    left.checked_add(right)
        .ok_or_else(|| calculation_error(format!("{operation} overflow")))
}

fn checked_sub(
    left: Decimal,
    right: Decimal,
    operation: &'static str,
) -> Result<Decimal, AdvisorError> {
    left.checked_sub(right)
        .ok_or_else(|| calculation_error(format!("{operation} overflow")))
}

fn checked_mul(
    left: Decimal,
    right: Decimal,
    operation: &'static str,
) -> Result<Decimal, AdvisorError> {
    left.checked_mul(right)
        .ok_or_else(|| calculation_error(format!("{operation} overflow")))
}

fn checked_div(
    left: Decimal,
    right: Decimal,
    operation: &'static str,
) -> Result<Decimal, AdvisorError> {
    left.checked_div(right)
        .ok_or_else(|| calculation_error(format!("{operation} failed")))
}

fn clamp_correlation(value: Decimal) -> Decimal {
    let negative_one = Decimal::new(-1, 0);
    if value > Decimal::ONE {
        Decimal::ONE
    } else if value < negative_one {
        negative_one
    } else {
        value
    }
}

fn calculation_error(message: impl Into<String>) -> AdvisorError {
    AdvisorError::Calculation {
        message: message.into(),
    }
}

fn default_threshold() -> Decimal {
    Decimal::new(70, 2)
}

fn sqrt_tolerance() -> Decimal {
    Decimal::new(1, 7)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone};
    use model::{BrokerKind, BrokerLog, Order, OrderIds, Status, Trade};
    use rust_decimal_macros::dec;
    use std::error::Error;

    #[derive(Debug, Clone)]
    struct TestBroker {
        bars: Vec<(String, Vec<MarketBar>)>,
    }

    impl TestBroker {
        fn new(bars: Vec<(String, Vec<MarketBar>)>) -> Self {
            Self { bars }
        }
    }

    impl Broker for TestBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in correlation tests".into())
        }

        fn get_bars(
            &self,
            symbol: &str,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
            timeframe: BarTimeframe,
            _account: &Account,
        ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
            if timeframe != BarTimeframe::OneDay {
                return Err("expected daily bars".into());
            }
            self.bars
                .iter()
                .find(|(stored_symbol, _bars)| stored_symbol == symbol)
                .map(|(_stored_symbol, bars)| bars.clone())
                .ok_or_else(|| format!("missing bars for {symbol}").into())
        }
    }

    fn market_bars(closes: &[Decimal]) -> Vec<MarketBar> {
        let mut day = 1u32;
        let mut bars = Vec::new();
        for close in closes {
            bars.push(MarketBar {
                time: Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap(),
                open: *close,
                high: *close,
                low: *close,
                close: *close,
                volume: 1,
            });
            day = day.checked_add(1).unwrap();
        }
        bars
    }

    fn pair(left: &str, right: &str, correlation: Decimal) -> CorrelationPair {
        CorrelationPair {
            left_symbol: left.to_string(),
            right_symbol: right.to_string(),
            correlation,
            observations: 3,
        }
    }

    #[test]
    fn correlation_calculator_uses_broker_bars_and_detects_candidate_cluster() {
        let broker = TestBroker::new(vec![
            (
                "AAPL".to_string(),
                market_bars(&[dec!(100), dec!(110), dec!(115.5), dec!(138.6)]),
            ),
            (
                "MSFT".to_string(),
                market_bars(&[dec!(200), dec!(220), dec!(231), dec!(277.2)]),
            ),
            (
                "TSLA".to_string(),
                market_bars(&[dec!(100), dec!(90), dec!(99), dec!(79.2)]),
            ),
        ]);
        let request = CorrelationRequest::new("aapl", vec!["msft".to_string(), "tsla".to_string()])
            .with_position_heat(vec![
                PositionHeat::new("AAPL", dec!(2)),
                PositionHeat::new("MSFT", dec!(3)),
                PositionHeat::new("TSLA", dec!(4)),
            ]);

        let advisory = CorrelationCalculator::default()
            .analyze(&request, &broker, &Account::default())
            .unwrap();

        let distance_from_one = advisory.max_corr.checked_sub(dec!(1)).unwrap().abs();
        assert!(distance_from_one < dec!(0.0001));
        assert_eq!(advisory.corr_with, "MSFT");
        assert_eq!(advisory.cluster, Some("AAPL,MSFT".to_string()));
        assert_eq!(advisory.heat_adjusted_pct, dec!(5));
        assert_eq!(advisory.pairs.len(), 3);
    }

    #[test]
    fn clustering_uses_single_linkage_transitivity() {
        let symbols = vec!["AAPL".to_string(), "MSFT".to_string(), "NVDA".to_string()];
        let pairs = vec![
            pair("AAPL", "MSFT", dec!(0.8)),
            pair("MSFT", "NVDA", dec!(0.8)),
            pair("AAPL", "NVDA", dec!(0.2)),
        ];

        let clusters = clusters_above_threshold(&symbols, &pairs, dec!(0.7));

        assert_eq!(clusters, vec![symbols]);
    }

    #[test]
    fn equal_weight_heat_is_used_when_position_heat_is_missing() {
        let request = CorrelationRequest::new("AAPL", vec!["MSFT".to_string()]);
        let symbols = normalized_symbols(&request).unwrap();
        let heat = heat_for_symbols(&request, &symbols, &symbols).unwrap();

        assert_eq!(heat, dec!(100));
    }

    #[test]
    fn invalid_threshold_is_rejected() {
        let error = CorrelationCalculator::new(CorrelationConfig::new(dec!(1.1), 60)).unwrap_err();

        assert!(error.to_string().contains("invalid correlation threshold"));
    }

    #[test]
    fn decimal_ln_matches_known_identity() {
        let result = decimal_ln(Decimal::ONE).unwrap();

        assert_eq!(result, Decimal::ZERO);
    }
}
