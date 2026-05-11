use crate::config::{CalendarCredentials, CalendarProvider};
use crate::AdvisorError;
use chrono::{NaiveDate, Utc};
use model::{TradeEvent, TradeEventSeverity, TradeEventSource, TradeEventType, WriteTradeEventDB};
use reqwest::blocking::Client;
use reqwest::Url;
use serde_json::Value;
use uuid::Uuid;

const FMP_BASE_URL: &str = "https://financialmodelingprep.com";
const POLYGON_BASE_URL: &str = "https://api.polygon.io";

/// Request for calendar-backed catalyst scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalystScanRequest {
    /// Trade that owns the persisted catalyst events.
    pub trade_id: Uuid,
    /// Symbol to scan for calendar events.
    pub symbol: String,
    /// Inclusive start date for the expected hold window.
    pub start_date: NaiveDate,
    /// Inclusive end date for the expected hold window.
    pub end_date: NaiveDate,
}

impl CatalystScanRequest {
    /// Build a catalyst scan request.
    pub fn new(
        trade_id: Uuid,
        symbol: impl Into<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Self {
        Self {
            trade_id,
            symbol: symbol.into(),
            start_date,
            end_date,
        }
    }
}

/// Calendar catalyst scan output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalystScanResult {
    /// Events returned by the calendar API and persisted to storage.
    pub events: Vec<TradeEvent>,
    /// Non-fatal warning explaining why scanning was skipped.
    pub warning: Option<String>,
}

impl CatalystScanResult {
    fn skipped(warning: impl Into<String>) -> Self {
        Self {
            events: Vec::new(),
            warning: Some(warning.into()),
        }
    }
}

/// Calendar API-backed catalyst scanner.
#[derive(Debug, Clone)]
pub struct CatalystScanner {
    credentials: CalendarCredentials,
    client: Client,
    endpoints: CalendarApiEndpoints,
}

impl CatalystScanner {
    /// Build a scanner from explicit calendar credentials.
    pub fn new(credentials: CalendarCredentials) -> Self {
        Self::with_client(credentials, Client::new())
    }

    /// Build a scanner from explicit calendar credentials and an HTTP client.
    pub fn with_client(credentials: CalendarCredentials, client: Client) -> Self {
        Self {
            credentials,
            client,
            endpoints: CalendarApiEndpoints::default(),
        }
    }

    /// Build a scanner from keychain-backed advisor configuration.
    pub fn from_keychain() -> Result<Self, AdvisorError> {
        Ok(Self::new(CalendarCredentials::read()?))
    }

    /// Fetch, classify, and persist catalyst events for a trade.
    pub fn scan(
        &self,
        request: &CatalystScanRequest,
        writer: &mut dyn WriteTradeEventDB,
    ) -> Result<CatalystScanResult, AdvisorError> {
        validate_request(request)?;
        if let Some(warning) = self.skip_warning() {
            return Ok(CatalystScanResult::skipped(warning));
        }

        let symbol = normalized_symbol(&request.symbol)?;
        let candidates = self.fetch_candidates(&symbol)?;
        let mut events = Vec::new();
        for candidate in candidates {
            if is_relevant_candidate(&candidate, request, &symbol) {
                let event = candidate.into_trade_event(request.trade_id, &symbol);
                let created = writer
                    .create_trade_event(&event)
                    .map_err(|error| AdvisorError::Persistence(error.to_string()))?;
                events.push(created);
            }
        }
        Ok(CatalystScanResult {
            events,
            warning: None,
        })
    }

    #[cfg(test)]
    fn with_endpoints(
        credentials: CalendarCredentials,
        client: Client,
        endpoints: CalendarApiEndpoints,
    ) -> Self {
        Self {
            credentials,
            client,
            endpoints,
        }
    }

    fn skip_warning(&self) -> Option<String> {
        match self.credentials.provider() {
            CalendarProvider::None => {
                Some("calendar provider disabled; catalyst scan skipped".into())
            }
            _ if !self.credentials.has_api_key() => {
                Some("calendar API key missing; catalyst scan skipped".into())
            }
            _ => None,
        }
    }

