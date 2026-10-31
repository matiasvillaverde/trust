//! Legacy bias report command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::autopsy;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::ArgMatches;
use core::calculators_performance::BiasAggregation;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

pub(in crate::dispatcher) fn bias_summary_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches
        .get_one::<String>("account")
        .ok_or_else(|| output::report_error(format, "missing_argument", "Missing --account"))?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let days = sub_matches.get_one::<i32>("days").copied().unwrap_or(7);
    let aggregation = trust
        .bias_aggregation_for_account(account.id, days)
        .map_err(|error| output::report_error(format, "report_failed", error.to_string()))?;

    match format {
        ReportOutputFormat::Text => print_bias_summary_text(&aggregation),
        ReportOutputFormat::Json => {
            let payload = bias_aggregation_payload(account.id, &aggregation);
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn print_bias_summary_text(aggregation: &BiasAggregation) {
    println!(
        "Bias Summary (last {} days, {} mistakes)",
        aggregation.window_days, aggregation.total_mistakes
    );
    println!("----------------------------------------");
    if let Some((tendency_number, count)) = aggregation.most_frequent_tendency {
        let label = autopsy::munger_tendency_label_by_number(tendency_number);
        let percentage = bias_count_percentage(count, aggregation.total_mistakes);
        println!(
            "Most frequent:  #{} {} ({}/{} = {}%)",
            tendency_number, label, count, aggregation.total_mistakes, percentage
        );
        if aggregation.mechanical_antidote_needed {
            println!("                -> Consider mechanical antidote");
        }
    } else {
        println!("Most frequent:  n/a");
    }
    println!("Lollapaloozas:  {}", aggregation.lollapalooza_count);
    println!("Omissions:      {}", aggregation.omission_count);
    println!("Commissions:    {}", aggregation.commission_count);
    println!(
        "Standdown:      {}",
        if aggregation.standdown_recommended {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "Mechanical antidote: {}",
        if aggregation.mechanical_antidote_needed {
            "Yes"
        } else {
            "No"
        }
    );
}

pub(in crate::dispatcher) fn bias_aggregation_payload(
    account_id: Uuid,
    aggregation: &BiasAggregation,
) -> Value {
    json!({
        "report": "bias_summary",
        "schema_version": 1,
        "generated_at": Utc::now().to_rfc3339(),
        "account_id": account_id.to_string(),
        "window_days": aggregation.window_days,
        "total_mistakes": aggregation.total_mistakes,
        "most_frequent_tendency": most_frequent_tendency_payload(aggregation),
        "lollapalooza_count": aggregation.lollapalooza_count,
        "omission_count": aggregation.omission_count,
        "commission_count": aggregation.commission_count,
        "standdown_recommended": aggregation.standdown_recommended,
        "mechanical_antidote_needed": aggregation.mechanical_antidote_needed,
    })
}

pub(in crate::dispatcher) fn most_frequent_tendency_payload(
    aggregation: &BiasAggregation,
) -> Value {
    aggregation
        .most_frequent_tendency
        .map(|(tendency_number, count)| {
            json!({
                "tendency_number": tendency_number,
                "count": count,
                "label": autopsy::munger_tendency_label_by_number(tendency_number),
                "percentage": bias_count_percentage(count, aggregation.total_mistakes),
            })
        })
        .unwrap_or(Value::Null)
}

pub(in crate::dispatcher) fn bias_count_percentage(count: i32, total: i32) -> String {
    if total <= 0 {
        return "0".to_string();
    }

    let percentage = Decimal::from(count)
        .checked_mul(dec!(100))
        .and_then(|value| value.checked_div(Decimal::from(total)))
        .unwrap_or(Decimal::ZERO)
        .round_dp(0);
    output::decimal_string(percentage)
}
