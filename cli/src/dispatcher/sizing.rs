//! Legacy sizing command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::args;
use crate::dispatcher::error::CliError;
use crate::dispatcher::levels;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use model::Currency;
use model::Level;
use rust_decimal::Decimal;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(in crate::dispatcher) struct TradeHypothesisReport {
    pub(in crate::dispatcher) available_capital: Decimal,
    pub(in crate::dispatcher) capital_required: Decimal,
    pub(in crate::dispatcher) capital_required_pct_of_available: Option<Decimal>,
    pub(in crate::dispatcher) risk_per_share: Decimal,
    pub(in crate::dispatcher) reward_per_share: Decimal,
    pub(in crate::dispatcher) max_loss: Decimal,
    pub(in crate::dispatcher) max_loss_pct_of_available: Option<Decimal>,
    pub(in crate::dispatcher) max_gain: Decimal,
    pub(in crate::dispatcher) max_gain_pct_of_available: Option<Decimal>,
    pub(in crate::dispatcher) risk_reward_ratio: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub(in crate::dispatcher) struct TradeHypothesisArgs {
    pub(in crate::dispatcher) preview: TradePreviewArgs,
    pub(in crate::dispatcher) target_price: Decimal,
    pub(in crate::dispatcher) quantity: Decimal,
}

#[derive(Debug, Clone)]
pub(in crate::dispatcher) struct TradePreviewArgs {
    pub(in crate::dispatcher) account_id: Uuid,
    pub(in crate::dispatcher) entry_price: Decimal,
    pub(in crate::dispatcher) stop_price: Decimal,
    pub(in crate::dispatcher) currency: Currency,
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn trade_size_preview(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let preview = parse_trade_preview_args(trust, sub_matches, format)?;

    let sizing = trust
        .calculate_level_adjusted_quantity(
            preview.account_id,
            preview.entry_price,
            preview.stop_price,
            &preview.currency,
        )
        .map_err(|error| {
            output::report_error(
                format,
                "size_preview_failed",
                format!("Unable to calculate level-adjusted quantity: {error}"),
            )
        })?;

    let current_level = trust
        .level_for_account(preview.account_id)
        .map_err(|error| {
            output::report_error(
                format,
                "size_preview_level_load_failed",
                format!("Unable to read account level: {error}"),
            )
        })?;

    let risk_per_share = trade_preview_risk_per_share(&preview);
    let levels = size_preview_levels(
        sizing.base_quantity,
        current_level.current_level,
        risk_per_share,
    );

    match format {
        ReportOutputFormat::Text => {
            for line in trade_size_preview_text_lines(
                &preview,
                sizing.base_quantity,
                sizing.final_quantity,
                sizing.level_multiplier,
                &levels,
            ) {
                println!("{line}");
            }
        }
        ReportOutputFormat::Json => {
            let payload = trade_size_preview_payload(
                &preview,
                risk_per_share,
                sizing.base_quantity,
                current_level.current_level,
                sizing.level_multiplier,
                sizing.final_quantity,
                levels,
            );
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn trade_hypothesis(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let hypothesis_args = parse_trade_hypothesis_args(trust, sub_matches, format)?;

    let hypothesis = trust
        .calculate_trade_hypothesis(
            hypothesis_args.preview.account_id,
            hypothesis_args.preview.entry_price,
            hypothesis_args.preview.stop_price,
            hypothesis_args.target_price,
            hypothesis_args.quantity,
            &hypothesis_args.preview.currency,
        )
        .map_err(|error| {
            output::report_error(
                format,
                "trade_hypothesis_failed",
                format!("Unable to calculate trade hypothesis: {error}"),
            )
        })?;
    let report = TradeHypothesisReport {
        available_capital: hypothesis.available_capital,
        capital_required: hypothesis.capital_required,
        capital_required_pct_of_available: hypothesis.capital_required_pct_of_available,
        risk_per_share: hypothesis.risk_per_share,
        reward_per_share: hypothesis.reward_per_share,
        max_loss: hypothesis.max_loss,
        max_loss_pct_of_available: hypothesis.max_loss_pct_of_available,
        max_gain: hypothesis.max_gain,
        max_gain_pct_of_available: hypothesis.max_gain_pct_of_available,
        risk_reward_ratio: hypothesis.risk_reward_ratio,
    };

    match format {
        ReportOutputFormat::Text => {
            for line in trade_hypothesis_text_lines(&hypothesis_args, &report) {
                println!("{line}");
            }
        }
        ReportOutputFormat::Json => {
            let payload = trade_hypothesis_payload(&hypothesis_args, &report);
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn parse_trade_preview_args(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<TradePreviewArgs, CliError> {
    Ok(TradePreviewArgs {
        account_id: levels::resolve_level_account_id(trust, sub_matches, format)?,
        entry_price: args::parse_decimal_arg(sub_matches, "entry", format)?,
        stop_price: args::parse_decimal_arg(sub_matches, "stop", format)?,
        currency: parse_currency_arg(sub_matches, format)?,
    })
}

pub(in crate::dispatcher) fn parse_trade_hypothesis_args(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<TradeHypothesisArgs, CliError> {
    Ok(TradeHypothesisArgs {
        preview: parse_trade_preview_args(trust, sub_matches, format)?,
        target_price: args::parse_decimal_arg(sub_matches, "target", format)?,
        quantity: parse_positive_quantity_arg(sub_matches, format)?,
    })
}

pub(in crate::dispatcher) fn parse_currency_arg(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<Currency, CliError> {
    let currency_raw = sub_matches
        .get_one::<String>("currency")
        .map(String::as_str)
        .unwrap_or("usd");
    Currency::from_str(&currency_raw.to_ascii_uppercase()).map_err(|_| {
        output::report_error(
            format,
            "invalid_currency",
            format!("Unsupported currency '{currency_raw}'. Use USD, EUR, or BTC."),
        )
    })
}

pub(in crate::dispatcher) fn parse_positive_quantity_arg(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<Decimal, CliError> {
    let quantity_raw = sub_matches.get_one::<String>("quantity").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --quantity")
    })?;
    let quantity = Decimal::from_str(quantity_raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_quantity",
            format!("Invalid decimal quantity: {quantity_raw}"),
        )
    })?;
    if quantity <= Decimal::ZERO {
        return Err(output::report_error(
            format,
            "invalid_quantity",
            "Quantity must be greater than 0",
        ));
    }
    Ok(quantity)
}

pub(in crate::dispatcher) fn trade_preview_risk_per_share(preview: &TradePreviewArgs) -> Decimal {
    preview
        .entry_price
        .checked_sub(preview.stop_price)
        .map(|value| value.abs())
        .unwrap_or(Decimal::ZERO)
}

pub(in crate::dispatcher) fn size_preview_levels(
    base_quantity: Decimal,
    current_level: u8,
    risk_per_share: Decimal,
) -> Vec<Value> {
    (0_u8..=4_u8)
        .map(|level| {
            let multiplier = Level::multiplier_for_level(level).unwrap_or(Decimal::ZERO);
            let quantity = base_quantity
                .checked_mul(multiplier)
                .unwrap_or(Decimal::ZERO)
                .max(Decimal::ZERO);
            let risk_amount = risk_per_share
                .checked_mul(quantity)
                .unwrap_or(Decimal::ZERO);

            json!({
                "level": level,
                "description": Level::level_description(level),
                "multiplier": output::decimal_string(multiplier),
                "quantity": output::decimal_string(quantity),
                "risk_amount": output::decimal_string(risk_amount),
                "current": level == current_level,
            })
        })
        .collect()
}

pub(in crate::dispatcher) fn trade_size_preview_text_lines(
    preview: &TradePreviewArgs,
    base_quantity: Decimal,
    final_quantity: Decimal,
    level_multiplier: Decimal,
    levels: &[Value],
) -> Vec<String> {
    let mut lines = vec![
        "Position Size Preview".to_string(),
        "====================".to_string(),
        format!("Account: {}", preview.account_id),
        format!(
            "Entry: {} {} | Stop: {} {} | Risk/Share: {} {}",
            crate::zen::price_relative_to(preview.entry_price, preview.entry_price),
            preview.currency,
            crate::zen::price_relative_to(preview.stop_price, preview.entry_price),
            preview.currency,
            crate::zen::amount_share(trade_preview_risk_per_share(preview), preview.entry_price,),
            preview.currency
        ),
        format!(
            "Base quantity (rules only): {} | Current level-adjusted: {} ({}x)",
            base_quantity,
            final_quantity,
            output::decimal_string(level_multiplier)
        ),
        String::new(),
        "Level ladder:".to_string(),
    ];

    for item in levels {
        let current_marker = if item["current"].as_bool().unwrap_or(false) {
            "  <- current"
        } else {
            ""
        };
        lines.push(format!(
            "L{} ({}x): qty {} | risk {} {}{}",
            item["level"].as_u64().unwrap_or(0),
            item["multiplier"].as_str().unwrap_or("0"),
            item["quantity"].as_str().unwrap_or("0"),
            if crate::zen::is_enabled() {
                "hidden"
            } else {
                item["risk_amount"].as_str().unwrap_or("0")
            },
            preview.currency,
            current_marker
        ));
    }

    lines
}

pub(in crate::dispatcher) fn trade_size_preview_payload(
    preview: &TradePreviewArgs,
    risk_per_share: Decimal,
    base_quantity: Decimal,
    current_level: u8,
    current_multiplier: Decimal,
    current_quantity: Decimal,
    levels: Vec<Value>,
) -> Value {
    json!({
        "report": "trade_size_preview",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": {
            "account_id": preview.account_id.to_string(),
        },
        "data": {
            "currency": preview.currency.to_string(),
            "entry_price": output::decimal_string(preview.entry_price),
            "stop_price": output::decimal_string(preview.stop_price),
            "risk_per_share": output::decimal_string(risk_per_share),
            "base_quantity": output::decimal_string(base_quantity),
            "current_level": current_level,
            "current_multiplier": output::decimal_string(current_multiplier),
            "current_quantity": output::decimal_string(current_quantity),
            "levels": levels,
        }
    })
}

pub(in crate::dispatcher) fn format_optional_decimal(value: Option<Decimal>) -> String {
    value
        .map(output::decimal_string)
        .unwrap_or_else(|| "n/a".to_string())
}

pub(in crate::dispatcher) fn format_optional_percentage(value: Option<Decimal>) -> String {
    value
        .map(|decimal| format!("{}%", output::decimal_string(decimal)))
        .unwrap_or_else(|| "n/a".to_string())
}

pub(in crate::dispatcher) fn trade_hypothesis_text_lines(
    args: &TradeHypothesisArgs,
    report: &TradeHypothesisReport,
) -> Vec<String> {
    vec![
        "Trade Hypothesis".to_string(),
        "================".to_string(),
        format!("Account: {}", args.preview.account_id),
        format!(
            "Entry: {} {} | Stop: {} {} | Target: {} {} | Quantity: {}",
            crate::zen::price_relative_to(args.preview.entry_price, args.preview.entry_price),
            args.preview.currency,
            crate::zen::price_relative_to(args.preview.stop_price, args.preview.entry_price),
            args.preview.currency,
            crate::zen::price_relative_to(args.target_price, args.preview.entry_price),
            args.preview.currency,
            args.quantity
        ),
        format!(
            "Available capital: {} {}",
            crate::zen::amount_share(report.available_capital, report.available_capital),
            args.preview.currency
        ),
        format!(
            "Capital required: {} {} ({})",
            crate::zen::amount_share(report.capital_required, report.available_capital),
            args.preview.currency,
            format_optional_percentage(report.capital_required_pct_of_available)
        ),
        format!(
            "Risk/share: {} {} | Reward/share: {} {} | R:R {}",
            crate::zen::amount_share(report.risk_per_share, args.preview.entry_price),
            args.preview.currency,
            crate::zen::amount_share(report.reward_per_share, args.preview.entry_price),
            args.preview.currency,
            format_optional_decimal(report.risk_reward_ratio)
        ),
        format!(
            "Max loss: {} {} ({})",
            crate::zen::amount_share(report.max_loss, report.available_capital),
            args.preview.currency,
            format_optional_percentage(report.max_loss_pct_of_available)
        ),
        format!(
            "Max gain: {} {} ({})",
            crate::zen::amount_share(report.max_gain, report.available_capital),
            args.preview.currency,
            format_optional_percentage(report.max_gain_pct_of_available)
        ),
    ]
}

pub(in crate::dispatcher) fn trade_hypothesis_payload(
    args: &TradeHypothesisArgs,
    report: &TradeHypothesisReport,
) -> Value {
    json!({
        "report": "trade_hypothesis",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": {
            "account_id": args.preview.account_id.to_string(),
        },
        "data": {
            "currency": args.preview.currency.to_string(),
            "quantity": args.quantity,
            "available_capital": output::decimal_string(report.available_capital),
            "entry_price": output::decimal_string(args.preview.entry_price),
            "stop_price": output::decimal_string(args.preview.stop_price),
            "target_price": output::decimal_string(args.target_price),
            "capital_required": output::decimal_string(report.capital_required),
            "capital_required_pct_of_available": report.capital_required_pct_of_available.map(output::decimal_string),
            "risk_per_share": output::decimal_string(report.risk_per_share),
            "reward_per_share": output::decimal_string(report.reward_per_share),
            "max_loss": output::decimal_string(report.max_loss),
            "max_loss_pct_of_available": report.max_loss_pct_of_available.map(output::decimal_string),
            "max_gain": output::decimal_string(report.max_gain),
            "max_gain_pct_of_available": report.max_gain_pct_of_available.map(output::decimal_string),
            "risk_reward_ratio": report.risk_reward_ratio.map(output::decimal_string),
        }
    })
}
