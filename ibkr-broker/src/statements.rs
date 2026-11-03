//! Original Activity Flex XML adapter. Source text never appears in parse errors.
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use model::statement::*;
use roxmltree::{Document, Node, ParsingOptions};
use rust_decimal::Decimal;
use std::collections::BTreeSet;

mod rows;

/// A stable parse failure; excludes XML values and broker identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatementParseError(pub &'static str);
impl std::fmt::Display for StatementParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for StatementParseError {}
type Result<T> = std::result::Result<T, StatementParseError>;
fn error() -> StatementParseError {
    StatementParseError("invalid_flex_statement")
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .filter(|v| !v.is_empty() && v.len() <= 512 && !v.chars().any(char::is_control))
        .ok_or_else(error)
}
fn decimal(node: Node<'_, '_>, name: &str) -> Result<Decimal> {
    let raw = attr(node, name)?;
    let number = raw.strip_prefix('-').unwrap_or(raw);
    let mut parts = number.split('.');
    let integer = parts.next().ok_or_else(error)?;
    if raw.len() > 64
        || integer.is_empty()
        || !integer.bytes().all(|c| c.is_ascii_digit())
        || parts
            .next()
            .is_some_and(|f| f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()))
        || parts.next().is_some()
    {
        return Err(error());
    }
    Decimal::from_str_exact(raw).map_err(|_| error())
}
fn date(raw: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y%m%d")
        .or_else(|_| NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
        .map_err(|_| error())
}
fn time(raw: &str, timezone: Tz) -> Result<DateTime<Utc>> {
    let value = NaiveDateTime::parse_from_str(raw, "%Y%m%d;%H%M%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d;%H:%M:%S").ok())
        .or_else(|| date(raw).ok()?.and_hms_opt(0, 0, 0))
        .ok_or_else(error)?;
    timezone
        .from_local_datetime(&value)
        .single()
        .map(|t| t.with_timezone(&Utc))
        .ok_or(StatementParseError("ambiguous_statement_timezone"))
}

fn midnight(value: NaiveDate, timezone: Tz) -> Result<DateTime<Utc>> {
    let value = value.and_hms_opt(0, 0, 0).ok_or_else(error)?;
    timezone
        .from_local_datetime(&value)
        .single()
        .map(|t| t.with_timezone(&Utc))
        .ok_or(StatementParseError("ambiguous_statement_timezone"))
}

fn section<'a, 'input>(statement: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    let mut found = statement.children().filter(|n| n.has_tag_name(name));
    let first = found
        .next()
        .ok_or(StatementParseError("missing_statement_section"))?;
    if found.next().is_some() {
        return Err(error());
    }
    Ok(first)
}

fn validate_structure(doc: &Document<'_>, expected_account: &str) -> Result<()> {
    for n in doc.descendants() {
        if n.ancestors().take(12).count() > 10
            || n.is_pi()
            || n.is_text() && n.text().is_some_and(|s| !s.trim().is_empty())
            || n.is_element() && (n.tag_name().namespace().is_some() || n.attributes().len() > 128)
            || n.attribute("accountId")
                .is_some_and(|id| id != expected_account)
        {
            return Err(StatementParseError(
                "statement_structure_or_account_mismatch",
            ));
        }
    }
    Ok(())
}

/// Parse one complete account report. The owner must supply its actual IANA
/// timezone because Flex timestamps do not reliably contain an offset.
pub fn parse_activity_xml(
    xml: &str,
    expected_account: &str,
    timezone: &str,
    now: DateTime<Utc>,
) -> Result<BrokerStatement> {
    if xml.is_empty() || xml.len() > MAX_STATEMENT_BYTES || xml.contains("<!DOCTYPE") {
        return Err(StatementParseError("unsafe_or_oversized_statement"));
    }
    let tz: Tz = timezone
        .parse()
        .map_err(|_| StatementParseError("invalid_statement_timezone"))?;
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 10_000,
            entity_resolver: None,
        },
    )
    .map_err(|_| error())?;
    validate_structure(&doc, expected_account)?;
    let root = doc.root_element();
    if !root.has_tag_name("FlexQueryResponse") || attr(root, "type")? != "AF" {
        return Err(error());
    }
    let statements = section(root, "FlexStatements")?;
    if attr(statements, "count")? != "1"
        || root.children().filter(Node::is_element).count() != 1
        || statements.children().filter(Node::is_element).count() != 1
    {
        return Err(error());
    }
    let node = section(statements, "FlexStatement")?;
    if attr(node, "accountId")? != expected_account {
        return Err(StatementParseError("statement_account_mismatch"));
    }
    let from_date = date(attr(node, "fromDate")?)?;
    let to_date = date(attr(node, "toDate")?)?;
    let start = midnight(from_date, tz)?;
    let end_exclusive = midnight(to_date.succ_opt().ok_or_else(error)?, tz)?;
    let generated_at = time(attr(node, "whenGenerated")?, tz)?;
    if start >= end_exclusive
        || generated_at < end_exclusive
        || generated_at > now
        || end_exclusive > now
    {
        return Err(StatementParseError("statement_requires_a_completed_period"));
    }
    let mut result = BrokerStatement {
        broker_account_id: expected_account.into(),
        from_date,
        to_date,
        timezone: timezone.into(),
        start,
        end_exclusive,
        generated_at,
        cash: Default::default(),
        executions: vec![],
        positions: vec![],
        cash_flows: vec![],
        blockers: BTreeSet::new(),
    };
    let mut seen = BTreeSet::new();
    for child in node.children().filter(Node::is_element) {
        if !seen.insert(child.tag_name().name()) {
            return Err(error());
        }
        match child.tag_name().name() {
            "Trades" | "OpenPositions" | "CashReport" | "CashTransactions"
            | "AccountInformation" | "SecuritiesInfo" => {}
            _ if child.children().any(|n| n.is_element()) || child.attributes().len() > 0 => {
                result
                    .blockers
                    .insert("unsupported_statement_section".into());
            }
            _ => {}
        }
    }
    rows::parse(&mut result, node, tz)?;
    Ok(result)
}
