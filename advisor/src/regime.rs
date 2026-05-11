use crate::AdvisorError;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use model::{Account, BarTimeframe, Broker, MarketBar};
use rust_decimal::Decimal;

const DEFAULT_LOOKBACK_DAYS: i64 = 252;
const DEFAULT_ATR_PERIOD: usize = 14;
const DEFAULT_ADX_PERIOD: usize = 14;
const DEFAULT_BREADTH_SMA_DAYS: usize = 50;

/// Request for market-regime filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegimeRequest {
    /// Broad index or market proxy to evaluate.
    pub symbol: String,
    /// Optional universe used for breadth analysis.
    pub breadth_universe: Vec<String>,
    /// Optional volatility index symbol. Missing broker data is ignored.
    pub vix_symbol: Option<String>,
}

impl RegimeRequest {
    /// Build a request for a market proxy symbol.
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            breadth_universe: Vec::new(),
            vix_symbol: Some(default_vix_symbol()),
        }
    }

    /// Attach a breadth universe for 50-day SMA participation analysis.
    pub fn with_breadth_universe(mut self, breadth_universe: Vec<String>) -> Self {
        self.breadth_universe = breadth_universe;
        self
    }

    /// Override or disable the volatility-index secondary signal.
    pub fn with_vix_symbol(mut self, vix_symbol: Option<String>) -> Self {
        self.vix_symbol = vix_symbol;
        self
    }
}

impl Default for RegimeRequest {
    fn default() -> Self {
        Self::new("SPY")
    }
}

/// Configuration for broker-bar regime analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegimeConfig {
    /// Calendar-day lookback requested from the broker.
    pub lookback_days: i64,
    /// ATR lookback period.
    pub atr_period: usize,
    /// ADX lookback period.
    pub adx_period: usize,
    /// SMA window used for breadth participation.
    pub breadth_sma_days: usize,
    /// VIX level that raises calm volatility to normal.
    pub vix_normal_level: Decimal,
    /// VIX level that forces elevated volatility.
    pub vix_elevated_level: Decimal,
}

impl RegimeConfig {
    /// Build explicit regime analysis configuration.
    pub fn new(
        lookback_days: i64,
        atr_period: usize,
        adx_period: usize,
        breadth_sma_days: usize,
    ) -> Self {
        Self {
            lookback_days,
            atr_period,
            adx_period,
            breadth_sma_days,
            vix_normal_level: default_vix_normal_level(),
            vix_elevated_level: default_vix_elevated_level(),
        }
    }
}

impl Default for RegimeConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_LOOKBACK_DAYS,
            DEFAULT_ATR_PERIOD,
            DEFAULT_ADX_PERIOD,
            DEFAULT_BREADTH_SMA_DAYS,
        )
    }
}

/// Volatility regime derived from ATR percentile and optional VIX level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolRegime {
    /// Current volatility is below the 25th ATR percentile.
    Calm,
    /// Current volatility is between the 25th and 75th ATR percentiles.
    Normal,
    /// Current volatility is above the 75th ATR percentile.
    Elevated,
}

/// Trend regime derived from ADX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendRegime {
    /// ADX is below 20.
    Choppy,
    /// ADX is between 20 and 40.
    Trending,
    /// ADX is above 40.
    StrongTrend,
}

/// Breadth regime from percentage of symbols above their 50-day SMA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreadthRegime {
    /// At least half of the configured universe is above its 50-day SMA.
    Broad,
    /// Less than half of the configured universe is above its 50-day SMA.
    Narrow,
}

/// Composite permission regime. The most cautious active axis wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeRegime {
    /// Calm volatility with a trending market.
    CalmTrending,
    /// Normal volatility with a trending market.
    NormalTrending,
    /// Elevated volatility with a trending market.
    ElevatedTrending,
    /// Choppy market with non-elevated volatility.
    Choppy,
    /// Choppy market with elevated volatility.
    ElevatedChoppy,
    /// Narrow breadth overrides otherwise constructive trend/volatility.
    NarrowBreadth,
}

