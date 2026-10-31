//! Legacy sessions command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_session_subcommand;
use crate::command_routing::SessionSubcommand;
use crate::dialogs::ConsoleDialogIo;
use crate::dialogs::DialogIo;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use advisor::RegimeConfig;
use advisor::RegimeRequest;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::Account;
use model::SessionPlan;
use model::SessionPlanClose;
use model::SessionRegime;
use model::Trade;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(in crate::dispatcher) struct SessionTradeReviewRow {
    pub(in crate::dispatcher) trade_id: Uuid,
    pub(in crate::dispatcher) symbol: String,
    pub(in crate::dispatcher) setup: Option<String>,
    pub(in crate::dispatcher) planned: bool,
}

pub(in crate::dispatcher) fn dispatch_session(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    match parse_session_subcommand(sub_matches) {
        SessionSubcommand::Open(sub_sub_matches) => session_open(trust, sub_sub_matches, format)?,
        SessionSubcommand::Close(sub_sub_matches) => session_close(trust, sub_sub_matches, format)?,
        SessionSubcommand::List(sub_sub_matches) => session_list(trust, sub_sub_matches, format)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn session_open(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --account for session open",
        )
    })?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    if trust
        .open_session_for_account(account.id)
        .map_err(|error| {
            output::report_error(
                format,
                "session_lookup_failed",
                format!("Failed to read open session: {error}"),
            )
        })?
        .is_some()
    {
        return Err(output::report_error(
            format,
            "session_already_open",
            format!("Account '{}' already has an open session", account.name),
        ));
    }

    let suggested = suggest_session_regime(trust, &account);
    let mut io = ConsoleDialogIo::default();
    let plan = session_plan_from_dialog(&mut io, &account, suggested, format)?;
    let saved = trust.create_session_plan(plan).map_err(|error| {
        output::report_error(
            format,
            "session_open_failed",
            format!("Failed to open session: {error}"),
        )
    })?;

    print_session_open_summary(&saved, &account, format)
}

pub(in crate::dispatcher) fn session_close(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let (account, plan) = open_session_target(trust, sub_matches, format)?;
    let closed_at = Utc::now().naive_utc();
    let actual_regime = suggest_session_regime(trust, &account);
    let trades = trust
        .trades_for_account_in_period(account.id, plan.opened_at, closed_at)
        .map_err(|error| {
            output::report_error(
                format,
                "session_trade_lookup_failed",
                format!("Failed to load session trades: {error}"),
            )
        })?;
    let review_rows = session_trade_review_rows(&plan, &trades);

    print_session_close_comparison(&plan, &account, actual_regime, &review_rows, format)?;

    let mut io = ConsoleDialogIo::default();
    let grade = select_session_grade(&mut io, format)?;
    let notes = read_optional_session_text(&mut io, "Adherence notes", format)?;
    let close = SessionPlanClose {
        session_plan_id: plan.id,
        closed_at,
        session_grade: Some(grade),
        adherence_notes: notes,
    };
    let closed = trust.close_session_plan(close).map_err(|error| {
        output::report_error(
            format,
            "session_close_failed",
            format!("Failed to close session: {error}"),
        )
    })?;

    print_session_closed_summary(&closed, &account, &review_rows, format)
}

pub(in crate::dispatcher) fn session_list(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            "Missing --account for session list",
        )
    })?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let sessions = trust
        .session_plans_for_account(
            account.id,
            chrono::NaiveDateTime::MIN,
            Utc::now().naive_utc(),
        )
        .map_err(|error| {
            output::report_error(
                format,
                "session_list_failed",
                format!("Failed to list sessions: {error}"),
            )
        })?;

    if format == ReportOutputFormat::Json {
        let payload = json!({
            "account": {
                "id": account.id.to_string(),
                "name": account.name,
            },
            "sessions": sessions.iter().map(session_plan_payload).collect::<Vec<_>>(),
        });
        return output::print_json(&payload);
    }

    println!("Session history for {} ({})", account.name, account.id);
    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }
    println!(
        "{:<19} {:<19} {:<9} {:<5} {:<8} setups",
        "opened_at", "closed_at", "regime", "max", "grade"
    );
    for session in sessions {
        println!(
            "{:<19} {:<19} {:<9} {:<5} {:<8} {}",
            session.opened_at,
            session
                .closed_at
                .map(|closed_at| closed_at.to_string())
                .unwrap_or_else(|| "open".to_string()),
            session.regime,
            session.max_positions,
            session.session_grade.as_deref().unwrap_or("-"),
            session.permitted_setups.join(", ")
        );
    }
    Ok(())
}

