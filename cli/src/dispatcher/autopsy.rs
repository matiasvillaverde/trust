//! Legacy autopsy command responsibility. Business mutations go through TrustFacade.
use crate::dialogs::ConsoleDialogIo;
use crate::dialogs::DialogIo;
use crate::dispatcher::args;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Mistake;
use model::MistakeErrorType;
use model::MungerTendency;
use model::Status;
use model::Trade;
use model::TradeCategory;
use model::TradeGrade;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn trade_autopsy(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = ReportOutputFormat::Text;
    let trade_id = args::parse_uuid_arg(sub_matches, "trade-id", format)?;
    let trade = trust.read_trade(trade_id).map_err(|error| {
        output::report_error(
            format,
            "trade_not_found",
            format!("Trade {trade_id} not found: {error}"),
        )
    })?;

    if !is_closed_trade_status(trade.status) {
        return Err(output::report_error(
            format,
            "trade_not_closed",
            format!("Trade {trade_id} is not closed"),
        ));
    }

    let grade = trust
        .latest_trade_grade(trade.id)
        .map_err(|error| {
            output::report_error(
                format,
                "grade_read_failed",
                format!("Failed to read latest trade grade: {error}"),
            )
        })?
        .ok_or_else(|| {
            output::report_error(
                format,
                "trade_not_graded",
                format!("Trade {trade_id} has not been graded"),
            )
        })?;

    let mut io = ConsoleDialogIo::default();
    let mistake = trade_autopsy_with_io(&trade, &grade, &mut io)?;
    let persisted = trust.create_mistake(mistake).map_err(|error| {
        output::report_error(
            format,
            "mistake_create_failed",
            format!("Failed to store trade autopsy: {error}"),
        )
    })?;

    print_trade_autopsy_summary(&trade, &grade, &persisted);
    Ok(())
}

pub(in crate::dispatcher) fn trade_autopsy_with_io(
    trade: &Trade,
    grade: &TradeGrade,
    io: &mut dyn DialogIo,
) -> Result<Mistake, CliError> {
    print_trade_autopsy_context(trade, grade);
    let mut bias_tags = select_autopsy_biases(io)?;
    let lollapalooza = bias_tags.len() >= 2;
    if lollapalooza && !bias_tags.contains(&MungerTendency::Lollapalooza) {
        bias_tags.push(MungerTendency::Lollapalooza);
    }
    let error_type = select_autopsy_error_type(io)?;
    let rule_violated = read_optional_autopsy_text(io, "What rule was violated?")?;
    let counterfactual_r = read_counterfactual_r(io)?;
    let lesson = read_required_autopsy_text(io, "One-sentence lesson")?;
    let now = Utc::now().naive_utc();

    Ok(Mistake {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        trade_id: trade.id,
        bias_tags,
        lollapalooza,
        error_type,
        rule_violated,
        counterfactual_r,
        lesson,
    })
}

pub(in crate::dispatcher) fn print_trade_autopsy_context(trade: &Trade, grade: &TradeGrade) {
    let entry_price = trade
        .entry
        .average_filled_price
        .unwrap_or(trade.entry.unit_price);
    println!("Trade autopsy");
    println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
    println!(
        "Entry: {}",
        crate::zen::price_relative_to(entry_price, entry_price)
    );
    println!(
        "Exit: {}",
        trade_exit_price(trade)
            .map(|value| crate::zen::price_relative_to(value, entry_price))
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "R-multiple: {}",
        trade_r_multiple(trade)
            .map(output::decimal_string)
            .unwrap_or_else(|| "n/a".to_string())
    );
    println!(
        "Grade: {} ({}/100)",
        grade.overall_grade, grade.overall_score
    );
    println!(
        "Thesis: {}",
        trade
            .thesis
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("n/a")
    );
    println!();
    println!("Common firing patterns:");
    println!("Loser held too long: #11 + #14");
    println!("Doubled down on a loser: #14 + #5");
    println!("Chased a breakout late: #15 + #18");
    println!("Sized up after win streak: #12 + #13");
    println!("Followed a tip/guru: #22 + #15");
    println!("Two or more biases together: #25 Lollapalooza");
}

