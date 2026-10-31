//! Legacy grades command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_grade_subcommand;
use crate::command_routing::GradeSubcommand;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

pub(in crate::dispatcher) fn dispatch_grade(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    match parse_grade_subcommand(sub_matches) {
        GradeSubcommand::Show(sub_sub_matches) => grade_show(trust, sub_sub_matches, format)?,
        GradeSubcommand::Summary(sub_sub_matches) => grade_summary(trust, sub_sub_matches, format)?,
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn grade_show(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use std::str::FromStr;

    let trade_id = sub_matches
        .get_one::<String>("trade_id")
        .ok_or_else(|| output::report_error(format, "missing_trade_id", "Missing trade id"))?;
    let trade_id = Uuid::from_str(trade_id)
        .map_err(|_| output::report_error(format, "invalid_trade_id", "Invalid trade ID format"))?;

    let regrade = sub_matches.get_flag("regrade");
    let requested_weights = if sub_matches.get_one::<String>("weights").is_some() {
        Some(parse_grade_weights(sub_matches, format)?)
    } else {
        None
    };

    let existing = trust.latest_trade_grade(trade_id).map_err(|e| {
        output::report_error(
            format,
            "grade_read_failed",
            format!("Failed to read existing grade: {e}"),
        )
    })?;

    let grade = if let Some(existing) = existing {
        if regrade {
            let weights =
                requested_weights.unwrap_or(core::services::grading::GradingWeightsPermille {
                    process: existing.process_weight_permille,
                    risk: existing.risk_weight_permille,
                    execution: existing.execution_weight_permille,
                    documentation: existing.documentation_weight_permille,
                });
            trust
                .grade_trade(trade_id, weights)
                .map(|d| d.grade)
                .map_err(|e| {
                    output::report_error(
                        format,
                        "grade_compute_failed",
                        format!("Failed to compute grade: {e}"),
                    )
                })?
        } else {
            if let Some(weights) = requested_weights {
                let same_weights = existing.process_weight_permille == weights.process
                    && existing.risk_weight_permille == weights.risk
                    && existing.execution_weight_permille == weights.execution
                    && existing.documentation_weight_permille == weights.documentation;
                if !same_weights {
                    return Err(output::report_error(
                            format,
                            "weights_mismatch",
                            "Existing grade uses different weights. Re-run with --regrade or omit --weights.",
                        ));
                }
            }
            existing
        }
    } else {
        let weights = requested_weights
            .unwrap_or_else(core::services::grading::GradingWeightsPermille::default);
        trust
            .grade_trade(trade_id, weights)
            .map(|d| d.grade)
            .map_err(|e| {
                output::report_error(
                    format,
                    "grade_compute_failed",
                    format!("Failed to compute grade: {e}"),
                )
            })?
    };

    if format == ReportOutputFormat::Json {
        let payload = json!({
            "report": "grade_show",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "data": {
                "trade_id": grade.trade_id.to_string(),
                "overall": {
                    "grade": grade.overall_grade.to_string(),
                    "score": grade.overall_score,
                },
                "components": {
                    "process": grade.process_score,
                    "risk": grade.risk_score,
                    "execution": grade.execution_score,
                    "documentation": grade.documentation_score,
                },
                "weights_permille": {
                    "process": grade.process_weight_permille,
                    "risk": grade.risk_weight_permille,
                    "execution": grade.execution_weight_permille,
                    "documentation": grade.documentation_weight_permille,
                },
                "recommendations": grade.recommendations,
                "graded_at": grade.graded_at.format("%Y-%m-%dT%H:%M:%S").to_string()
            }
        });
        output::print_json(&payload)?;
        return Ok(());
    }

    println!("Trade Grade Report: {trade_id}");
    println!("=====================================");
    println!(
        "Overall Grade: {} ({}/100)",
        grade.overall_grade, grade.overall_score
    );
    println!();
    println!("Breakdown:");
    println!(
        "Process Adherence: {}/100 (weight {}‰)",
        grade.process_score, grade.process_weight_permille
    );
    println!(
        "Risk Management:   {}/100 (weight {}‰)",
        grade.risk_score, grade.risk_weight_permille
    );
    println!(
        "Execution Quality: {}/100 (weight {}‰)",
        grade.execution_score, grade.execution_weight_permille
    );
    println!(
        "Documentation:     {}/100 (weight {}‰)",
        grade.documentation_score, grade.documentation_weight_permille
    );
    println!();
    if grade.recommendations.is_empty() {
        println!("Recommendations: (none)");
    } else {
        println!("Recommendations:");
        for rec in &grade.recommendations {
            println!("• {rec}");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn grade_summary(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    use core::calculators_performance::PerformanceCalculator;
    use std::collections::HashMap;
    use std::str::FromStr;

    let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
        Some(Uuid::from_str(account_arg).map_err(|_| {
            output::report_error(format, "invalid_account_id", "Invalid account ID format")
        })?)
    } else {
        None
    };

    let days = sub_matches.get_one::<u32>("days").copied().unwrap_or(30);
    let weights = parse_grade_weights(sub_matches, format)?;

    let mut closed_trades = trust.search_closed_trades(account_id).map_err(|e| {
        output::report_error(
            format,
            "trading_data_unavailable",
            format!("Unable to retrieve closed trades: {e}"),
        )
    })?;

    closed_trades = PerformanceCalculator::filter_trades_by_days(&closed_trades, days);

    // Ensure every closed trade in window has a grade with the requested weights.
    for trade in &closed_trades {
        let latest = trust.latest_trade_grade(trade.id).unwrap_or(None);
        let needs = match latest {
            None => true,
            Some(g) => {
                g.process_weight_permille != weights.process
                    || g.risk_weight_permille != weights.risk
                    || g.execution_weight_permille != weights.execution
                    || g.documentation_weight_permille != weights.documentation
            }
        };
        if needs {
            let _ = trust.grade_trade(trade.id, weights).map_err(|e| {
                output::report_error(
                    format,
                    "grade_compute_failed",
                    format!("Failed to compute grade for trade {}: {e}", trade.id),
                )
            })?;
        }
    }

    let mut per_trade_latest: HashMap<Uuid, model::TradeGrade> = HashMap::new();
    for trade in &closed_trades {
        if let Ok(Some(g)) = trust.latest_trade_grade(trade.id) {
            per_trade_latest.insert(trade.id, g);
        }
    }

    let grades: Vec<model::TradeGrade> = per_trade_latest.into_values().collect();
    let count = u32::try_from(grades.len()).map_err(|_| {
        output::report_error(
            format,
            "grade_count_overflow",
            "Too many grades to summarize safely",
        )
    })?;

    let avg = |xs: Vec<u8>| -> Decimal {
        if xs.is_empty() {
            return dec!(0);
        }
        let sum: u32 = xs.into_iter().map(u32::from).sum();
        Decimal::from(sum)
            .checked_div(Decimal::from(count))
            .unwrap_or(dec!(0))
    };

    let overall_avg = avg(grades.iter().map(|g| g.overall_score).collect());
    let process_avg = avg(grades.iter().map(|g| g.process_score).collect());
    let risk_avg = avg(grades.iter().map(|g| g.risk_score).collect());
    let execution_avg = avg(grades.iter().map(|g| g.execution_score).collect());
    let documentation_avg = avg(grades.iter().map(|g| g.documentation_score).collect());

    let mut dist: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for g in &grades {
        let entry = dist.entry(g.overall_grade.to_string()).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    if format == ReportOutputFormat::Json {
        let payload = json!({
            "report": "grade_summary",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.map(|id| id.to_string()) },
            "filters": { "days": days },
            "data": {
                "trade_count": count,
                "average_overall_score": output::decimal_string(overall_avg),
                "component_averages": {
                    "process": output::decimal_string(process_avg),
                    "risk": output::decimal_string(risk_avg),
                    "execution": output::decimal_string(execution_avg),
                    "documentation": output::decimal_string(documentation_avg)
                },
                "distribution": dist,
                "weights_permille": {
                    "process": weights.process,
                    "risk": weights.risk,
                    "execution": weights.execution,
                    "documentation": weights.documentation
                }
            }
        });
        output::print_json(&payload)?;
        return Ok(());
    }

    println!("Trade Grading Summary (Last {days} days)");
    println!("===================================");
    println!("Average Score: {overall_avg}/100");
    println!("Grade Distribution:");
    for bucket in ["A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-", "D", "F"] {
        let n = dist.get(bucket).copied().unwrap_or(0);
        if count == 0 {
            println!("{bucket}: {n}");
        } else {
            let pct = Decimal::from(n)
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(Decimal::from(count)))
                .unwrap_or(dec!(0));
            println!("{bucket}: {n} ({pct}%)");
        }
    }
    println!();
    println!("Component Averages:");
    println!("Process:        {process_avg}/100");
    println!("Risk:           {risk_avg}/100");
    println!("Execution:      {execution_avg}/100");
    println!("Documentation:  {documentation_avg}/100");

    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(in crate::dispatcher) fn parse_grade_weights(
    matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<core::services::grading::GradingWeightsPermille, CliError> {
    use core::services::grading::GradingWeightsPermille;

    let Some(raw) = matches.get_one::<String>("weights") else {
        return Ok(GradingWeightsPermille::default());
    };

    let parts: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let [process_raw, risk_raw, execution_raw, documentation_raw] = parts.as_slice() else {
        return Err(output::report_error(
            format,
            "invalid_weights",
            "Weights must have 4 comma-separated numbers",
        ));
    };

    let parse_u16 = |s: &str| -> Result<u16, CliError> {
        s.parse::<u16>().map_err(|_| {
            output::report_error(format, "invalid_weights", "Weights must be valid integers")
        })
    };

    let a = parse_u16(process_raw)?;
    let b = parse_u16(risk_raw)?;
    let c = parse_u16(execution_raw)?;
    let d = parse_u16(documentation_raw)?;

    let sum = u32::from(a)
        .saturating_add(u32::from(b))
        .saturating_add(u32::from(c))
        .saturating_add(u32::from(d));

    let (process, risk, execution, documentation) = if sum == 100 {
        (
            a.saturating_mul(10),
            b.saturating_mul(10),
            c.saturating_mul(10),
            d.saturating_mul(10),
        )
    } else if sum == 1000 {
        (a, b, c, d)
    } else {
        return Err(output::report_error(
            format,
            "invalid_weights",
            "Weights must sum to 100 (percent) or 1000 (permille)",
        ));
    };

    let weights = GradingWeightsPermille {
        process,
        risk,
        execution,
        documentation,
    };
    Ok(weights)
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct GradeCommand;
impl super::Command for GradeCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_grade(trust, args)
    }
}
