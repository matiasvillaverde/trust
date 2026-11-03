#!/usr/bin/env python3
"""Run explicit Trust assurance levels; never promote simulated evidence to broker acceptance."""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs/reliability/assurance.json"
EVIDENCE = ROOT / ".local/assurance"
CARGO = os.environ.get("CARGO", "cargo")
TAIL = ["--", "--test-threads=1"]
REQUIRED = {
    1: {"independent_cash_handles_partial_fills_partial_exits_and_late_fee_booking",
        "short_cash_uses_sale_proceeds_and_buy_to_cover_not_margin_reservations",
        "new_accounts_use_points_and_reject_invalid_reserves_without_rows",
        "legacy_upgrade_preserves_old_tax_liability_and_new_values_survive_reopen",
        "backups_roundtrip_explicit_units_and_restore_legacy_v2_without_guessing_new_values",
        "malformed_percentage_units_or_negative_legacy_values_fail_closed",
        "one_and_two_percent_have_consistent_tax_liability_on_realized_profit",
        "events::trade::tests::long_and_short_brackets_have_the_same_reward_in_risk_units",
        "small_pages_do_not_decode_trades_outside_the_page_or_scope",
        "historical_gains_do_not_grant_an_agent_extra_daily_loss_capacity",
        "shared_heat_reserves_across_profiles_and_rejects_one_decimal_over_boundary",
        "funding_and_submission_do_not_double_charge_or_release_previous_reservation",
        "margin_buffer_reserves_cash_and_stresses_maintenance",
        "concentration_groups_aggregate_without_offset_credit",
        "gap_stress_and_fees_can_bind_with_safe_planned_stop",
        "tight_stops_cannot_evade_atr_floor_and_submission_preserves_floor",
        "throttle_drops_immediately_and_recovers_only_one_timed_step",
        "agent_loss_entry_turnover_and_cooldown_survive_policy_replacement",
        "a_protective_order_cannot_be_credited_to_two_trades_or_legs",
        "nonzero_risk_that_underflows_decimal_precision_cannot_authorize_an_entry",
        "nested_risk_policy_rejects_lossy_money_and_untyped_fields_before_dispatch",
        "services::leveling::tests::losing_window_never_upgrades_even_with_many_winners",
        "submission::submission_revalidates_changed_trade_and_monthly_caps_without_broker_effects",
        "workers::worker_transaction::tests::ledger::checkpoints::cash_reads_rebind_account_currency_and_preserve_fractional_appends",
        "workers::worker_transaction::tests::ledger::checkpoints::historical_edits_deletion_scope_moves_and_corrupt_receipts_invalidate_cash",
        "workers::worker_transaction::tests::ledger::checkpoints::nested_rollbacks_restore_cash_and_do_not_reuse_abandoned_prefixes",
        "workers::worker_transaction::tests::ledger::checkpoints::external_commits_and_reader_snapshots_never_publish_stale_cash",
        "workers::worker_transaction::tests::ledger::checkpoints::backdated_cash_preserves_ordered_overflow_rejection",
        "workers::worker_transaction::tests::ledger::checkpoints::missing_or_replaced_monitor_triggers_force_reverification",
        "workers::worker_transaction::tests::ledger::checkpoints::replace_by_identity_or_rowid_cannot_double_count_prior_cash",
        "workers::worker_transaction::tests::ledger::checkpoints::same_timestamp_earlier_ids_replay_before_the_saved_boundary",
        "workers::worker_transaction::tests::ledger::checkpoints::malformed_trade_identity_cannot_hide_behind_an_existing_cash_checkpoint",
        "workers::worker_transaction::tests::ledger::checkpoints::read_only_or_file_backed_temp_storage_uses_exact_full_replay",
        "workers::worker_transaction::tests::ledger::checkpoints::monthly_reference_query_branches_share_one_snapshot",
        "workers::worker_transaction::tests::ledger::checkpoints::malformed_cursor_storage_cannot_repeat_or_skip_cash_receipts",
        "workers::worker_transaction::tests::ledger::checkpoints::failed_suffix_replay_remains_rejected_until_the_ledger_is_repaired",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::appended_cash_uses_one_balance_lookup_and_one_schema_guard",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::recreating_an_identical_monitor_cannot_hide_unprocessed_cash",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::restored_edit_monitors_cannot_hide_a_missed_update_or_deletion",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::failure_to_save_verified_cash_rolls_back_the_receipt",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::an_overflowed_append_cannot_be_cured_by_a_later_offsetting_receipt",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::malformed_appended_receipts_invalidate_saved_cash_until_repaired",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::other_trigger_effects_are_included_before_cash_can_be_reused",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::an_ignored_projection_completion_cannot_authorize_old_cash",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::a_commit_between_metadata_and_cash_lookup_cannot_reuse_old_money",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::restoring_a_backup_discards_cash_from_the_replaced_ledger",
        "workers::worker_transaction::tests::ledger::checkpoints::write_through::abrupt_exit_discards_uncommitted_cash_and_reverifies_on_restart",
        "cash_projection::a_projection_write_failure_rolls_back_funding_and_retry_reserves_once",
        "cash_projection::a_poisoned_cash_append_blocks_submission_without_broker_effects",
        "every_external_position_order_and_shared_account_reservation_consumes_capacity",
        "unrealized_daily_loss_latches_and_later_profit_does_not_resume",
        "owner_pnl_counter_reconciliation_preserves_losses_and_cannot_reset_high_water",
        "mandate_capacity_marks_existing_exposure_and_respects_remaining_slots",
        "safety_state_history_is_atomic_private_and_rejects_deletion_or_rollback",
        "stops_preserve_locked_profit_even_with_stale_client_snapshots",
        "broker_snapshot_cannot_erase_previous_stop_protection",
        "owner_tokens_require_original_recent_authentication_and_exact_mfa_context",
        "agents_cannot_gain_owner_authority_from_claims_scopes_or_tool_arguments",
        "owner_authentication_is_rechecked_before_dispatch_and_idempotent_replay",
        "submission::submission_reuses_only_its_own_reservation_and_preserves_monthly_boundary",
        "submission::submission_fails_closed_when_reservation_or_rule_reads_are_corrupt",
        "submission::realized_loss_after_funding_cannot_be_hidden_by_its_reservation",
        "generated_funding_gate_matches_risk_formula",
        "generated_lifecycles_conserve_cash_with_fees_and_adverse_exits",
        "a_reservation_cannot_be_funded_again_after_funding_fill_cancel_or_close",
        "transfers_move_exact_cash_between_related_accounts_and_survive_restart",
        "rejected_transfers_leave_cash_and_receipts_unchanged",
        "failed_transfers_roll_back_both_accounts_and_can_be_retried_once",
        "distributions_conserve_cash_and_record_the_actual_allocations",
        "non_positive_cash_adjustments_and_distributions_never_write_receipts",
        "distribution_threshold_and_available_cash_are_enforced_without_side_effects",
        "failed_distributions_roll_back_every_leg_and_history_before_retry",
        "arbitrary_transfer_sequences_cannot_create_money_or_overdraw",
        "broker_reconciliation_authority_cannot_record_manual_executions",
        "service_requires_explicit_trade_and_monthly_policy"},
    2: {"statement_mcp_matches_actual_fills_fees_cash_and_replays_after_encrypted_restart",
        "unresolved_statement_halts_all_profiles_until_explicit_resolution_and_verified_resume",
        "integrity::statement_failure_rolls_back_artifact_result_audit_and_halt_as_one_unit",
        "access::statement_tools_require_owner_mfa_and_authorization_to_every_linked_profile_even_on_replay",
        "history::history_pages_match_over_mcp_http_and_restart_and_account_units_are_explicit",
        "reports_reconcile_realized_fees_open_exposure_and_account_scope_after_restart",
        "reports_reject_missing_accounts_and_mixed_currencies_instead_of_plausible_totals",
        "non_success_http_status_cannot_be_reported_as_success_by_a_null_error_envelope",
        "lost_mutation_response_sends_once_and_requires_reconciliation_with_the_original_key",
        "operator_environment_selects_service_and_token_without_exposing_them",
        "discovery_resolves_exact_instruments_and_previews_without_committing_cash_or_orders",
        "invalid_operator_configuration_stops_before_readiness_or_financial_storage",
        "connected_events_remove_credentials_from_wire_and_debug_output",
        "serialized_session_workflow_replaces_duplicates_removes_only_the_requested_trade_and_shuts_down",
        "portfolio_heat_is_atomic_across_agents_and_profiles_and_preview_never_reserves",
        "admission_rechecks_quotes_margin_and_calendar_before_broker_submission",
        "observed_agent_losses_and_fees_block_reentry_after_restart_without_policy_reset",
        "risk_market_data_uses_exact_contract_and_requires_timestamped_realtime_evidence",
        "order_cache_refresh_never_authorizes_duplicate_submission",
        "whole_account_reservations_and_halt_survive_restart_and_preserve_cancellation",
        "external_positions_unrealized_losses_and_snapshot_failures_block_new_exposure",
        "failed_advisory_and_market_reads_do_not_create_unknown_trading_outcomes",
        "agent_trades_within_owner_policy_and_can_only_tighten_stops",
        "incomplete_or_truncated_order_snapshots_never_submit",
        "real_process_mcp_ibkr_lifecycles_replay_audit_metrics_and_redaction",
        "mcp_submission_observes_policy_changes_after_funding",
        "crash_after_broker_acceptance_recovers_unknown_without_resubmission_and_detects_tamper"},
    3: {"live_gateway_preflight_authenticates_and_selects_account",
        "live_gateway_resolves_stock_etf_and_bond_contracts",
        "live_gateway_previews_stock_etf_and_bond_brackets_with_whatif"},
}