pub(in crate::dispatcher) fn open_session_target(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(Account, SessionPlan), CliError> {
    if let Some(account_raw) = sub_matches.get_one::<String>("account") {
        let account = context::resolve_account_arg(trust, account_raw, format)?;
        let plan = trust
            .open_session_for_account(account.id)
            .map_err(|error| {
                output::report_error(
                    format,
                    "session_lookup_failed",
                    format!("Failed to read open session: {error}"),
                )
            })?
            .ok_or_else(|| {
                output::report_error(
                    format,
                    "no_open_session",
                    format!("No open session for account '{}'", account.name),
                )
            })?;
        return Ok((account, plan));
    }

    let accounts = trust.search_all_accounts().map_err(|error| {
        output::report_error(
            format,
            "accounts_unavailable",
            format!("Failed to load accounts: {error}"),
        )
    })?;
    let mut open = Vec::new();
    for account in accounts {
        if let Some(plan) = trust
            .open_session_for_account(account.id)
            .map_err(|error| {
                output::report_error(
                    format,
                    "session_lookup_failed",
                    format!("Failed to read open session: {error}"),
                )
            })?
        {
            open.push((account, plan));
        }
    }

    match open.len() {
        0 => Err(output::report_error(
            format,
            "no_open_session",
            "No open session found",
        )),
        1 => open.into_iter().next().ok_or_else(|| {
            output::report_error(format, "no_open_session", "No open session found")
        }),
        _ => Err(output::report_error(
            format,
            "ambiguous_open_session",
            "More than one session is open; pass --account",
        )),
    }
}

pub(in crate::dispatcher) fn session_plan_from_dialog(
    io: &mut dyn DialogIo,
    account: &Account,
    suggested_regime: Option<SessionRegime>,
    format: ReportOutputFormat,
) -> Result<SessionPlan, CliError> {
    let regime = select_session_regime(io, suggested_regime, format)?;
    let permitted_setups = read_session_setups(io, format)?;
    let max_positions = read_session_max_positions(io, format)?;
    let hypothesis = read_required_session_text(io, "Hypothesis", 500, format)?;
    let success_criteria = read_required_session_text(io, "Success criteria", 500, format)?;
    let failure_criteria = read_required_session_text(io, "Failure criteria", 500, format)?;
    let now = Utc::now().naive_utc();

    Ok(SessionPlan {
        id: Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
        account_id: account.id,
        opened_at: now,
        closed_at: None,
        regime,
        permitted_setups,
        max_positions,
        hypothesis,
        success_criteria,
        failure_criteria,
        session_grade: None,
        adherence_notes: None,
    })
}

pub(in crate::dispatcher) fn select_session_regime(
    io: &mut dyn DialogIo,
    suggested: Option<SessionRegime>,
    format: ReportOutputFormat,
) -> Result<SessionRegime, CliError> {
    let regimes = [
        SessionRegime::Calm,
        SessionRegime::Normal,
        SessionRegime::Elevated,
    ];
    let labels = regimes
        .iter()
        .map(|regime| {
            let label = session_regime_label(*regime);
            if Some(*regime) == suggested {
                format!("{label} (suggested)")
            } else {
                label.to_string()
            }
        })
        .collect::<Vec<_>>();
    let default = regimes
        .iter()
        .position(|regime| Some(*regime) == suggested)
        .unwrap_or(1);
    let selected = io
        .select_index("Regime today?", &labels, default)
        .map_err(|error| {
            output::report_error(
                format,
                "session_dialog_failed",
                format!("Failed to select regime: {error}"),
            )
        })?
        .ok_or_else(|| {
            output::report_error(format, "session_dialog_cancelled", "Session open cancelled")
        })?;
    regimes.get(selected).copied().ok_or_else(|| {
        output::report_error(format, "invalid_session_regime", "Invalid regime selection")
    })
}

pub(in crate::dispatcher) fn read_session_setups(
    io: &mut dyn DialogIo,
    format: ReportOutputFormat,
) -> Result<Vec<String>, CliError> {
    let raw = read_required_session_text(io, "Permitted setups (comma-separated)", 500, format)?;
    model::parse_session_setups(&raw).map_err(|error| {
        output::report_error(
            format,
            "invalid_session_setups",
            format!("Invalid permitted setups: {error:?}"),
        )
    })
}

pub(in crate::dispatcher) fn read_session_max_positions(
    io: &mut dyn DialogIo,
    format: ReportOutputFormat,
) -> Result<i32, CliError> {
    let raw = read_required_session_text(io, "Max new positions", 32, format)?;
    let max_positions = raw.trim().parse::<i32>().map_err(|_| {
        output::report_error(
            format,
            "invalid_session_max_positions",
            "Max new positions must be an integer",
        )
    })?;
    if max_positions < 0 {
        return Err(output::report_error(
            format,
            "invalid_session_max_positions",
            "Max new positions cannot be negative",
        ));
    }
    Ok(max_positions)
}

pub(in crate::dispatcher) fn select_session_grade(
    io: &mut dyn DialogIo,
    format: ReportOutputFormat,
) -> Result<String, CliError> {
    let labels = ["A", "B", "C", "D", "F"]
        .iter()
        .map(|grade| (*grade).to_string())
        .collect::<Vec<_>>();
    let selected = io
        .select_index("Session grade", &labels, 2)
        .map_err(|error| {
            output::report_error(
                format,
                "session_dialog_failed",
                format!("Failed to select grade: {error}"),
            )
        })?
        .ok_or_else(|| {
            output::report_error(
                format,
                "session_dialog_cancelled",
                "Session close cancelled",
            )
        })?;
    labels.get(selected).cloned().ok_or_else(|| {
        output::report_error(
            format,
            "invalid_session_grade",
            "Invalid session grade selection",
        )
    })
}

pub(in crate::dispatcher) fn read_required_session_text(
    io: &mut dyn DialogIo,
    prompt: &str,
    max_chars: usize,
    format: ReportOutputFormat,
) -> Result<String, CliError> {
    let value = io.input_text(prompt, false).map_err(|error| {
        output::report_error(
            format,
            "session_dialog_failed",
            format!("Failed to read {prompt}: {error}"),
        )
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(output::report_error(
            format,
            "invalid_session_text",
            format!("{prompt} cannot be blank"),
        ));
    }
    if trimmed.chars().count() > max_chars {
        return Err(output::report_error(
            format,
            "invalid_session_text",
            format!("{prompt} cannot exceed {max_chars} characters"),
        ));
    }
    Ok(trimmed.to_string())
}

pub(in crate::dispatcher) fn read_optional_session_text(
    io: &mut dyn DialogIo,
    prompt: &str,
    format: ReportOutputFormat,
) -> Result<Option<String>, CliError> {
    let value = io.input_text(prompt, true).map_err(|error| {
        output::report_error(
            format,
            "session_dialog_failed",
            format!("Failed to read {prompt}: {error}"),
        )
    })?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub(in crate::dispatcher) fn suggest_session_regime(
    trust: &mut TrustFacade,
    account: &Account,
) -> Option<SessionRegime> {
    trust
        .regime_advisory(&RegimeRequest::default(), account, RegimeConfig::default())
        .ok()
        .map(|snapshot| match snapshot.composite {
            advisor::CompositeRegime::CalmTrending => SessionRegime::Calm,
            advisor::CompositeRegime::NormalTrending => SessionRegime::Normal,
            advisor::CompositeRegime::ElevatedTrending
            | advisor::CompositeRegime::Choppy
            | advisor::CompositeRegime::ElevatedChoppy
            | advisor::CompositeRegime::NarrowBreadth => SessionRegime::Elevated,
        })
}

pub(in crate::dispatcher) fn session_trade_review_rows(
    plan: &SessionPlan,
    trades: &[Trade],
) -> Vec<SessionTradeReviewRow> {
    trades
        .iter()
        .map(|trade| {
            let setup = trade_setup_type(trade);
            let planned = setup
                .as_deref()
                .is_some_and(|value| setup_is_permitted(&plan.permitted_setups, value));
            SessionTradeReviewRow {
                trade_id: trade.id,
                symbol: trade.trading_vehicle.symbol.clone(),
                setup,
                planned,
            }
        })
        .collect()
}

pub(in crate::dispatcher) fn trade_setup_type(trade: &Trade) -> Option<String> {
    trade
        .context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(in crate::dispatcher) fn setup_is_permitted(permitted_setups: &[String], setup: &str) -> bool {
    permitted_setups
        .iter()
        .any(|permitted| permitted.eq_ignore_ascii_case(setup.trim()))
}

pub(in crate::dispatcher) fn session_regime_label(regime: SessionRegime) -> &'static str {
    match regime {
        SessionRegime::Calm => "Calm",
        SessionRegime::Normal => "Normal",
        SessionRegime::Elevated => "Elevated",
    }
}

pub(in crate::dispatcher) fn print_session_open_summary(
    plan: &SessionPlan,
    account: &Account,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    if format == ReportOutputFormat::Json {
        return output::print_json(&json!({
            "session": session_plan_payload(plan),
            "account": {
                "id": account.id.to_string(),
                "name": account.name,
            },
        }));
    }

    println!("Session opened:");
    println!("  account: {} ({})", account.name, account.id);
    println!("  session_id: {}", plan.id);
    println!("  opened_at: {}", plan.opened_at);
    println!("  regime: {}", plan.regime);
    println!("  permitted_setups: {}", plan.permitted_setups.join(", "));
    println!("  max_new_positions: {}", plan.max_positions);
    Ok(())
}

pub(in crate::dispatcher) fn print_session_close_comparison(
    plan: &SessionPlan,
    account: &Account,
    actual_regime: Option<SessionRegime>,
    review_rows: &[SessionTradeReviewRow],
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    if format == ReportOutputFormat::Json {
        return output::print_json(&json!({
            "comparison": session_comparison_payload(plan, account, actual_regime, review_rows),
        }));
    }

    println!("Session review:");
    println!("  account: {} ({})", account.name, account.id);
    println!("  opened_at: {}", plan.opened_at);
    println!(
        "  regime: planned {} | actual {}",
        plan.regime,
        actual_regime
            .map(|regime| regime.to_string())
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!(
        "  positions: planned max {} | actual {}",
        plan.max_positions,
        review_rows.len()
    );
    println!("Trades:");
    if review_rows.is_empty() {
        println!("  none");
    } else {
        for row in review_rows {
            let marker = if row.planned { "planned" } else { "UNPLANNED" };
            println!(
                "  {} {} setup={} {}",
                row.trade_id,
                row.symbol,
                row.setup.as_deref().unwrap_or("none"),
                marker
            );
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn print_session_closed_summary(
    plan: &SessionPlan,
    account: &Account,
    review_rows: &[SessionTradeReviewRow],
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    if format == ReportOutputFormat::Json {
        return output::print_json(&json!({
            "closed_session": session_plan_payload(plan),
            "comparison": session_comparison_payload(plan, account, None, review_rows),
        }));
    }

    println!("Session closed:");
    println!("  session_id: {}", plan.id);
    println!("  closed_at: {}", plan.closed_at.unwrap_or(plan.updated_at));
    println!("  grade: {}", plan.session_grade.as_deref().unwrap_or("-"));
    println!(
        "  unplanned_trades: {}",
        review_rows.iter().filter(|row| !row.planned).count()
    );
    Ok(())
}

pub(in crate::dispatcher) fn session_plan_payload(plan: &SessionPlan) -> Value {
    json!({
        "id": plan.id.to_string(),
        "created_at": plan.created_at.to_string(),
        "updated_at": plan.updated_at.to_string(),
        "account_id": plan.account_id.to_string(),
        "opened_at": plan.opened_at.to_string(),
        "closed_at": plan.closed_at.map(|value| value.to_string()),
        "regime": plan.regime.to_string(),
        "permitted_setups": &plan.permitted_setups,
        "max_positions": plan.max_positions,
        "hypothesis": &plan.hypothesis,
        "success_criteria": &plan.success_criteria,
        "failure_criteria": &plan.failure_criteria,
        "session_grade": &plan.session_grade,
        "adherence_notes": &plan.adherence_notes,
    })
}

pub(in crate::dispatcher) fn session_comparison_payload(
    plan: &SessionPlan,
    account: &Account,
    actual_regime: Option<SessionRegime>,
    review_rows: &[SessionTradeReviewRow],
) -> Value {
    json!({
        "account": {
            "id": account.id.to_string(),
            "name": account.name,
        },
        "session_id": plan.id.to_string(),
        "planned_regime": plan.regime.to_string(),
        "actual_regime": actual_regime.map(|regime| regime.to_string()),
        "planned_max_positions": plan.max_positions,
        "actual_positions": review_rows.len(),
        "trades": review_rows.iter().map(|row| {
            json!({
                "trade_id": row.trade_id.to_string(),
                "symbol": &row.symbol,
                "setup": &row.setup,
                "planned": row.planned,
                "unplanned": !row.planned,
            })
        }).collect::<Vec<_>>(),
    })
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct SessionCommand;
impl super::Command for SessionCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_session(trust, args)
    }
}