/// Market-regime snapshot used as a permission filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegimeSnapshot {
    /// Volatility axis.
    pub vol_regime: VolRegime,
    /// Trend-strength axis.
    pub trend_regime: TrendRegime,
    /// Optional breadth axis.
    pub breadth_regime: Option<BreadthRegime>,
    /// Composite regime after applying the most-cautious-axis rule.
    pub composite: CompositeRegime,
    /// Whether breakout setups are permitted.
    pub breakouts_permitted: bool,
    /// Whether mean-reversion setups are permitted.
    pub mean_reversion_permitted: bool,
    /// Time at which the snapshot was computed.
    pub computed_at: NaiveDateTime,
}

/// Backward-compatible advisory name for the regime snapshot.
pub type RegimeAdvisory = RegimeSnapshot;

/// Broker-bar market-regime filter.
#[derive(Debug, Clone, Default)]
pub struct RegimeFilter {
    config: RegimeConfig,
}

impl RegimeFilter {
    /// Build a regime filter with explicit configuration.
    pub fn new(config: RegimeConfig) -> Result<Self, AdvisorError> {
        validate_config(config)?;
        Ok(Self { config })
    }

    /// Return the active regime configuration.
    pub fn config(&self) -> RegimeConfig {
        self.config
    }

    /// Evaluate the current market regime from broker daily bars.
    pub fn evaluate(
        &self,
        request: &RegimeRequest,
        broker: &dyn Broker,
        account: &Account,
    ) -> Result<RegimeSnapshot, AdvisorError> {
        validate_config(self.config)?;
        let end = Utc::now();
        let start = lookback_start(end, self.config.lookback_days)?;
        let symbol = normalized_symbol(&request.symbol)?;
        let bars = fetch_bars(broker, account, &symbol, start, end)?;

        let atr_values = atr_series_from_bars(&bars, self.config.atr_period)?;
        let current_atr = latest_decimal(&atr_values, "ATR")?;
        let mut vol_regime = classify_volatility(current_atr, &atr_values)?;
        if let Some(vix_level) = self.fetch_vix_level(request, broker, account, start, end)? {
            vol_regime = apply_vix_signal(vol_regime, vix_level, self.config);
        }

        let adx_values = adx_series_from_bars(&bars, self.config.adx_period)?;
        let trend_regime = classify_trend(latest_decimal(&adx_values, "ADX")?);
        let breadth_regime = self.evaluate_breadth(request, broker, account, start, end)?;
        let composite = composite_regime(vol_regime, trend_regime, breadth_regime);
        let (breakouts_permitted, mean_reversion_permitted) = permissions(composite);

        Ok(RegimeSnapshot {
            vol_regime,
            trend_regime,
            breadth_regime,
            composite,
            breakouts_permitted,
            mean_reversion_permitted,
            computed_at: end.naive_utc(),
        })
    }

