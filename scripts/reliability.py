#!/usr/bin/env python3
"""Run the offline reliability gate and retain named-test evidence (no live orders)."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import assurance

ROOT = Path(__file__).resolve().parents[1]
COMMAND = ["cargo", "test", "--locked", "--workspace", "--all-features", "--no-fail-fast", "--", "--test-threads=1"]
REQUIRED = {
    "statement_mcp_matches_actual_fills_fees_cash_and_replays_after_encrypted_restart",
    "unresolved_statement_halts_all_profiles_until_explicit_resolution_and_verified_resume",
    "access::statement_tools_require_owner_mfa_and_authorization_to_every_linked_profile_even_on_replay",
    "access::statement_cash_includes_every_linked_profile_and_excludes_other_broker_accounts",
    "integrity::statement_failure_rolls_back_artifact_result_audit_and_halt_as_one_unit",
    "integrity::statement_source_result_and_deletion_tampering_stop_the_worker",
    "integrity::missing_execution_rows_cannot_hide_recorded_fills_behind_an_apparently_matching_empty_report",
    "integrity::audit_verify_checks_older_attempts_and_restart_refuses_modified_statement_bytes",
    "flex_parser_rejects_entities_external_resources_instructions_and_resource_exhaustion",
    "independent_cash_handles_partial_fills_partial_exits_and_late_fee_booking",
    "short_cash_uses_sale_proceeds_and_buy_to_cover_not_margin_reservations",
    "secret_permissions_links_special_files_and_bounds_fail_closed",
    "private_directories_and_keys_never_overwrite_or_accept_bad_formats",
    "production_accepts_ram_mounted_secrets",
    "native_backend_requires_patched_stable_sqlcipher_four",
    "config::tests::production_requires_independent_ram_keys_and_private_files",
    "config::tests::encrypted_configuration_rejects_data_colocated_keys_and_path_aliases",
    "native_storage_uses_pinned_sqlcipher_and_creates_private_control_files",
    "encrypted_copy_and_rotation_preserve_source_schema_rows_and_control_version",
    "sqlcipher_financial_backup_restores_and_rejects_wrong_keys_plaintext_and_corruption",
    "encrypted_control_copy_preserves_audit_chain_and_reopens_after_rotation",
    "encrypted_process_lifecycle_restart_backup_restore_and_wrong_key_fail_closed",
    "every_external_position_order_and_shared_account_reservation_consumes_capacity",
    "unrealized_daily_loss_latches_and_later_profit_does_not_resume",
    "owner_pnl_counter_reconciliation_preserves_losses_and_cannot_reset_high_water",
    "mandate_capacity_marks_existing_exposure_and_respects_remaining_slots",
    "whole_account_reservations_and_halt_survive_restart_and_preserve_cancellation",
    "external_positions_unrealized_losses_and_snapshot_failures_block_new_exposure",
    "failed_advisory_and_market_reads_do_not_create_unknown_trading_outcomes",
    "safety_state_history_is_atomic_private_and_rejects_deletion_or_rollback",
    "operation_resolution_is_audited_and_cannot_hide_secondary_account_uncertainty",
    "session_input_errors_are_definitive_and_unknown_journaling_does_not_halt_finances",
    "parameterized_skills_validate_identifiers_and_account_authority",
    "validators::trade::tests::stop_changes_never_widen_long_or_short_risk",
    "validators::trade::tests::nonpositive_current_stop_requires_reconciliation",
    "stops_preserve_locked_profit_even_with_stale_client_snapshots",
    "broker_snapshot_cannot_erase_previous_stop_protection",
    "owner_configuration_requires_one_human_and_rejects_privileged_agent_grants",
    "owner_tokens_require_original_recent_authentication_and_exact_mfa_context",
    "agents_cannot_gain_owner_authority_from_claims_scopes_or_tool_arguments",
    "http_and_mcp_challenge_owner_mfa_before_accepting_mutations",
    "owner_authentication_is_rechecked_before_dispatch_and_idempotent_replay",
    "agent_trades_within_owner_policy_and_can_only_tighten_stops",
    "submission::submission_revalidates_changed_trade_and_monthly_caps_without_broker_effects",
    "submission::submission_reuses_only_its_own_reservation_and_preserves_monthly_boundary",
    "submission::submission_fails_closed_when_reservation_or_rule_reads_are_corrupt",
    "submission::realized_loss_after_funding_cannot_be_hidden_by_its_reservation",
    "order_cache_refresh_never_authorizes_duplicate_submission",
    "incomplete_or_truncated_order_snapshots_never_submit",
    "broker_reconciliation_authority_cannot_record_manual_executions",
    "service_requires_explicit_trade_and_monthly_policy",
    "mcp_submission_observes_policy_changes_after_funding",
    "metrics::tests::concurrent_scrapes_have_consistent_histogram_buckets_count_and_sum",
    "metrics::tests::submillisecond_latencies_are_preserved_and_labels_stay_bounded",
    "logging::tests::blocked_log_sink_is_bounded_and_does_not_block_producers",
    "metrics_distinguish_replay_denial_and_domain_execution",
    "monitoring_scope_cannot_read_finances_or_mutate_and_requires_global_grant",
    "reconciliation_keeps_stop_and_target_intents_independent",
    "atomic::tests::worker_savepoints_preserve_outer_atomicity_without_a_diesel_test_transaction",
    "detected_runtime_audit_corruption_stops_financial_admission",
    "incremental_broker_projections_match_full_ledger_for_partial_cancel_exit_fees_and_replay",
    "workers::worker_transaction::tests::cash_movement_projection_matches_ledger_and_rejects_corrupt_amounts",
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
    "real_process_mcp_ibkr_lifecycles_replay_audit_metrics_and_redaction",
    "risk_rejection_is_definitive_and_leaves_cash_and_broker_untouched",
    "crash_after_broker_acceptance_recovers_unknown_without_resubmission_and_detects_tamper",
    "queue_timeout_never_dispatches_expired_intent_and_audit_failure_stops_worker",
    "graceful_shutdown_finishes_active_operation_before_process_exit",
    "failed_recovery_audit_rolls_back_state_and_recovery_is_recorded_once",
    "queue_and_request_deadlines_have_validated_bounds",
    "fractional_risk_rules_round_trip_and_preview_is_fundable",
    "monthly_rule_alone_limits_risk_and_short_collateral_preview",
    "monthly_risk_counts_long_and_short_through_active_states",
    "monthly_budget_does_not_reset_after_current_month_deposit_or_carried_reservation",
    "funding_and_cash_writes_roll_back_at_each_persistence_boundary",
    "manual_execution_and_cancel_roll_back_at_each_persistence_boundary",
    "generated_lifecycles_conserve_cash_with_fees_and_adverse_exits",
    "generated_funding_gate_matches_risk_formula",
    "read_failures_never_look_like_an_empty_portfolio",
    "mixed_currency_ledgers_remain_visible_and_cannot_be_aggregated",
    "fractional_risk_backup_round_trip_and_corruption_rejection",
    "workers::worker_order::tests::order_sqlite_conversion_rejects_corrupt_financial_values",
    "workers::worker_order::tests::invalid_order_writes_leave_persisted_values_unchanged",
    "every_tool_enforces_scope_before_domain_or_broker_access",
    "catalogue_support::every_tool_rejects_missing_unknown_wrong_type_and_boundary_arguments",
    "concurrent_duplicates_credit_once_and_distinct_withdrawals_cannot_overspend",
    "journal_intent_failure_stops_worker_before_financial_mutation",
    "journal_completion_failure_keeps_committed_deposit_unknown_without_replay",
    "manual_tools::manual_tools_record_actual_prices_and_conserve_long_short_cash",
    "real_http_mcp_auth_discovery_tools_resources_and_prompts",
    "uncertain_broker_submission_is_never_executed_twice",
}


def inventory():
    names = set(re.findall(r'spec\(\w+,"([^"]+)"', (ROOT / "service/src/catalog.rs").read_text()))
    documented = set(re.findall(r'^\| `([^`]+)` \| `trust:', (ROOT / "docs/reliability/features.md").read_text(), re.M))
    if not names or names != documented:
        raise ValueError(f"Tool inventory drift: undocumented={sorted(names-documented)}, stale={sorted(documented-names)}")
    cases = re.findall(r'^\d+\. ', (ROOT / "docs/reliability/edge-cases.md").read_text(), re.M)
    return {"tools": sorted(names), "edge_case_register_items": len(cases)}


def summarize(log, exit_code):
    tests = []
    suite = "unknown"
    summaries = []
    for line in log.splitlines():
        running = re.search(r'Running (.+?) \(target/', line)
        if running:
            suite = running[1]
        match = re.fullmatch(r'test (.+?) \.\.\. (ok|FAILED|ignored(?:,.*)?)', line)
        if match:
            tests.append({"suite": suite, "name": match[1].removesuffix(" - should panic"), "result": match[2]})
        match = re.search(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', line)
        if match:
            summaries.append({"result": match[1], "passed": int(match[2]), "failed": int(match[3]), "ignored": int(match[4])})
    passed = {t["name"] for t in tests if t["result"] == "ok"}
    missing = sorted(REQUIRED-passed)
    totals = {key: sum(s[key] for s in summaries) for key in ["passed", "failed", "ignored"]}
    complete = bool(summaries) and len(tests) == sum(totals.values())
    success = exit_code == 0 and complete and not missing and totals["failed"] == 0
    return {"success": success, "command_exit_code": exit_code, "complete_test_output": complete,
            "totals": totals, "suite_summaries": len(summaries), "missing_required_regressions": missing,
            "tests": tests, "ignored": [t for t in tests if t["result"].startswith("ignored")]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summarize", type=Path, help="Summarize a completed all-features workspace test log")
    parser.add_argument("--exit-code", type=int, help="Required with --summarize: actual cargo exit code")
    parser.add_argument("--output", type=Path, default=ROOT / ".local/reliability/results.json")
    parser.add_argument("--coverage", action="store_true", help="Run the same suite with LLVM coverage and repository coverage thresholds")
    args = parser.parse_args()
    if args.summarize and args.exit_code is None:
        parser.error("--summarize requires the actual --exit-code; a partial log is not success")
    os.environ.setdefault("PROPTEST_RNG_SEED", "20261027")
    coverage = inventory()
    source_fingerprint = assurance.fingerprint()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    command = COMMAND
    if args.coverage:
        command = ["cargo", "llvm-cov", "--locked", "--workspace", "--all-features", "--no-fail-fast", "--json",
                   "--output-path", str(args.output.with_name("coverage.json")),
                   "--ignore-filename-regex", r"(^|/)test_support\.rs$",
                   "--fail-under-lines", "94", "--fail-under-functions", "90", "--fail-under-regions", "93",
                   "--", "--test-threads=1"]
    log_path = args.summarize or args.output.with_suffix(".log")
    if args.summarize:
        exit_code = args.exit_code
    else:
        print("Running: " + " ".join(command), flush=True)
        print(f"Full test output: {log_path}", flush=True)
        with log_path.open("w") as handle:
            exit_code = subprocess.run(command, cwd=ROOT, stdout=handle, stderr=subprocess.STDOUT, check=False).returncode
    report = summarize(log_path.read_text(), exit_code)
    report.update(coverage)
    report["generated_at_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    report["command"] = command
    report["log"] = str(log_path.resolve())
    report["cargo_lock_sha256"] = hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest()
    report["source_fingerprint"] = source_fingerprint
    report["proptest_rng_seed"] = os.environ["PROPTEST_RNG_SEED"]
    if not args.summarize and report["success"]:
        assurance.record_workspace_run(log_path, exit_code, source_fingerprint, command)
    report["limits"] = ["Test names and assertions are evidence, not proof of exhaustive state-space coverage.",
                        "Ignored broker tests were not run. No live orders are part of this gate.",
                        "The edge-case register includes external acceptance work; its item count is not a passed-case count."]
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ["success", "totals", "missing_required_regressions"]}))
    print(f"Named-test evidence: {args.output}")
    return 0 if report["success"] else 1


if __name__ == "__main__":
    sys.exit(main())