def fingerprint(root=ROOT):
    """Hash code/configuration, including dirty/untracked source, without reading secrets."""
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
    ).decode().split("\0")
    crates = {"model", "core", "db-sqlite", "service", "cli", "advisor", "credentials", "security",
              "ibkr-broker", "alpaca-broker", "broker-sync"}
    exact = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "makefile", "Dockerfile", ".dockerignore", ".cargo/config.toml", ".gitignore", "deploy/config.example.json", "deploy/compose.yaml", "docs/security.md"}
    digest = hashlib.sha256()
    for name in sorted(set(names)):
        parts = Path(name).parts
        if not parts:
            continue
        source = parts[0] in crates and (name.endswith("/Cargo.toml") or
            (len(parts) > 1 and parts[1] in {"src", "tests", "migrations"}))
        support = name.startswith(("scripts/", "skills/", "docs/reliability/", ".github/workflows/", "deploy/keycloak/", "deploy/postgres/"))
        if not (name in exact or source or support):
            continue
        path = root / name
        if path.is_symlink():
            raise ValueError("Source symlinks cannot be used as assurance evidence")
        digest.update(name.encode() + b"\0")
        digest.update(path.read_bytes() if path.is_file() else b"DELETED")
        digest.update(b"\0")
    return digest.hexdigest()