    fn fetch_vix_level(
        &self,
        request: &RegimeRequest,
        broker: &dyn Broker,
        account: &Account,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Option<Decimal>, AdvisorError> {
        let Some(vix_symbol) = &request.vix_symbol else {
            return Ok(None);
        };
        let symbol = normalized_symbol(vix_symbol)?;
        let Ok(mut bars) = broker.get_bars(&symbol, start, end, BarTimeframe::OneDay, account)
        else {
            return Ok(None);
        };
        bars.sort_by_key(|bar| bar.time);
        Ok(bars.last().map(|bar| bar.close))
    }

    fn evaluate_breadth(
        &self,
        request: &RegimeRequest,
        broker: &dyn Broker,
        account: &Account,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Option<BreadthRegime>, AdvisorError> {
        let symbols = normalized_unique_symbols(&request.breadth_universe)?;
        if symbols.is_empty() {
            return Ok(None);
        }

        let mut above_count = 0usize;
        for symbol in &symbols {
            let bars = fetch_bars(broker, account, symbol, start, end)?;
            if latest_close_above_sma(&bars, self.config.breadth_sma_days)? {
                above_count = checked_usize_add(above_count, 1, "breadth above count")?;
            }
        }
        let pct_above = percentage(above_count, symbols.len(), "breadth percentage")?;
        if pct_above >= Decimal::from(50u32) {
            Ok(Some(BreadthRegime::Broad))
        } else {
            Ok(Some(BreadthRegime::Narrow))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectionalPoint {
    true_range: Decimal,
    plus_dm: Decimal,
    minus_dm: Decimal,
}

fn validate_config(config: RegimeConfig) -> Result<(), AdvisorError> {
    if config.lookback_days <= 0 {
        return Err(calculation_error("lookback_days must be greater than zero"));
    }
    if config.atr_period == 0 {
        return Err(calculation_error("atr_period must be greater than zero"));
    }
    if config.adx_period == 0 {
        return Err(calculation_error("adx_period must be greater than zero"));
    }
    if config.breadth_sma_days == 0 {
        return Err(calculation_error(
            "breadth_sma_days must be greater than zero",
        ));
    }
    if config.vix_normal_level < Decimal::ZERO || config.vix_elevated_level < Decimal::ZERO {
        return Err(calculation_error("VIX thresholds cannot be negative"));
    }
    if config.vix_normal_level > config.vix_elevated_level {
        return Err(calculation_error(
            "vix_normal_level cannot exceed vix_elevated_level",
        ));
    }
    Ok(())
}

fn lookback_start(end: DateTime<Utc>, lookback_days: i64) -> Result<DateTime<Utc>, AdvisorError> {
    let duration = Duration::try_days(lookback_days)
        .ok_or_else(|| calculation_error("lookback_days cannot be represented"))?;
    end.checked_sub_signed(duration)
        .ok_or_else(|| calculation_error("lookback window starts before representable time"))
}

fn fetch_bars(
    broker: &dyn Broker,
    account: &Account,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<MarketBar>, AdvisorError> {
    broker
        .get_bars(symbol, start, end, BarTimeframe::OneDay, account)
        .map_err(|error| AdvisorError::BrokerData(error.to_string()))
}

fn atr_series_from_bars(bars: &[MarketBar], period: usize) -> Result<Vec<Decimal>, AdvisorError> {
    let points = directional_points_from_bars(bars)?;
    let ranges: Vec<Decimal> = points.iter().map(|point| point.true_range).collect();
    rolling_average(&ranges, period)
}

fn adx_series_from_bars(bars: &[MarketBar], period: usize) -> Result<Vec<Decimal>, AdvisorError> {
    let points = directional_points_from_bars(bars)?;
    let mut dx_values = Vec::new();
    let Some(last_start) = points.len().checked_sub(period) else {
        return Ok(dx_values);
    };
    let mut start = 0usize;
    loop {
        let window = points.iter().skip(start).take(period);
        let (sum_tr, sum_plus, sum_minus) = directional_sums(window)?;
        if sum_tr > Decimal::ZERO {
            let plus_di = checked_div(
                checked_mul(Decimal::from(100u32), sum_plus, "plus DI scale")?,
                sum_tr,
                "plus DI",
            )?;
            let minus_di = checked_div(
                checked_mul(Decimal::from(100u32), sum_minus, "minus DI scale")?,
                sum_tr,
                "minus DI",
            )?;
            let denominator = checked_add(plus_di, minus_di, "DX denominator")?;
            if denominator > Decimal::ZERO {
                let spread =
                    checked_sub(plus_di.max(minus_di), plus_di.min(minus_di), "DX spread")?;
                let dx = checked_div(
                    checked_mul(Decimal::from(100u32), spread, "DX scale")?,
                    denominator,
                    "DX",
                )?;
                dx_values.push(dx);
            }
        }
        if start == last_start {
            break;
        }
        start = checked_usize_add(start, 1, "ADX rolling start")?;
    }
    rolling_average(&dx_values, period)
}

fn directional_sums<'a>(
    points: impl Iterator<Item = &'a DirectionalPoint>,
) -> Result<(Decimal, Decimal, Decimal), AdvisorError> {
    let mut sum_tr = Decimal::ZERO;
    let mut sum_plus = Decimal::ZERO;
    let mut sum_minus = Decimal::ZERO;
    for point in points {
        sum_tr = checked_add(sum_tr, point.true_range, "TR sum")?;
        sum_plus = checked_add(sum_plus, point.plus_dm, "plus DM sum")?;
        sum_minus = checked_add(sum_minus, point.minus_dm, "minus DM sum")?;
    }
    Ok((sum_tr, sum_plus, sum_minus))
}

fn directional_points_from_bars(bars: &[MarketBar]) -> Result<Vec<DirectionalPoint>, AdvisorError> {
    let mut sorted = bars.to_vec();
    sorted.sort_by_key(|bar| bar.time);
    let mut points = Vec::new();
    let mut iter = sorted.iter();
    let Some(mut previous) = iter.next() else {
        return Ok(points);
    };
    for current in iter {
        if current.high < current.low {
            return Err(calculation_error("market bar high cannot be below low"));
        }
        let high_low = checked_sub(current.high, current.low, "high-low range")?;
        let high_prev_close =
            checked_sub(current.high, previous.close, "high previous close")?.abs();
        let low_prev_close = checked_sub(current.low, previous.close, "low previous close")?.abs();
        let true_range = high_low.max(high_prev_close).max(low_prev_close);
        let up_move = checked_sub(current.high, previous.high, "up move")?;
        let down_move = checked_sub(previous.low, current.low, "down move")?;
        points.push(DirectionalPoint {
            true_range,
            plus_dm: positive_directional_move(up_move, down_move),
            minus_dm: negative_directional_move(up_move, down_move),
        });
        previous = current;
    }
    Ok(points)
}

fn positive_directional_move(up_move: Decimal, down_move: Decimal) -> Decimal {
    if up_move > down_move && up_move > Decimal::ZERO {
        up_move
    } else {
        Decimal::ZERO
    }
}

fn negative_directional_move(up_move: Decimal, down_move: Decimal) -> Decimal {
    if down_move > up_move && down_move > Decimal::ZERO {
        down_move
    } else {
        Decimal::ZERO
    }
}

fn rolling_average(values: &[Decimal], period: usize) -> Result<Vec<Decimal>, AdvisorError> {
    let mut averages = Vec::new();
    let Some(last_start) = values.len().checked_sub(period) else {
        return Ok(averages);
    };
    let mut start = 0usize;
    loop {
        let sum = sum_decimals(values.iter().skip(start).take(period).copied())?;
        averages.push(checked_div(sum, Decimal::from(period), "rolling average")?);
        if start == last_start {
            break;
        }
        start = checked_usize_add(start, 1, "rolling average start")?;
    }
    Ok(averages)
}

fn latest_decimal(values: &[Decimal], label: &'static str) -> Result<Decimal, AdvisorError> {
    values
        .last()
        .copied()
        .ok_or_else(|| calculation_error(format!("insufficient bars to compute {label}")))
}

fn classify_volatility(
    current_atr: Decimal,
    atr_values: &[Decimal],
) -> Result<VolRegime, AdvisorError> {
    let p25 = percentile_value(atr_values, 25)?;
    let p75 = percentile_value(atr_values, 75)?;
    if current_atr < p25 {
        Ok(VolRegime::Calm)
    } else if current_atr > p75 {
        Ok(VolRegime::Elevated)
    } else {
        Ok(VolRegime::Normal)
    }
}

fn apply_vix_signal(vol_regime: VolRegime, vix_level: Decimal, config: RegimeConfig) -> VolRegime {
    if vix_level >= config.vix_elevated_level {
        VolRegime::Elevated
    } else if vix_level >= config.vix_normal_level && vol_regime == VolRegime::Calm {
        VolRegime::Normal
    } else {
        vol_regime
    }
}

fn classify_trend(adx: Decimal) -> TrendRegime {
    if adx < Decimal::from(20u32) {
        TrendRegime::Choppy
    } else if adx <= Decimal::from(40u32) {
        TrendRegime::Trending
    } else {
        TrendRegime::StrongTrend
    }
}

fn composite_regime(
    vol_regime: VolRegime,
    trend_regime: TrendRegime,
    breadth_regime: Option<BreadthRegime>,
) -> CompositeRegime {
    if trend_regime == TrendRegime::Choppy && vol_regime == VolRegime::Elevated {
        CompositeRegime::ElevatedChoppy
    } else if trend_regime == TrendRegime::Choppy {
        CompositeRegime::Choppy
    } else if breadth_regime == Some(BreadthRegime::Narrow) {
        CompositeRegime::NarrowBreadth
    } else {
        trending_composite(vol_regime)
    }
}

fn trending_composite(vol_regime: VolRegime) -> CompositeRegime {
    match vol_regime {
        VolRegime::Calm => CompositeRegime::CalmTrending,
        VolRegime::Normal => CompositeRegime::NormalTrending,
        VolRegime::Elevated => CompositeRegime::ElevatedTrending,
    }
}

fn permissions(composite: CompositeRegime) -> (bool, bool) {
    match composite {
        CompositeRegime::CalmTrending
        | CompositeRegime::NormalTrending
        | CompositeRegime::ElevatedTrending => (true, true),
        CompositeRegime::Choppy
        | CompositeRegime::ElevatedChoppy
        | CompositeRegime::NarrowBreadth => (false, true),
    }
}

fn latest_close_above_sma(bars: &[MarketBar], period: usize) -> Result<bool, AdvisorError> {
    let mut sorted = bars.to_vec();
    sorted.sort_by_key(|bar| bar.time);
    let closes: Vec<Decimal> = sorted
        .iter()
        .filter(|bar| bar.close > Decimal::ZERO)
        .map(|bar| bar.close)
        .collect();
    let start = closes
        .len()
        .checked_sub(period)
        .ok_or_else(|| calculation_error("insufficient bars to compute breadth SMA"))?;
    let sma = checked_div(
        sum_decimals(closes.iter().skip(start).take(period).copied())?,
        Decimal::from(period),
        "breadth SMA",
    )?;
    let latest = latest_decimal(&closes, "latest close")?;
    Ok(latest > sma)
}

fn percentile_value(values: &[Decimal], percentile: usize) -> Result<Decimal, AdvisorError> {
    if values.is_empty() {
        return Err(calculation_error("cannot rank an empty percentile series"));
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let len_minus_one = sorted
        .len()
        .checked_sub(1)
        .ok_or_else(|| calculation_error("percentile length underflow"))?;
    let scaled = len_minus_one
        .checked_mul(percentile)
        .ok_or_else(|| calculation_error("percentile index overflow"))?;
    let index = scaled
        .checked_div(100)
        .ok_or_else(|| calculation_error("percentile index division failed"))?;
    sorted
        .get(index)
        .copied()
        .ok_or_else(|| calculation_error("percentile index out of range"))
}

fn normalized_unique_symbols(symbols: &[String]) -> Result<Vec<String>, AdvisorError> {
    let mut normalized = Vec::new();
    for symbol in symbols {
        let candidate = normalized_symbol(symbol)?;
        if !normalized.iter().any(|existing| existing == &candidate) {
            normalized.push(candidate);
        }
    }
    Ok(normalized)
}

fn normalized_symbol(symbol: &str) -> Result<String, AdvisorError> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return Err(AdvisorError::BlankValue { field: "symbol" });
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn percentage(
    numerator: usize,
    denominator: usize,
    label: &'static str,
) -> Result<Decimal, AdvisorError> {
    if denominator == 0 {
        return Err(calculation_error(format!(
            "{label} denominator cannot be zero"
        )));
    }
    checked_div(
        checked_mul(
            Decimal::from(100u32),
            Decimal::from(numerator),
            "percentage scale",
        )?,
        Decimal::from(denominator),
        label,
    )
}

fn sum_decimals(values: impl Iterator<Item = Decimal>) -> Result<Decimal, AdvisorError> {
    let mut total = Decimal::ZERO;
    for value in values {
        total = checked_add(total, value, "decimal sum")?;
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

fn checked_usize_add(
    left: usize,
    right: usize,
    operation: &'static str,
) -> Result<usize, AdvisorError> {
    left.checked_add(right)
        .ok_or_else(|| calculation_error(format!("{operation} overflow")))
}

fn calculation_error(message: impl Into<String>) -> AdvisorError {
    AdvisorError::Calculation {
        message: message.into(),
    }
}

fn default_vix_symbol() -> String {
    "VIX".to_string()
}

fn default_vix_normal_level() -> Decimal {
    Decimal::from(20u32)
}

fn default_vix_elevated_level() -> Decimal {
    Decimal::from(30u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
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
            Err("not used in regime tests".into())
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
            Err("not used in regime tests".into())
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
            Err("not used in regime tests".into())
        }

        fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
            Err("not used in regime tests".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in regime tests".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not used in regime tests".into())
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

    fn trend_bars(count: usize, start_close: Decimal, step: Decimal) -> Vec<MarketBar> {
        let mut bars = Vec::new();
        let mut close = start_close;
        while bars.len() < count {
            bars.push(market_bar(
                bars.len(),
                close,
                close.checked_add(dec!(1)).unwrap(),
                close.checked_sub(dec!(1)).unwrap(),
            ));
            close = close.checked_add(step).unwrap();
        }
        bars
    }

    fn high_volatility_bars() -> Vec<MarketBar> {
        let mut bars = trend_bars(40, dec!(100), dec!(0));
        let mut close = dec!(100);
        while bars.len() < 60 {
            let high = close.checked_add(dec!(20)).unwrap();
            let low = close.checked_sub(dec!(20)).unwrap();
            bars.push(market_bar(bars.len(), close, high, low));
            close = if close == dec!(100) {
                dec!(101)
            } else {
                dec!(100)
            };
        }
        bars
    }

    fn market_bar(index: usize, close: Decimal, high: Decimal, low: Decimal) -> MarketBar {
        let base = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let offset = i64::try_from(index).unwrap();
        MarketBar {
            time: base
                .checked_add_signed(Duration::try_days(offset).unwrap())
                .unwrap(),
            open: close,
            high,
            low,
            close,
            volume: 1,
        }
    }

    #[test]
    fn regime_filter_uses_broker_bars_and_permits_trending_breakouts() {
        let broker = TestBroker::new(vec![(
            "SPY".to_string(),
            trend_bars(60, dec!(100), dec!(1)),
        )]);
        let request = RegimeRequest::new("spy").with_vix_symbol(None);

        let snapshot = RegimeFilter::default()
            .evaluate(&request, &broker, &Account::default())
            .unwrap();

        assert_eq!(snapshot.vol_regime, VolRegime::Normal);
        assert_eq!(snapshot.trend_regime, TrendRegime::StrongTrend);
        assert_eq!(snapshot.breadth_regime, None);
        assert_eq!(snapshot.composite, CompositeRegime::NormalTrending);
        assert!(snapshot.breakouts_permitted);
        assert!(snapshot.mean_reversion_permitted);
    }

    #[test]
    fn elevated_choppy_regime_blocks_breakouts() {
        let broker = TestBroker::new(vec![("SPY".to_string(), high_volatility_bars())]);
        let request = RegimeRequest::new("SPY").with_vix_symbol(None);

        let snapshot = RegimeFilter::default()
            .evaluate(&request, &broker, &Account::default())
            .unwrap();

        assert_eq!(snapshot.vol_regime, VolRegime::Elevated);
        assert_eq!(snapshot.trend_regime, TrendRegime::Choppy);
        assert_eq!(snapshot.composite, CompositeRegime::ElevatedChoppy);
        assert!(!snapshot.breakouts_permitted);
        assert!(snapshot.mean_reversion_permitted);
    }

    #[test]
    fn narrow_breadth_is_most_cautious_axis_for_constructive_index() {
        let broker = TestBroker::new(vec![
            ("SPY".to_string(), trend_bars(60, dec!(100), dec!(1))),
            ("AAA".to_string(), trend_bars(60, dec!(50), dec!(1))),
            ("BBB".to_string(), trend_bars(60, dec!(100), dec!(-1))),
            ("CCC".to_string(), trend_bars(60, dec!(80), dec!(-1))),
        ]);
        let request = RegimeRequest::new("SPY")
            .with_vix_symbol(None)
            .with_breadth_universe(vec![
                "aaa".to_string(),
                "bbb".to_string(),
                "ccc".to_string(),
            ]);

        let snapshot = RegimeFilter::default()
            .evaluate(&request, &broker, &Account::default())
            .unwrap();

        assert_eq!(snapshot.breadth_regime, Some(BreadthRegime::Narrow));
        assert_eq!(snapshot.composite, CompositeRegime::NarrowBreadth);
        assert!(!snapshot.breakouts_permitted);
    }

    #[test]
    fn missing_default_vix_symbol_degrades_to_atr_signal() {
        let broker = TestBroker::new(vec![(
            "SPY".to_string(),
            trend_bars(60, dec!(100), dec!(1)),
        )]);

        let snapshot = RegimeFilter::default()
            .evaluate(&RegimeRequest::new("SPY"), &broker, &Account::default())
            .unwrap();

        assert_eq!(snapshot.vol_regime, VolRegime::Normal);
        assert_eq!(snapshot.composite, CompositeRegime::NormalTrending);
    }

    #[test]
    fn vix_level_can_force_elevated_volatility() {
        let broker = TestBroker::new(vec![
            ("SPY".to_string(), trend_bars(60, dec!(100), dec!(1))),
            ("VIX".to_string(), trend_bars(60, dec!(35), dec!(0))),
        ]);

        let snapshot = RegimeFilter::default()
            .evaluate(&RegimeRequest::new("SPY"), &broker, &Account::default())
            .unwrap();

        assert_eq!(snapshot.vol_regime, VolRegime::Elevated);
        assert_eq!(snapshot.composite, CompositeRegime::ElevatedTrending);
        assert!(snapshot.breakouts_permitted);
    }

    #[test]
    fn atr14_is_decimal_true_range_average() {
        let bars = trend_bars(15, dec!(100), dec!(1));

        let atr_values = atr_series_from_bars(&bars, 14).unwrap();

        assert_eq!(atr_values, vec![dec!(2)]);
    }

    #[test]
    fn percentile_classification_uses_current_atr_rank() {
        let values = vec![dec!(1), dec!(2), dec!(3), dec!(4), dec!(5)];

        assert_eq!(
            classify_volatility(dec!(1), &values).unwrap(),
            VolRegime::Calm
        );
        assert_eq!(
            classify_volatility(dec!(5), &values).unwrap(),
            VolRegime::Elevated
        );
        assert_eq!(
            classify_volatility(dec!(2), &values).unwrap(),
            VolRegime::Normal
        );
    }

    #[test]
    fn invalid_config_is_rejected() {
        let error = RegimeFilter::new(RegimeConfig::new(0, 14, 14, 50)).unwrap_err();

        assert!(error.to_string().contains("lookback_days"));
    }
}