pub(in crate::dispatcher) fn print_trade_autopsy_summary(
    trade: &Trade,
    grade: &TradeGrade,
    mistake: &Mistake,
) {
    println!();
    println!("Stored trade autopsy: {}", mistake.id);
    println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
    println!(
        "Grade: {} ({}/100)",
        grade.overall_grade, grade.overall_score
    );
    println!(
        "Biases: {}",
        mistake
            .bias_tags
            .iter()
            .map(|tag| munger_tendency_display(*tag))
            .collect::<Vec<String>>()
            .join(", ")
    );
    println!(
        "Lollapalooza: {}",
        if mistake.lollapalooza { "yes" } else { "no" }
    );
    println!("Error type: {}", mistake.error_type);
    println!(
        "Rule violated: {}",
        mistake.rule_violated.as_deref().unwrap_or("n/a")
    );
    println!(
        "Counterfactual R: {}",
        output::decimal_string(mistake.counterfactual_r)
    );
    println!("Lesson: {}", mistake.lesson);
}

pub(in crate::dispatcher) fn select_autopsy_biases(
    io: &mut dyn DialogIo,
) -> Result<Vec<MungerTendency>, CliError> {
    let options = autopsy_bias_options();
    let labels = options
        .iter()
        .map(|tag| munger_tendency_display(*tag))
        .collect::<Vec<String>>();
    let selected = io
        .multi_select_indices("Which biases were active? (select all that apply)", &labels)
        .map_err(|error| {
            output::report_error(
                ReportOutputFormat::Text,
                "autopsy_cancelled",
                format!("Bias selection failed: {error}"),
            )
        })?;

    if selected.is_empty() {
        return Err(output::report_error(
            ReportOutputFormat::Text,
            "missing_bias_tags",
            "Select at least one active bias",
        ));
    }

    selected
        .into_iter()
        .map(|index| {
            options.get(index).copied().ok_or_else(|| {
                output::report_error(
                    ReportOutputFormat::Text,
                    "invalid_bias_selection",
                    format!("Invalid bias selection index: {index}"),
                )
            })
        })
        .collect()
}

pub(in crate::dispatcher) fn select_autopsy_error_type(
    io: &mut dyn DialogIo,
) -> Result<MistakeErrorType, CliError> {
    let labels = vec!["Commission".to_string(), "Omission".to_string()];
    let selected = io
        .select_index("Error of commission or omission?", &labels, 0)
        .map_err(|error| {
            output::report_error(
                ReportOutputFormat::Text,
                "autopsy_cancelled",
                format!("Error type selection failed: {error}"),
            )
        })?
        .ok_or_else(|| {
            output::report_error(
                ReportOutputFormat::Text,
                "autopsy_cancelled",
                "Autopsy cancelled before error type selection",
            )
        })?;

    match selected {
        0 => Ok(MistakeErrorType::Commission),
        1 => Ok(MistakeErrorType::Omission),
        _ => Err(output::report_error(
            ReportOutputFormat::Text,
            "invalid_error_type_selection",
            format!("Invalid error type selection index: {selected}"),
        )),
    }
}

