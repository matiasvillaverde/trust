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
]


def main():
    digest = fingerprint()
    output = ROOT / ".local/assurance/mutations"
    output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(ROOT / "target"), CARGO_INCREMENTAL="0", TRUST_IBKR_LIVE_E2E="0")
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
            if Path(name).suffix not in {".rs", ".toml", ".lock", ".json", ".md", ".sql", ".snap"}:
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
              "experiments": results, "scope": "three curated safety guards; not an exhaustive mutation score"}
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return 0 if report["source_unchanged"] and all(result["status"] == "killed" for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
