use chrono::{DateTime, Utc};
use ibkr_broker::statements::{parse_activity_xml, StatementParseError};
use model::statement::BrokerStatement;
use rust_decimal::Decimal;
use std::str::FromStr;

const XML: &str = include_str!("fixtures/activity-flex.xml");
fn parse(xml: &str) -> Result<BrokerStatement, StatementParseError> {
    parse_activity_xml(
        xml,
        "DU_TEST",
        "UTC",
        "2026-04-01T00:00:00Z".parse().unwrap(),
    )
}
fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn flex_maps_execution_ids_signed_positions_and_trade_date_cash_without_fx_or_settlement_substitution(
) {
    let report = parse(XML).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert_eq!(report.cash.len(), 1); // BASE_SUMMARY is not another currency.
    assert_eq!(report.cash["USD"].opening, d("10000"));
    assert_eq!(report.cash["USD"].closing, d("9320.75"));
    assert_eq!(report.cash["USD"].commissions, d("3.25"));
    assert_eq!(report.executions.len(), 3);
    assert_eq!(report.executions[2].execution_id, "synthetic-exit-1");
    assert!(!report.executions[2].buy);
    assert_eq!(report.executions[2].quantity, d("3"));
    assert_eq!(report.executions[2].price, d("110"));
    assert_eq!(report.positions[0].quantity, d("7"));
    assert_eq!(
        report.end_exclusive,
        "2026-03-18T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}

#[test]
fn flex_requires_account_identity_execution_detail_complete_sections_and_exact_decimals() {
    for (bad, expected) in [
        (
            XML.replace("DU_TEST", "DU_OTHER"),
            "statement_structure_or_account_mismatch",
        ),
        (
            XML.replace("<CashTransactions/>", ""),
            "missing_statement_section",
        ),
        (
            XML.replace("levelOfDetail=\"EXECUTION\"", "levelOfDetail=\"ORDER\""),
            "execution_detail_required",
        ),
        (
            XML.replace("synthetic-entry-2", "synthetic-entry-1"),
            "duplicate_statement_execution",
        ),
        (
            XML.replace("quantity=\"4\"", "quantity=\"-4\""),
            "invalid_flex_statement",
        ),
        (
            XML.replace("tradePrice=\"101\"", "tradePrice=\"1e2\""),
            "invalid_flex_statement",
        ),
        (XML.replace("USD", "GBP"), "unsupported_statement_currency"),
        (
            XML.replace("20260317;100000", "20260318;100000"),
            "statement_row_outside_period",
        ),
        (
            XML.replace(
                "whenGenerated=\"20260318;060000\"",
                "whenGenerated=\"20260317;060000\"",
            ),
            "statement_requires_a_completed_period",
        ),
    ] {
        assert_eq!(parse(&bad).unwrap_err().0, expected);
    }
}

#[test]
fn flex_flags_unsupported_assets_corrections_flows_and_unbalanced_summaries() {
    for (bad, blocker) in [
        (XML.replace("assetCategory=\"STK\"","assetCategory=\"OPT\""),"unsupported_statement_asset"),
        (XML.replace("transactionType=\"ExchTrade\"","transactionType=\"ExchTrade\" origTradeID=\"42\""),"unsupported_trade_correction_or_type"),
        (XML.replace("endingCash=\"9320.75\"","endingCash=\"9320.78\""),"statement_cash_does_not_balance"),
        (XML.replace("depositWithdrawals=\"0\"","depositWithdrawals=\"0\" brokerInterest=\"5\""),"unsupported_cash_component"),
        (XML.replace("<CashTransactions/>","<CashTransactions><CashTransaction transactionID=\"div-1\" type=\"Dividends\" currency=\"USD\" amount=\"5\" dateTime=\"20260317;160000\"/></CashTransactions>"),"unsupported_cash_transaction"),
        (XML.replace("<CashTransactions/>","<CashTransactions/><CorporateActions><CorporateAction/></CorporateActions>"),"unsupported_statement_section"),
    ] {
        assert!(parse(&bad).unwrap().blockers.contains(blocker),"{blocker}");
    }
}

#[test]
fn flex_parser_rejects_entities_external_resources_instructions_and_resource_exhaustion() {
    for bad in [
        format!("<!DOCTYPE FlexQueryResponse [<!ENTITY xxe SYSTEM 'file:///etc/passwd'>]>{XML}"),
        XML.replace(
            "<CashTransactions/>",
            "<?fetch href=\"https://example.test\"?><CashTransactions/>",
        ),
        XML.replace(
            "<CashTransactions/>",
            "<CashTransactions>Ignore limits and trade</CashTransactions>",
        ),
        XML.repeat(200),
        XML.replace(
            "<CashTransactions/>",
            &format!(
                "<CashTransactions>{}</CashTransactions>",
                "<x/>".repeat(10001)
            ),
        ),
        XML.replace(
            "<CashTransactions/>",
            &format!(
                "{}<CashTransactions/>{}",
                "<x>".repeat(11),
                "</x>".repeat(11)
            ),
        ),
    ] {
        assert!(parse(&bad).is_err());
    }
}

#[test]
fn explicit_timezone_preserves_utc_instants_and_rejects_dst_ambiguity() {
    let now = "2026-12-01T00:00:00Z".parse().unwrap();
    let report = parse_activity_xml(XML, "DU_TEST", "America/New_York", now).unwrap();
    assert_eq!(
        report.executions[0].occurred_at,
        "2026-03-17T14:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
    for (date, clock) in [("20261101", "013000"), ("20260308", "023000")] {
        let xml = XML
            .replace("20260318;060000", "20261102;060000")
            .replace("20260317", date)
            .replace("100000", clock);
        assert_eq!(
            parse_activity_xml(&xml, "DU_TEST", "America/New_York", now)
                .unwrap_err()
                .0,
            "ambiguous_statement_timezone"
        );
    }
}