pub(in crate::dispatcher) fn read_optional_autopsy_text(
    io: &mut dyn DialogIo,
    prompt: &str,
) -> Result<Option<String>, CliError> {
    let value = io.input_text(prompt, true).map_err(|error| {
        output::report_error(
            ReportOutputFormat::Text,
            "autopsy_cancelled",
            format!("Autopsy input failed: {error}"),
        )
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub(in crate::dispatcher) fn read_required_autopsy_text(
    io: &mut dyn DialogIo,
    prompt: &str,
) -> Result<String, CliError> {
    let value = io.input_text(prompt, false).map_err(|error| {
        output::report_error(
            ReportOutputFormat::Text,
            "autopsy_cancelled",
            format!("Autopsy input failed: {error}"),
        )
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(output::report_error(
            ReportOutputFormat::Text,
            "missing_autopsy_input",
            format!("{prompt} cannot be empty"),
        ));
    }
    Ok(trimmed.to_string())
}

pub(in crate::dispatcher) fn read_counterfactual_r(
    io: &mut dyn DialogIo,
) -> Result<Decimal, CliError> {
    let value = read_required_autopsy_text(
        io,
        "What would have happened if you had followed the rule? (counterfactual R-multiple)",
    )?;
    Decimal::from_str(&value).map_err(|_| {
        output::report_error(
            ReportOutputFormat::Text,
            "invalid_counterfactual_r",
            format!("Invalid counterfactual R-multiple: {value}"),
        )
    })
}

pub(in crate::dispatcher) fn autopsy_bias_options() -> Vec<MungerTendency> {
    MungerTendency::all()
        .into_iter()
        .filter(|tag| *tag != MungerTendency::Lollapalooza)
        .collect()
}

pub(in crate::dispatcher) fn munger_tendency_display(tag: MungerTendency) -> String {
    format!("#{} {}", tag.number(), munger_tendency_label(tag))
}

pub(in crate::dispatcher) fn munger_tendency_label_by_number(number: i32) -> String {
    MungerTendency::all()
        .into_iter()
        .find(|tag| i32::from(tag.number()) == number)
        .map(munger_tendency_label)
        .unwrap_or("Unknown")
        .to_string()
}

pub(in crate::dispatcher) fn munger_tendency_label(tag: MungerTendency) -> &'static str {
    match tag {
        MungerTendency::RewardPunishment => "Reward/Punishment Superresponse",
        MungerTendency::InconsistencyAvoidance => "Inconsistency-Avoidance",
        MungerTendency::MereAssociation => "Influence-from-Mere-Association",
        MungerTendency::PainAvoidingDenial => "Pain-Avoiding Psychological Denial",
        MungerTendency::ExcessiveSelfRegard => "Excessive Self-Regard",
        MungerTendency::Overoptimism => "Overoptimism",
        MungerTendency::DeprivalSuperreaction => "Deprival Superreaction",
        MungerTendency::SocialProof => "Social-Proof",
        MungerTendency::ContrastMisreaction => "Contrast-Misreaction",
        MungerTendency::StressInfluence => "Stress-Influence",
        MungerTendency::AvailabilityMisweighing => "Availability-Misweighing",
        MungerTendency::AuthorityMisinfluence => "Authority-Misinfluence",
        MungerTendency::Lollapalooza => "Lollapalooza",
    }
}

pub(in crate::dispatcher) fn is_closed_trade_status(status: Status) -> bool {
    matches!(status, Status::ClosedTarget | Status::ClosedStopLoss)
}

pub(in crate::dispatcher) fn trade_exit_price(trade: &Trade) -> Option<Decimal> {
    match trade.status {
        Status::ClosedTarget => Some(
            trade
                .target
                .average_filled_price
                .unwrap_or(trade.target.unit_price),
        ),
        Status::ClosedStopLoss => Some(
            trade
                .safety_stop
                .average_filled_price
                .unwrap_or(trade.safety_stop.unit_price),
        ),
        _ => None,
    }
}

pub(in crate::dispatcher) fn trade_r_multiple(trade: &Trade) -> Option<Decimal> {
    let entry = trade
        .entry
        .average_filled_price
        .unwrap_or(trade.entry.unit_price);
    let stop = trade.safety_stop.unit_price;
    let exit = trade_exit_price(trade)?;
    let risk = match trade.category {
        TradeCategory::Long => entry.checked_sub(stop)?,
        TradeCategory::Short => stop.checked_sub(entry)?,
    };
    if risk <= Decimal::ZERO {
        return None;
    }
    let pnl = match trade.category {
        TradeCategory::Long => exit.checked_sub(entry)?,
        TradeCategory::Short => entry.checked_sub(exit)?,
    };
    pnl.checked_div(risk)
}
