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
    1: {"submission_revalidates_changed_trade_and_monthly_caps_without_broker_effects",
        "stops_preserve_locked_profit_even_with_stale_client_snapshots",
        "broker_snapshot_cannot_erase_previous_stop_protection",
        "owner_tokens_require_original_recent_authentication_and_exact_mfa_context",
        "agents_cannot_gain_owner_authority_from_claims_scopes_or_tool_arguments",
        "owner_authentication_is_rechecked_before_dispatch_and_idempotent_replay",
        "submission_reuses_only_its_own_reservation_and_preserves_monthly_boundary",
        "submission_fails_closed_when_reservation_or_rule_reads_are_corrupt",
        "generated_funding_gate_matches_risk_formula",
        "generated_lifecycles_conserve_cash_with_fees_and_adverse_exits",
        "broker_reconciliation_authority_cannot_record_manual_executions",
        "service_requires_explicit_trade_and_monthly_policy"},
    2: {"order_cache_refresh_never_authorizes_duplicate_submission",
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
    crates = {"model", "core", "db-sqlite", "service", "cli", "advisor", "credentials",
              "ibkr-broker", "alpaca-broker", "broker-sync"}
    exact = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "makefile", "Dockerfile", ".gitignore", "deploy/config.example.json"}
    digest = hashlib.sha256()
    for name in sorted(set(names)):
        parts = Path(name).parts
        if not parts:
            continue
        source = parts[0] in crates and (name.endswith("/Cargo.toml") or
            (len(parts) > 1 and parts[1] in {"src", "tests", "migrations"}))
        support = name.startswith(("scripts/", "skills/", "docs/reliability/", ".github/workflows/"))
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
        return [base + ["-p", "ibkr-broker"] + TAIL,
                base + ["-p", "trust-service", "--test", "e2e"] + TAIL]
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