def evaluate(log, returncodes, required):
    tests = re.findall(r"^test (.+?) \.\.\. (ok|FAILED|ignored[^\n]*)$", log, re.M)
    summaries = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", log)
    count = sum(sum(map(int, row)) for row in summaries)
    passed = {name for name, state in tests if state == "ok"}
    missing = sorted(required - passed)
    complete = bool(summaries) and len(tests) == count
    good = bool(returncodes) and all(code == 0 for code in returncodes)
    return {"status": "passed" if good and complete and not missing and
            all(state != "FAILED" for _, state in tests) else "failed",
            "complete_output": complete, "missing_required": missing,
            "tests": [{"name": name, "result": state} for name, state in tests],
            "returncodes": returncodes}


def current_report(level, digest, directory=EVIDENCE):
    path = directory / f"level-{level}.json"
    if not path.exists():
        return {"status": "not_run"}
    try:
        report = json.loads(path.read_text())
        log = (directory / f"level-{level}.log").read_bytes()
        date = dt.datetime.fromisoformat(report["generated_at_utc"])
        age = (dt.datetime.now(dt.timezone.utc) - date).total_seconds()
        if report["source_fingerprint"] != digest or not 0 <= age <= 7 * 86400:
            return {"status": "stale"}
        if hashlib.sha256(log).hexdigest() != report["log_sha256"]:
            return {"status": "invalid_evidence"}
        checked = evaluate(log.decode(), report["returncodes"], REQUIRED[level])
        if checked["status"] != report["status"] or checked["status"] != "passed":
            return {"status": "failed"}
        return {"status": "passed", "generated_at_utc": report["generated_at_utc"]}
    except (ValueError, KeyError, OSError, TypeError):
        return {"status": "invalid_evidence"}


def commands(level):
    base = [CARGO, "test", "--locked"]
    if level == 1:
        return [base + ["-p", "core", "-p", "trust-model", "-p", "db-sqlite"] + TAIL,
                base + ["-p", "trust-service", "--test", "service_contract"] + TAIL]
    if level == 2:
        return [base + ["-p", "ibkr-broker", "-p", "broker-sync"] + TAIL,
                base + ["-p", "trust-service", "--test", "e2e", "--test", "product_reports", "--test", "product_discovery", "--test", "statement_workflow"] + TAIL,
                base + ["-p", "trust-cli", "--test", "service_client"] + TAIL]
    return [base + ["-p", "ibkr-broker", "--test", "live_gateway_smoke_test", name,
                    "--", "--ignored", "--exact", "--test-threads=1"] for name in sorted(REQUIRED[3])]


