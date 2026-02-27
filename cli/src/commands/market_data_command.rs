use clap::{Arg, Command};

pub struct MarketDataCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl MarketDataCommandBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("market-data")
                .about("Fetch and normalize broker market data")
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help("Output format")
                        .value_parser(["text", "json"])
                        .default_value("text")
                        .global(true),
                )
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn snapshot(mut self) -> Self {
        self.subcommands.push(
            Command::new("snapshot")
                .about("Get latest best-effort snapshot for a symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Ticker symbol (e.g. AAPL)")
                        .required(true),
                ),
        );
        self
    }

    pub fn bars(mut self) -> Self {
        self.subcommands.push(
            Command::new("bars")
                .about("Get historical OHLCV bars for a symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Ticker symbol (e.g. AAPL)")
                        .required(true),
                )
                .arg(
                    Arg::new("timeframe")
                        .long("timeframe")
                        .value_name("TIMEFRAME")
                        .help("Bar timeframe: 1m | 1h | 1d")
                        .value_parser(["1m", "1h", "1d"])
                        .required(true),
                )
                .arg(
                    Arg::new("start")
                        .long("start")
                        .value_name("RFC3339")
                        .help("Start timestamp (RFC3339, e.g. 2026-01-01T00:00:00Z)")
                        .required(true),
                )
                .arg(
                    Arg::new("end")
                        .long("end")
                        .value_name("RFC3339")
                        .help("End timestamp (RFC3339, e.g. 2026-01-02T00:00:00Z)")
                        .required(true),
                ),
        );
        self
    }

    pub fn stream(mut self) -> Self {
        self.subcommands.push(
            Command::new("stream")
                .about("Stream realtime bars/quotes/trades for symbols")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbols")
                        .long("symbols")
                        .value_name("SYMBOLS")
                        .help("Comma-separated symbols (e.g. AAPL,MSFT)")
                        .required(true),
                )
                .arg(
                    Arg::new("channels")
                        .long("channels")
                        .value_name("CHANNELS")
                        .help("Comma-separated channels: bars,quotes,trades")
                        .default_value("quotes,trades"),
                )
                .arg(
                    Arg::new("max-events")
                        .long("max-events")
                        .value_name("N")
                        .help("Maximum number of events to emit before exit")
                        .default_value("50"),
                )
                .arg(
                    Arg::new("timeout-seconds")
                        .long("timeout-seconds")
                        .value_name("SECONDS")
                        .help("Maximum stream duration in seconds")
                        .default_value("10"),
                ),
        );
        self
    }

    pub fn quote(mut self) -> Self {
        self.subcommands.push(
            Command::new("quote")
                .about("Get latest quote (bid/ask) for a symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Ticker symbol (e.g. AAPL)")
                        .required(true),
                ),
        );
        self
    }

    pub fn trade(mut self) -> Self {
        self.subcommands.push(
            Command::new("trade")
                .about("Get latest trade tick for a symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Ticker symbol (e.g. AAPL)")
                        .required(true),
                ),
        );
        self
    }

    pub fn session(mut self) -> Self {
        self.subcommands.push(
            Command::new("session")
                .about("Get latest session freshness and source metadata for a symbol")
                .arg(
                    Arg::new("account")
                        .long("account")
                        .value_name("ACCOUNT")
                        .help("Account name or UUID used to resolve broker keys")
                        .required(true),
                )
                .arg(
                    Arg::new("symbol")
                        .long("symbol")
                        .value_name("SYMBOL")
                        .help("Ticker symbol (e.g. AAPL)")
                        .required(true),
                ),
        );
        self
    }
}

