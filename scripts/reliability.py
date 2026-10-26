#!/usr/bin/env python3
"""Run the offline reliability gate and retain named-test evidence (no live orders)."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COMMAND = ["cargo", "test", "--locked", "--workspace", "--all-features", "--no-fail-fast", "--", "--test-threads=1"]
REQUIRED = {
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
    "every_tool_rejects_missing_unknown_wrong_type_and_boundary_arguments",
    "concurrent_duplicates_credit_once_and_distinct_withdrawals_cannot_overspend",
    "journal_intent_failure_stops_worker_before_financial_mutation",
    "journal_completion_failure_keeps_committed_deposit_unknown_without_replay",
    "manual_tools_record_actual_prices_and_conserve_long_short_cash",
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
    coverage = inventory()
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
    report["limits"] = ["Test names and assertions are evidence, not proof of exhaustive state-space coverage.",
                        "Ignored broker tests were not run. No live orders are part of this gate.",
                        "The edge-case register includes external acceptance work; its item count is not a passed-case count."]
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ["success", "totals", "missing_required_regressions"]}))
    print(f"Named-test evidence: {args.output}")
    return 0 if report["success"] else 1


if __name__ == "__main__":
    sys.exit(main())
