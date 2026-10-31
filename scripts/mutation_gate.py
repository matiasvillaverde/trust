#!/usr/bin/env python3
"""Prove selected safety regressions fail when their production guard is removed.

Uses an isolated source copy; never edits the user's checkout or contacts a broker.
Compilation failures are invalid experiments, not killed mutations.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from assurance import ROOT, fingerprint

MUTATIONS = [
    ("submission_policy", "core/src/commands/trade.rs",
     "            crate::validators::funding::can_submit_funded(&current_trade, database)\n                .map_err(|error| error as Box<dyn Error>)?;", "",
     "core", "reliability", "submission_revalidates_changed_trade_and_monthly_caps_without_broker_effects"),
    ("order_snapshot", "ibkr-broker/src/client.rs", '("force", "false".to_string())', '("force", "true".to_string())',
     "ibkr-broker", "http_integration", "submit_trade_recovers_complete_existing_bracket_without_duplicate_post"),
    ("manual_authority", "service/src/catalog.rs", '"trust:manual"', '"trust:reconcile"',
     "trust-service", "service_contract", "broker_reconciliation_authority_cannot_record_manual_executions"),
    ("owner_role", "service/src/auth.rs", '&& (!owner_scope(scope) || self.grant.role == Role::Owner)', '',
     "trust-service", "service_contract", "agents_cannot_gain_owner_authority_from_claims_scopes_or_tool_arguments"),
    ("owner_freshness", "service/src/auth.rs", '        if owner_scope(scope) {\n            self.check_owner_authentication(now)?;\n        }', '',
     "trust-service", "service_contract", "owner_authentication_is_rechecked_before_dispatch_and_idempotent_replay"),
    ("protective_stop", "core/src/validators/trade.rs",
     'if trade.category == TradeCategory::Long && trade.safety_stop.unit_price > new_price_stop\n        || trade.category == TradeCategory::Short && trade.safety_stop.unit_price < new_price_stop', 'if false',
     "core", "security_regressions", "stops_preserve_locked_profit_even_with_stale_client_snapshots"),
    ("recorded_stop", "core/src/commands/trade.rs", 'protection.safety_stop.unit_price = recorded_stop_bound(&current_trade, database)?;',
     'protection.safety_stop.unit_price = current_trade.safety_stop.unit_price;',
     "core", "security_regressions", "broker_snapshot_cannot_erase_previous_stop_protection"),
    ("transfer_credit", "core/src/services/fund_transfer_service.rs",
     '&TransactionCategory::Deposit,\n                amount,',
     '&TransactionCategory::Deposit,\n                Decimal::ONE,',
     "core", "capital_contracts", "transfers_move_exact_cash_between_related_accounts_and_survive_restart"),
    ("transfer_rollback", "core/src/commands/atomic.rs",
     'let rollback_error = database.rollback_to_savepoint(name).err();',
     'let rollback_error: Option<Box<dyn Error>> = None;',
     "core", "capital_contracts", "failed_transfers_roll_back_both_accounts_and_can_be_retried_once"),
    ("distribution_threshold", "model/src/distribution.rs",
     'if profit < self.minimum_threshold {', 'if false {',
     "core", "capital_contracts", "distribution_threshold_and_available_cash_are_enforced_without_side_effects"),
    ("distribution_receipt", "core/src/services/profit_distribution_service.rs",
     'tax_amount: result.tax_amount,', 'tax_amount: result.earnings_amount,',
     "core", "capital_contracts", "distributions_conserve_cash_and_record_the_actual_allocations"),
    ("funding_replay", "core/src/validators/trade.rs",
     'Status::New => Ok(()),', 'Status::New | Status::Funded => Ok(()),',
     "core", "reliability", "a_reservation_cannot_be_funded_again_after_funding_fill_cancel_or_close"),
]


def main():
    digest = fingerprint()
    output = ROOT / ".local/assurance/mutations"
    output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    # Cargo releases its build lock before tests/rustdoc finish. Sharing the main
    # target could replace dependency artifacts underneath a concurrent test run.
    env.update(CARGO_TARGET_DIR=str(ROOT / "target/assurance-mutations"), CARGO_INCREMENTAL="0", TRUST_IBKR_LIVE_E2E="0")
    # The isolated copy contains source only. Its relative .cargo/config.toml
    # cannot locate the native library prepared in the original checkout.
    env.setdefault("SQLCIPHER_LIB_DIR", str(ROOT / ".local/sqlcipher/lib"))
    env.setdefault("SQLCIPHER_INCLUDE_DIR", str(ROOT / ".local/sqlcipher/include"))
    env.pop("TRUST_IBKR_ALLOW_PAPER_MUTATIONS", None)
    results = []
    with tempfile.TemporaryDirectory(prefix="trust-mutations-") as directory:
        checkout = Path(directory)
        names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT).decode().split("\0")
        for name in set(names):
            path = ROOT / name
            if not name or not path.is_file() or path.is_symlink():
                continue
            # Only build/test source; no ignored databases, tokens or local config.
            embedded_skill_metadata = name.startswith("skills/") and Path(name).suffix == ".yaml"
            if Path(name).suffix not in {".rs", ".toml", ".lock", ".json", ".md", ".sql", ".snap"} and not embedded_skill_metadata:
                continue
            if name.startswith((".local/", ".env", "deploy/")):
                continue
            target = checkout / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        for name, relative, old, new, package, suite, test in MUTATIONS:
            path = checkout / relative
            original = path.read_text()
            if old not in original:
                raise ValueError(f"Mutation {name} no longer matches its guard; update the experiment")
            command = [os.environ.get("CARGO", "cargo"), "test", "--locked", "--offline", "-p", package,
                       "--test", suite, test, "--", "--exact", "--test-threads=1"]
            outcomes = {}
            try:
                for variant in ("baseline", "mutant"):
                    path.write_text(original if variant == "baseline" else original.replace(old, new))
                    log_path = output / f"{name}-{variant}.log"
                    print(f"Testing {name}: {variant}", flush=True)
                    with log_path.open("w") as log:
                        code = subprocess.run(command, cwd=checkout, env=env, stdout=log, stderr=subprocess.STDOUT, check=False).returncode
                    raw = log_path.read_bytes()
                    expected = f"test {test} ... " + ("ok" if variant == "baseline" else "FAILED")
                    outcomes[variant] = {"exit_code": code, "assertion_observed": expected in raw.decode(),
                                         "log_sha256": hashlib.sha256(raw).hexdigest()}
                killed = (outcomes["baseline"]["exit_code"] == 0 and outcomes["mutant"]["exit_code"] == 101
                          and all(result["assertion_observed"] for result in outcomes.values()))
                results.append({"mutation": name, "status": "killed" if killed else "invalid_or_survived", **outcomes})
            finally:
                path.write_text(original)
    report = {"source_fingerprint": digest, "source_unchanged": fingerprint() == digest,
              "experiments": results, "scope": f"{len(MUTATIONS)} curated safety guards; not an exhaustive mutation score"}
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return 0 if report["source_unchanged"] and all(result["status"] == "killed" for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