    fn fetch_candidates(&self, symbol: &str) -> Result<Vec<CalendarEventCandidate>, AdvisorError> {
        let api_key = self
            .credentials
            .api_key()
            .ok_or_else(|| AdvisorError::CalendarResponse {
                message: "calendar API key missing after configuration check".to_string(),
            })?;
        let requests = self.calendar_requests(symbol, api_key)?;
        let mut candidates = Vec::new();
        for request in requests {
            let body = self.get_json(request.url)?;
            candidates.extend(decode_calendar_response(&body, request.kind, symbol)?);
        }
        Ok(candidates)
    }

    fn calendar_requests(
        &self,
        symbol: &str,
        api_key: &str,
    ) -> Result<Vec<CalendarApiRequest>, AdvisorError> {
        match self.credentials.provider() {
            CalendarProvider::Fmp => Ok(vec![
                CalendarApiRequest::new(
                    CalendarEndpointKind::FmpEarnings,
                    build_calendar_url(
                        &self.endpoints.fmp_base_url,
                        "api/v3/earning_calendar",
                        &[("symbol", symbol), ("apikey", api_key)],
                    )?,
                ),
                CalendarApiRequest::new(
                    CalendarEndpointKind::FmpDividends,
                    build_calendar_url(
                        &self.endpoints.fmp_base_url,
                        "api/v3/stock_dividend_calendar",
                        &[("symbol", symbol), ("apikey", api_key)],
                    )?,
                ),
            ]),
            CalendarProvider::Polygon => Ok(vec![CalendarApiRequest::new(
                CalendarEndpointKind::PolygonEvents,
                build_calendar_url(
                    &self.endpoints.polygon_base_url,
                    &format!("v3/reference/tickers/{symbol}/events"),
                    &[("apiKey", api_key)],
                )?,
            )]),
            CalendarProvider::None => Ok(Vec::new()),
        }
    }

    fn get_json(&self, url: Url) -> Result<Value, AdvisorError> {
        let response = self.client.get(url).send()?;
        let status = response.status();
        if !status.is_success() {
            return Err(AdvisorError::Http(format!(
                "calendar API returned status {status}"
            )));
        }
        Ok(response.json::<Value>()?)
    }
}

