//! Legacy level output command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::output;
use core::services::leveling::LevelCriterionProgress;
use core::services::leveling::LevelEvaluationOutcome;
use core::services::leveling::LevelPathProgress;
use core::services::leveling::LevelProgressReport;
use serde_json::json;
use serde_json::Value;
use uuid::Uuid;

pub(in crate::dispatcher) fn print_level_evaluation_text(
    outcome: &LevelEvaluationOutcome,
    apply: bool,
) {
    println!("Level evaluation complete.");
    println!(
        "Current: L{} ({})",
        outcome.current_level.current_level,
        model::Level::level_description(outcome.current_level.current_level)
    );
    if let Some(decision) = &outcome.decision {
        println!(
            "Decision: {:?} to L{} ({})",
            decision.direction, decision.target_level, decision.reason
        );
    } else {
        println!("Decision: no change");
    }
    if let Some(applied) = &outcome.applied_level {
        println!("Applied: new level L{}", applied.current_level);
    } else if apply {
        println!("Applied: no change");
    }
    print_level_progress_text(&outcome.progress);
}

pub(in crate::dispatcher) fn level_evaluation_payload(
    account_id: Uuid,
    apply: bool,
    outcome: &LevelEvaluationOutcome,
) -> Value {
    json!({
        "report": "level_evaluate",
        "format_version": 1,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "scope": { "account_id": account_id.to_string() },
        "apply": apply,
        "data": {
            "current_level": outcome.current_level.current_level,
            "decision": outcome.decision.as_ref().map(|decision| {
                json!({
                    "target_level": decision.target_level,
                    "direction": format!("{:?}", decision.direction),
                    "reason": decision.reason,
                    "trigger_type": decision.trigger_type.to_string()
                })
            }),
            "applied_level": outcome.applied_level.as_ref().map(|level| level.current_level),
            "progress": level_progress_payload(&outcome.progress),
        }
    })
}

pub(in crate::dispatcher) fn print_level_progress_text(progress: &LevelProgressReport) {
    println!("Progress to adjacent levels:");

    if progress.upgrade_paths.is_empty() {
        println!("- Upgrade: unavailable at current bounds");
    } else {
        for path in &progress.upgrade_paths {
            print_level_path_text("Upgrade", path);
        }
    }

    if progress.downgrade_paths.is_empty() {
        println!("- Downgrade: unavailable at current bounds");
    } else {
        for path in &progress.downgrade_paths {
            print_level_path_text("Downgrade", path);
        }
    }
}

pub(in crate::dispatcher) fn print_level_path_text(
    direction_label: &str,
    path: &LevelPathProgress,
) {
    let target = path
        .target_level
        .map_or_else(|| "n/a".to_string(), |level| format!("L{level}"));
    println!(
        "- {direction_label} via {} -> {} [{}]",
        path.path,
        target,
        if path.all_met { "ready" } else { "not ready" }
    );
    for criterion in &path.criteria {
        print_level_criterion_text(criterion);
    }
}

pub(in crate::dispatcher) fn print_level_criterion_text(criterion: &LevelCriterionProgress) {
    println!(
        "  - {} {} {} (actual {}, missing {})",
        criterion.key,
        criterion.comparator,
        criterion.threshold,
        criterion.actual,
        criterion.missing
    );
}

pub(in crate::dispatcher) fn level_progress_payload(progress: &LevelProgressReport) -> Value {
    json!({
        "current_level": progress.current_level,
        "status": progress.status.to_string(),
        "upgrade_paths": progress.upgrade_paths.iter().map(level_path_payload).collect::<Vec<_>>(),
        "downgrade_paths": progress.downgrade_paths.iter().map(level_path_payload).collect::<Vec<_>>(),
    })
}

pub(in crate::dispatcher) fn level_path_payload(path: &LevelPathProgress) -> Value {
    json!({
        "path": path.path,
        "trigger_type": path.trigger_type.to_string(),
        "direction": format!("{:?}", path.direction),
        "target_level": path.target_level,
        "all_met": path.all_met,
        "criteria": path.criteria.iter().map(level_criterion_payload).collect::<Vec<_>>(),
    })
}

pub(in crate::dispatcher) fn level_criterion_payload(criterion: &LevelCriterionProgress) -> Value {
    json!({
        "key": criterion.key,
        "comparator": criterion.comparator,
        "actual": output::decimal_string(criterion.actual),
        "threshold": output::decimal_string(criterion.threshold),
        "missing": output::decimal_string(criterion.missing),
        "met": criterion.met,
    })
}