impl Default for MarketDataCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::MarketDataCommandBuilder;

    #[test]
    fn market_data_builder_registers_subcommands() {
        let cmd = MarketDataCommandBuilder::new()
            .snapshot()
            .bars()
            .stream()
            .quote()
            .trade()
            .session()
            .build();
        let names: Vec<String> = cmd
            .get_subcommands()
            .map(|sub| sub.get_name().to_string())
            .collect();
        assert!(names.contains(&"snapshot".to_string()));
        assert!(names.contains(&"bars".to_string()));
        assert!(names.contains(&"stream".to_string()));
        assert!(names.contains(&"quote".to_string()));
        assert!(names.contains(&"trade".to_string()));
        assert!(names.contains(&"session".to_string()));
    }

    #[test]
    fn market_data_snapshot_and_bars_parse() {
        let snapshot = MarketDataCommandBuilder::new().snapshot().build();
        let snapshot_matches = snapshot
            .try_get_matches_from([
                "market-data",
                "--format",
                "json",
                "snapshot",
                "--account",
                "acc-1",
                "--symbol",
                "AAPL",
            ])
            .expect("snapshot should parse");
        assert_eq!(
            snapshot_matches
                .get_one::<String>("format")
                .map(String::as_str),
            Some("json")
        );
        let snapshot_sub = snapshot_matches
            .subcommand_matches("snapshot")
            .expect("snapshot subcommand");
        assert_eq!(
            snapshot_sub
                .get_one::<String>("account")
                .map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(
            snapshot_sub.get_one::<String>("symbol").map(String::as_str),
            Some("AAPL")
        );

        let bars = MarketDataCommandBuilder::new().bars().build();
        let bars_matches = bars
            .try_get_matches_from([
                "market-data",
                "bars",
                "--account",
                "acc-1",
                "--symbol",
                "AAPL",
                "--timeframe",
                "1d",
                "--start",
                "2026-01-01T00:00:00Z",
                "--end",
                "2026-01-02T00:00:00Z",
            ])
            .expect("bars should parse");
        let bars_sub = bars_matches
            .subcommand_matches("bars")
            .expect("bars subcommand");
        assert_eq!(
            bars_sub.get_one::<String>("account").map(String::as_str),
            Some("acc-1")
        );
        assert_eq!(
            bars_sub.get_one::<String>("symbol").map(String::as_str),
            Some("AAPL")
        );
        assert_eq!(
            bars_sub.get_one::<String>("timeframe").map(String::as_str),
            Some("1d")
        );

        let stream = MarketDataCommandBuilder::new().stream().build();
        let stream_matches = stream
            .try_get_matches_from([
                "market-data",
                "stream",
                "--account",
                "acc-1",
                "--symbols",
                "AAPL,MSFT",
                "--channels",
                "quotes,trades",
                "--max-events",
                "10",
                "--timeout-seconds",
                "5",
            ])
            .expect("stream should parse");
        let stream_sub = stream_matches
            .subcommand_matches("stream")
            .expect("stream subcommand");
        assert_eq!(
            stream_sub.get_one::<String>("symbols").map(String::as_str),
            Some("AAPL,MSFT")
        );
        assert_eq!(
            stream_sub.get_one::<String>("channels").map(String::as_str),
            Some("quotes,trades")
        );

        let quote = MarketDataCommandBuilder::new().quote().build();
        let quote_matches = quote
            .try_get_matches_from([
                "market-data",
                "quote",
                "--account",
                "acc-1",
                "--symbol",
                "AAPL",
            ])
            .expect("quote should parse");
        let quote_sub = quote_matches
            .subcommand_matches("quote")
            .expect("quote subcommand");
        assert_eq!(
            quote_sub.get_one::<String>("symbol").map(String::as_str),
            Some("AAPL")
        );

        let trade = MarketDataCommandBuilder::new().trade().build();
        let trade_matches = trade
            .try_get_matches_from([
                "market-data",
                "trade",
                "--account",
                "acc-1",
                "--symbol",
                "AAPL",
            ])
            .expect("trade should parse");
        assert!(trade_matches.subcommand_matches("trade").is_some());

        let session = MarketDataCommandBuilder::new().session().build();
        let session_matches = session
            .try_get_matches_from([
                "market-data",
                "session",
                "--account",
                "acc-1",
                "--symbol",
                "AAPL",
            ])
            .expect("session should parse");
        assert!(session_matches.subcommand_matches("session").is_some());
    }

    #[test]
    fn market_data_default_matches_new() {
        let from_default = MarketDataCommandBuilder::default().snapshot().build();
        let from_new = MarketDataCommandBuilder::new().snapshot().build();
        assert_eq!(from_default.get_name(), from_new.get_name());
    }
}