impl Default for CatalystScanner {
    fn default() -> Self {
        Self::new(CalendarCredentials::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarApiEndpoints {
    fmp_base_url: String,
    polygon_base_url: String,
}

impl Default for CalendarApiEndpoints {
    fn default() -> Self {
        Self {
            fmp_base_url: FMP_BASE_URL.to_string(),
            polygon_base_url: POLYGON_BASE_URL.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct CalendarApiRequest {
    kind: CalendarEndpointKind,
    url: Url,
}

impl CalendarApiRequest {
    fn new(kind: CalendarEndpointKind, url: Url) -> Self {
        Self { kind, url }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalendarEndpointKind {
    FmpEarnings,
    FmpDividends,
    PolygonEvents,
}

impl CalendarEndpointKind {
    fn default_event_type(self) -> TradeEventType {
        match self {
            CalendarEndpointKind::FmpEarnings => TradeEventType::Earnings,
            CalendarEndpointKind::FmpDividends => TradeEventType::ExDividend,
            CalendarEndpointKind::PolygonEvents => TradeEventType::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarEventCandidate {
    symbol: Option<String>,
    event_type: TradeEventType,
    event_date: NaiveDate,
    severity: TradeEventSeverity,
    notes: Option<String>,
}

impl CalendarEventCandidate {
    fn into_trade_event(self, trade_id: Uuid, symbol: &str) -> TradeEvent {
        let now = Utc::now().naive_utc();
        TradeEvent {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id,
            symbol: symbol.to_string(),
            event_type: self.event_type,
            event_date: self.event_date,
            severity: self.severity,
            notes: self.notes,
            source: TradeEventSource::CalendarApi,
        }
    }
}

fn validate_request(request: &CatalystScanRequest) -> Result<(), AdvisorError> {
    normalized_symbol(&request.symbol)?;
    if request.end_date < request.start_date {
        return Err(AdvisorError::InvalidDateWindow {
            start_date: request.start_date,
            end_date: request.end_date,
        });
    }
    Ok(())
}

fn normalized_symbol(symbol: &str) -> Result<String, AdvisorError> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return Err(AdvisorError::BlankValue { field: "symbol" });
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn build_calendar_url(
    base_url: &str,
    path: &str,
    params: &[(&str, &str)],
) -> Result<Url, AdvisorError> {
    let mut url = Url::parse(base_url).map_err(|error| AdvisorError::Http(error.to_string()))?;
    url.set_path(path);
    url.query_pairs_mut()
        .clear()
        .extend_pairs(params.iter().copied());
    Ok(url)
}

fn decode_calendar_response(
    value: &Value,
    kind: CalendarEndpointKind,
    request_symbol: &str,
) -> Result<Vec<CalendarEventCandidate>, AdvisorError> {
    let items = calendar_items(value)?;
    let mut candidates = Vec::new();
    for item in items {
        if let Some(candidate) = candidate_from_item(item, kind, request_symbol) {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn calendar_items(value: &Value) -> Result<Vec<&Value>, AdvisorError> {
    if let Some(items) = value.as_array() {
        return Ok(items.iter().collect());
    }
    for field in ["results", "events", "data", "historical"] {
        if let Some(items) = value.get(field).and_then(Value::as_array) {
            return Ok(items.iter().collect());
        }
    }
    Err(AdvisorError::CalendarResponse {
        message: "expected a calendar event array".to_string(),
    })
}

fn candidate_from_item(
    item: &Value,
    kind: CalendarEndpointKind,
    request_symbol: &str,
) -> Option<CalendarEventCandidate> {
    let event_date = parse_event_date(item)?;
    let label = event_label(item);
    let event_type = classify_event_type(kind.default_event_type(), label.as_deref());
    Some(CalendarEventCandidate {
        symbol: extract_symbol(item).or_else(|| Some(request_symbol.to_string())),
        event_type,
        event_date,
        severity: severity_for(event_type),
        notes: event_notes(kind, label.as_deref()),
    })
}

fn parse_event_date(item: &Value) -> Option<NaiveDate> {
    for field in [
        "date",
        "event_date",
        "eventDate",
        "start_date",
        "startDate",
        "exDate",
        "ex_dividend_date",
    ] {
        if let Some(value) = string_field(item, field).and_then(|date| parse_date_text(&date)) {
            return Some(value);
        }
    }
    None
}

fn parse_date_text(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            trimmed
                .split('T')
                .next()
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        })
}

fn event_label(item: &Value) -> Option<String> {
    let mut parts = Vec::new();
    for field in [
        "event_type",
        "eventType",
        "type",
        "event",
        "title",
        "name",
        "description",
    ] {
        if let Some(value) = string_field(item, field) {
            parts.push(value);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn extract_symbol(item: &Value) -> Option<String> {
    for field in ["symbol", "ticker", "ticker_symbol"] {
        if let Some(value) = string_field(item, field) {
            return Some(value.to_ascii_uppercase());
        }
    }
    None
}

fn string_field(item: &Value, field: &str) -> Option<String> {
    item.get(field).and_then(Value::as_str).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn classify_event_type(default_type: TradeEventType, label: Option<&str>) -> TradeEventType {
    let Some(text) = label else {
        return default_type;
    };
    let lower = text.to_ascii_lowercase();
    if text_matches_earnings(&lower) {
        TradeEventType::Earnings
    } else if text_matches_fed(&lower) {
        TradeEventType::Fed
    } else if text_matches_cpi(&lower) {
        TradeEventType::Cpi
    } else if text_matches_nfp(&lower) {
        TradeEventType::Nfp
    } else if text_matches_ex_dividend(&lower) {
        TradeEventType::ExDividend
    } else if text_matches_guidance(&lower) {
        TradeEventType::Guidance
    } else {
        default_type
    }
}

fn text_matches_earnings(text: &str) -> bool {
    text.contains("earnings") || text.contains("earning report")
}

fn text_matches_fed(text: &str) -> bool {
    text.contains("fomc")
        || text.contains("federal reserve")
        || text.contains("fed decision")
        || text.contains("fed minutes")
}

fn text_matches_cpi(text: &str) -> bool {
    text.contains("consumer price index") || text_has_token(text, "cpi")
}

fn text_matches_nfp(text: &str) -> bool {
    text.contains("nonfarm")
        || text.contains("non-farm")
        || text.contains("payroll")
        || text_has_token(text, "nfp")
}

fn text_matches_ex_dividend(text: &str) -> bool {
    text.contains("ex-dividend") || text.contains("ex dividend") || text.contains("dividend")
}

fn text_matches_guidance(text: &str) -> bool {
    text.contains("guidance") || text.contains("forecast") || text.contains("outlook")
}

fn text_has_token(text: &str, token: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| part == token)
}

fn severity_for(event_type: TradeEventType) -> TradeEventSeverity {
    match event_type {
        TradeEventType::Earnings
        | TradeEventType::Fed
        | TradeEventType::Cpi
        | TradeEventType::Nfp => TradeEventSeverity::High,
        TradeEventType::ExDividend => TradeEventSeverity::Low,
        TradeEventType::Guidance | TradeEventType::Other => TradeEventSeverity::Medium,
    }
}

fn event_notes(kind: CalendarEndpointKind, label: Option<&str>) -> Option<String> {
    let provider = match kind {
        CalendarEndpointKind::FmpEarnings | CalendarEndpointKind::FmpDividends => "FMP",
        CalendarEndpointKind::PolygonEvents => "Polygon",
    };
    let prefix = format!("{provider} calendar API");
    label.map_or(Some(prefix.clone()), |value| {
        Some(format!("{prefix}: {value}"))
    })
}

fn is_relevant_candidate(
    candidate: &CalendarEventCandidate,
    request: &CatalystScanRequest,
    normalized_symbol: &str,
) -> bool {
    let in_window =
        candidate.event_date >= request.start_date && candidate.event_date <= request.end_date;
    let symbol_matches = candidate
        .symbol
        .as_deref()
        .map(|symbol| symbol.eq_ignore_ascii_case(normalized_symbol))
        .unwrap_or(true);
    in_window && symbol_matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use model::WriteTradeEventDB;
    use std::error::Error;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};

    #[derive(Debug, Default)]
    struct MemoryTradeEventWriter {
        events: Vec<TradeEvent>,
    }

    impl WriteTradeEventDB for MemoryTradeEventWriter {
        fn create_trade_event(&mut self, event: &TradeEvent) -> Result<TradeEvent, Box<dyn Error>> {
            self.events.push(event.clone());
            Ok(event.clone())
        }

        fn delete_trade_event(&mut self, _event_id: Uuid) -> Result<(), Box<dyn Error>> {
            Ok(())
        }
    }

    struct MockCalendarServer {
        base_url: String,
        handle: JoinHandle<()>,
    }

    impl MockCalendarServer {
        fn start(routes: Vec<(&'static str, &'static str)>) -> Self {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let handle = thread::spawn(move || serve_routes(listener, routes));
            Self { base_url, handle }
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }

        fn join(self) {
            self.handle.join().unwrap();
        }
    }

    fn serve_routes(listener: TcpListener, routes: Vec<(&'static str, &'static str)>) {
        for _route in &routes {
            let (stream, _) = listener.accept().unwrap();
            serve_request(stream, &routes);
        }
    }

    fn serve_request(mut stream: TcpStream, routes: &[(&'static str, &'static str)]) {
        let mut request_line = String::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        reader.read_line(&mut request_line).unwrap();
        let path = request_line.split_whitespace().nth(1).unwrap();
        let body = routes
            .iter()
            .find(|(route_path, _body)| path.starts_with(route_path))
            .map(|(_route_path, body)| *body)
            .unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn request() -> CatalystScanRequest {
        CatalystScanRequest::new(Uuid::new_v4(), "aapl", date(2026, 5, 1), date(2026, 5, 31))
    }

    #[test]
    fn missing_calendar_api_key_skips_with_warning() {
        let scanner = CatalystScanner::new(CalendarCredentials::new(CalendarProvider::Fmp, None));
        let mut writer = MemoryTradeEventWriter::default();

        let result = scanner.scan(&request(), &mut writer).unwrap();

        assert!(result.events.is_empty());
        assert!(writer.events.is_empty());
        assert_eq!(
            result.warning,
            Some("calendar API key missing; catalyst scan skipped".to_string())
        );
    }

    #[test]
    fn classification_matches_risk_table() {
        let cases = [
            (
                "earnings",
                TradeEventType::Earnings,
                TradeEventSeverity::High,
            ),
            (
                "FOMC fed decision",
                TradeEventType::Fed,
                TradeEventSeverity::High,
            ),
            ("CPI release", TradeEventType::Cpi, TradeEventSeverity::High),
            (
                "NFP payrolls",
                TradeEventType::Nfp,
                TradeEventSeverity::High,
            ),
            (
                "ex-dividend date",
                TradeEventType::ExDividend,
                TradeEventSeverity::Low,
            ),
            (
                "guidance revision",
                TradeEventType::Guidance,
                TradeEventSeverity::Medium,
            ),
        ];

        for (label, event_type, severity) in cases {
            let classified = classify_event_type(TradeEventType::Other, Some(label));
            assert_eq!(classified, event_type);
            assert_eq!(severity_for(classified), severity);
        }
    }

    #[test]
    fn fmp_scan_persists_windowed_events_from_mock_http() {
        let server = MockCalendarServer::start(vec![
            (
                "/api/v3/earning_calendar",
                r#"[{"date":"2026-05-14","symbol":"AAPL"},{"date":"2026-06-01","symbol":"AAPL"}]"#,
            ),
            (
                "/api/v3/stock_dividend_calendar",
                r#"[{"date":"2026-05-20","symbol":"AAPL","label":"ex-dividend"}]"#,
            ),
        ]);
        let credentials =
            CalendarCredentials::new(CalendarProvider::Fmp, Some("test-key".to_string()));
        let scanner = CatalystScanner::with_endpoints(
            credentials,
            Client::new(),
            CalendarApiEndpoints {
                fmp_base_url: server.base_url().to_string(),
                polygon_base_url: POLYGON_BASE_URL.to_string(),
            },
        );
        let mut writer = MemoryTradeEventWriter::default();

        let result = scanner.scan(&request(), &mut writer).unwrap();
        server.join();

        assert_eq!(result.warning, None);
        assert_eq!(result.events.len(), 2);
        assert_eq!(writer.events, result.events);
        assert_eq!(
            result.events.first().unwrap().event_type,
            TradeEventType::Earnings
        );
        assert_eq!(
            result.events.first().unwrap().severity,
            TradeEventSeverity::High
        );
        assert_eq!(
            result.events.get(1).unwrap().event_type,
            TradeEventType::ExDividend
        );
        assert_eq!(
            result.events.get(1).unwrap().severity,
            TradeEventSeverity::Low
        );
        assert!(result
            .events
            .iter()
            .all(|event| event.source == TradeEventSource::CalendarApi));
    }

    #[test]
    fn polygon_scan_reads_results_array() {
        let server = MockCalendarServer::start(vec![(
            "/v3/reference/tickers/AAPL/events",
            r#"{"results":[{"event_date":"2026-05-22","ticker":"AAPL","type":"guidance revision"}]}"#,
        )]);
        let credentials =
            CalendarCredentials::new(CalendarProvider::Polygon, Some("test-key".to_string()));
        let scanner = CatalystScanner::with_endpoints(
            credentials,
            Client::new(),
            CalendarApiEndpoints {
                fmp_base_url: FMP_BASE_URL.to_string(),
                polygon_base_url: server.base_url().to_string(),
            },
        );
        let mut writer = MemoryTradeEventWriter::default();

        let result = scanner.scan(&request(), &mut writer).unwrap();
        server.join();

        assert_eq!(result.events.len(), 1);
        assert_eq!(
            result.events.first().unwrap().event_type,
            TradeEventType::Guidance
        );
        assert_eq!(
            result.events.first().unwrap().severity,
            TradeEventSeverity::Medium
        );
    }
}