def record_workspace_run(log_path, exit_code, digest, command):
    """Record both offline scopes from one actual, complete workspace run."""
    raw = log_path.read_bytes()
    if digest != fingerprint():
        return
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    for level in (1, 2):
        report = evaluate(raw.decode(), [exit_code], REQUIRED[level])
        report.update(schema_version=1, level=level, source_fingerprint=digest,
                      log_sha256=hashlib.sha256(raw).hexdigest(), commands=[command],
                      generated_at_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
                      broker_contacted=False, orders_placed=False,
                      proptest_rng_seed=os.environ.get("PROPTEST_RNG_SEED"))
        path = EVIDENCE / f"level-{level}.log"
        path.write_bytes(raw)
        os.chmod(path, 0o600)
        path = path.with_suffix(".json")
        path.write_text(json.dumps(report, indent=2) + "\n")
        os.chmod(path, 0o600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["status", "run"], nargs="?", default="status")
    parser.add_argument("--level", type=int, choices=range(1, 7))
    parser.add_argument("--connect-paper", action="store_true", help="Explicitly contact the configured authenticated paper gateway; never places orders")
    args = parser.parse_args()
    manifest = json.loads(MANIFEST.read_text())
    digest = fingerprint()
    if args.action == "status":
        levels = [{**level, **(current_report(level["level"], digest) if level["level"] <= 3
                    else {"status": "not_verified", "reason": "Requires supervised broker and operational acceptance"})}
                  for level in manifest["levels"]]
        print(json.dumps({"source_fingerprint": digest, "levels": levels,
                          "live_ready": False, "live_blockers": manifest["live_blockers"]}, indent=2))
        return 0
    if args.level is None:
        parser.error("run requires --level")
    if args.level > 3:
        parser.error("Levels 4-6 require supervised acceptance; a smoke test cannot promote them. See docs/reliability/verification.md and scripts/ibkr_paper_acceptance.py.")
    for prior in range(1, args.level):
        if current_report(prior, digest)["status"] != "passed":
            parser.error(f"Level {prior} must pass for the current source before level {args.level}")
    env = os.environ.copy()
    env.setdefault("PROPTEST_RNG_SEED", "20261027")
    env["TRUST_IBKR_LIVE_E2E"] = "0"
    env.pop("TRUST_IBKR_ALLOW_PAPER_MUTATIONS", None)
    if args.level == 3:
        names = ["TRUST_IBKR_ACCOUNT_ID", "TRUST_IBKR_URL"] + [
            f"TRUST_IBKR_{asset}_{field}" for asset in ["STOCK", "ETF", "BOND"]
            for field in ["SYMBOL", "ENTRY_PRICE", "STOP_PRICE", "TARGET_PRICE"]]
        if not args.connect_paper or any(not env.get(name) for name in names):
            parser.error("Level 3 requires --connect-paper and the gateway/account/symbol/price configuration documented in docs/interactive-brokers-e2e.md")
        if not env["TRUST_IBKR_ACCOUNT_ID"].startswith("DU"):
            parser.error("Paper preflight requires a DU account and broker-confirmed isPaper")
        env["TRUST_IBKR_LIVE_E2E"] = "1"
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    log_path = EVIDENCE / f"level-{args.level}.log"
    codes = []
    selected = commands(args.level)
    with log_path.open("w") as log:
        os.chmod(log_path, 0o600)
        for command in selected:
            print("Running " + " ".join(command), flush=True)
            code = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=False).returncode
            codes.append(code)
            if code:
                break
    raw = log_path.read_bytes()
    report = evaluate(raw.decode(), codes, REQUIRED[args.level])
    if digest != fingerprint():
        report["status"] = "stale"
    report.update(schema_version=1, level=args.level, source_fingerprint=digest,
                  log_sha256=hashlib.sha256(raw).hexdigest(), commands=selected,
                  generated_at_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
                  broker_contacted=args.level == 3, orders_placed=False,
                  proptest_rng_seed=env["PROPTEST_RNG_SEED"])
    path = log_path.with_suffix(".json")
    path.write_text(json.dumps(report, indent=2) + "\n")
    os.chmod(path, 0o600)
    print(json.dumps({"level": args.level, "status": report["status"], "evidence": str(path), "missing_required": report["missing_required"]}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
