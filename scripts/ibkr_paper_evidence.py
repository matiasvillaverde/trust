#!/usr/bin/env python3
"""Read-only paper evidence capture and independent, exact-decimal reconciliation.

Never submits, cancels, reconciles or changes policy. See docs/ibkr-paper-evidence.md.
"""
import argparse
import datetime as dt
from decimal import Decimal, localcontext
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import uuid

from assurance import fingerprint
from ibkr_paper_acceptance import AcceptanceError, Mcp

ROLES = ("entry", "safety_stop", "target")
STAGES = ("before", "protected", "reconnected", "closed")


def require(condition, message):
    if not condition:
        raise AcceptanceError(message)


def money(value):
    require(isinstance(value, str) and len(value) <= 64 and re.fullmatch(r"-?[0-9]+(?:\.[0-9]+)?", value),
            "Financial evidence must be finite plain decimal strings")
    return Decimal(value)


def timestamp(value):
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(parsed.tzinfo is not None, "Evidence timestamps need an explicit timezone")
    return parsed


def write_new(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with os.fdopen(os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600), "w") as handle:
        json.dump(value, handle, indent=2)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())


def capture(client, account, trade, stage):
    """Pagination must complete; every result retains its request/timestamp provenance."""
    calls = []

    def call(tool, args):
        reply = client.call(tool, args)
        require(not reply.get("error"), "Capture was rejected; refresh/reconcile separately before collecting evidence")
        calls.append({"tool": tool, "request_id": reply["request_id"], "generated_at": reply["generated_at"]})
        return reply["data"]

    def pages(tool, args):
        items, offset, total = [], 0, None
        for _ in range(100):
            page = call(tool, {**args, "offset": offset, "limit": 200})
            if total is None:
                total = page["total"]
            require(total == page["total"], "Evidence changed during pagination; take a new read-only capture")
            items.extend(page["items"])
            next_offset = page["next_offset"]
            if next_offset is None:
                require(len(items) == total, "Incomplete paginated evidence")
                return items
            require(isinstance(next_offset, int) and next_offset > offset, "Invalid evidence cursor")
            offset = next_offset
        raise AcceptanceError("Evidence page budget exceeded")

    ids = {"account_id": account, "trade_id": trade}
    account_record = call("account_get", {"account_id": account})["account"]
    require(account_record["environment"] == "Paper" and account_record["broker_kind"].lower() == "ibkr"
            and account_record["broker_account_id"].startswith("DU"), "Only an explicitly identified IBKR paper account is accepted")
    record = call("trade_get", ids)
    require(record["account_id"] == account and record["id"] == trade, "Trade identity mismatch")
    readiness = call("account_readiness", {"account_id": account})
    require(readiness["snapshot"] is not None, "Capture needs a human owner session authorized for all linked accounts")
    return {"schema_version": 1, "stage": stage, "source_fingerprint": fingerprint(),
            "captured_at": dt.datetime.now(dt.timezone.utc).isoformat(), "account": account_record,
            "trade": record, "readiness": readiness, "executions": pages("executions_list", ids),
            "transactions": pages("transactions_list", {"account_id": account}),
            "trade_audit": call("trade_audit", ids), "calls": calls}


def execution_map(rows, account_id, trade_id, symbol):
    result = {}
    for row in rows:
        key = row["broker_execution_id"]
        require(isinstance(key, str) and key and key not in result, "Missing or duplicate execution identity")
        require(row["account_id"] == account_id and row["trade_id"] == trade_id and row["symbol"] == symbol,
                "Execution belongs to another account, trade or instrument")
        require(row["side"] in {"Buy", "Sell"} and money(row["qty"]) > 0 and money(row["price"]) > 0,
                "Unsupported or invalid execution")
        result[key] = (row["broker_order_id"], row["side"], money(row["qty"]), money(row["price"]))
    return result


def verify(captures, statement, source_fingerprint):
    """Scope: isolated single-unit long stock/ETF; no FX, transfers or unrelated activity."""
    require(set(captures) == set(STAGES), "All four lifecycle captures are required")
    first, final = captures["before"], captures["closed"]
    account, trade = first["account"], first["trade"]
    aid, tid, native, currency = account["id"], trade["id"], account["broker_account_id"], trade["currency"]
    symbol = trade["trading_vehicle"]["symbol"]
    require(account["environment"] == "Paper" and account["broker_kind"].lower() == "ibkr" and native.startswith("DU"), "Paper identity required")
    require(trade["category"] == "Long" and trade["trading_vehicle"]["category"] in {"Stock", "Etf"}, "Unsupported acceptance instrument or direction")
    require(trade["status"] == "New" and all(money(trade[r]["quantity"]) == 1 and money(trade[r]["filled_quantity"]) == 0 for r in ROLES), "A fresh one-unit bracket is required")
    previous = None
    for stage in STAGES:
        capture_ = captures[stage]
        current = timestamp(capture_["captured_at"])
        require(previous is None or current > previous, "Lifecycle observations must be strictly ordered")
        previous = current
        require(capture_["stage"] == stage and capture_["source_fingerprint"] == source_fingerprint, "Stale source or wrong capture stage")
        require(capture_["account"]["id"] == aid and capture_["account"]["broker_account_id"] == native
                and capture_["account"]["environment"] == "Paper" and capture_["account"]["broker_kind"].lower() == "ibkr"
                and capture_["trade"]["id"] == tid and capture_["trade"]["account_id"] == aid
                and capture_["trade"]["currency"] == currency, "Cross-account or currency evidence")
        snapshot = capture_["readiness"]["snapshot"]
        require(snapshot["scope"] == {"broker": "ibkr", "environment": "paper", "account_id": native}
                and snapshot["currency"] == currency and snapshot["complete"] is True and snapshot["consistent"] is True,
                "Incomplete or foreign broker evidence")
        require(0 <= (current - timestamp(snapshot["observed_at"])).total_seconds() <= 120, "Stale broker evidence")
        require(capture_["readiness"]["unresolved_operation_count"] == 0, "Unresolved operations need owner investigation")
    require(not first["readiness"]["snapshot"]["positions"] and not first["readiness"]["snapshot"]["working_orders"], "Use an isolated, initially flat paper account")
    ending = final["trade"]
    require(ending["status"] in {"ClosedTarget", "ClosedStopLoss"}, "No confirmed completed target/stop lifecycle")
    exit_role = "target" if ending["status"] == "ClosedTarget" else "safety_stop"
    sibling = "safety_stop" if exit_role == "target" else "target"
    require(money(ending["entry"]["filled_quantity"]) == money(ending[exit_role]["filled_quantity"]) == 1
            and money(ending[sibling]["filled_quantity"]) == 0 and ending[sibling]["status"] in {"Canceled", "Expired"}, "Entry, exit and sibling termination are not confirmed")
    require(not final["readiness"]["snapshot"]["positions"] and not final["readiness"]["snapshot"]["working_orders"], "Residual broker exposure or working orders remain")
    ids = {role: ending[role]["broker_order_id"] for role in ROLES}
    require(all(isinstance(value, str) and value for value in ids.values()) and len(set(ids.values())) == 3, "Bracket order identities are incomplete")
    for stage in ("protected", "reconnected"):
        observation = captures[stage]
        require(observation["trade"]["status"] == "Filled", "Protection/reconnect observations require a filled position")
        require(all(observation["trade"][role]["broker_order_id"] == ids[role] for role in ROLES), "Order identity changed across reconnect")
        orders = {row["order_id"]: row for row in observation["readiness"]["snapshot"]["working_orders"]}
        for role in ("safety_stop", "target"):
            order = orders.get(ids[role], {})
            require(order.get("side") == "sell" and money(order.get("remaining_quantity")) == 1, "Protective child missing or wrong quantity/side")
    require(statement["schema_version"] == 1 and statement["complete"] is True
            and statement["broker_account_id"] == native and statement["currency"] == currency,
            "Independent statement identity, completeness or currency mismatch")
    require(timestamp(statement["period_start"]) <= timestamp(first["captured_at"])
            and timestamp(statement["period_end"]) >= timestamp(final["captured_at"]), "Statement does not cover the full run")
    require(statement["opening_positions"] == [] and statement["closing_positions"] == [] and statement["closing_working_orders"] == []
            and statement["external_cash_flows"] == [], "Unexplained positions, orders or cash flows in isolated run")
    expected = execution_map(statement["executions"], aid, tid, symbol)
    actual = execution_map(final["executions"], aid, tid, symbol)
    require(actual and actual == expected, "Trust executions differ from independent broker evidence")
    require({row[0] for row in expected.values()} == {ids["entry"], ids[exit_role]}, "Unexpected execution order")
    for role, side in (("entry", "Buy"), (exit_role, "Sell")):
        fills = [row for row in actual.values() if row[0] == ids[role]]
        require(all(row[1] == side for row in fills) and sum(row[2] for row in fills) == 1, "Execution side or total quantity mismatch")
        price = sum(row[2] * row[3] for row in fills)
        require(abs(price - money(ending[role]["average_filled_price"])) <= Decimal("0.000001"), "Ledger execution price mismatch")
    fee_ids, fees = set(), Decimal(0)
    for row in statement["fees"]:
        require(row["id"] and row["id"] not in fee_ids and row["currency"] == currency
                and row["broker_order_id"] in {ids["entry"], ids[exit_role]} and money(row["amount"]) >= 0, "Invalid or duplicate fee evidence")
        fee_ids.add(row["id"])
        fees += money(row["amount"])
    require(statement["fees_final"] is True, "Fees are not final; repeat read-only capture after settlement")
    ledger_fees, tx_ids = Decimal(0), set()
    for row in final["transactions"]:
        require(row["id"] not in tx_ids and row["account_id"] == aid and row["currency"] == currency, "Duplicate or foreign ledger transaction")
        tx_ids.add(row["id"])
        if isinstance(row["category"], dict) and any(row["category"].get(key) == tid for key in ("FeeOpen", "FeeClose")):
            require(money(row["amount"]) >= 0, "Invalid ledger fee")
            ledger_fees += money(row["amount"])
    require(ledger_fees == fees, "Trust fees differ from independent fees")
    opening, closing = money(statement["opening_cash"]), money(statement["closing_cash"])
    cash_delta = sum(qty * price * (1 if side == "Sell" else -1) for _, side, qty, price in expected.values()) - fees
    require(abs(opening + cash_delta - closing) <= Decimal("0.01"), "Independent cash conservation failed")
    require(abs(money(first["readiness"]["snapshot"]["cash"]) - opening) <= Decimal("0.01")
            and abs(money(final["readiness"]["snapshot"]["cash"]) - closing) <= Decimal("0.01"), "Broker cash differs from independent statement")
    return {"status": "reconciled", "scope": "one-unit long paper target/stop lifecycle", "exit": exit_role,
            "execution_count": len(actual), "fees": str(fees), "cash_delta": str(cash_delta),
            "source_fingerprint": source_fingerprint, "level_4_complete": False,
            "operator_review_required": ["Independent source authenticity and normalization", "Broker OCA/parent linkage and stop activation", "Actual gateway/service restart and session recovery", "Partial fills and remaining acceptance matrix"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    c = sub.add_parser("capture")
    for name in ("service-url", "token-file", "account-id", "trade-id"):
        c.add_argument("--" + name, required=True)
    c.add_argument("--stage", required=True, choices=STAGES)
    c.add_argument("--output", type=Path, required=True)
    v = sub.add_parser("verify")
    v.add_argument("--manifest", type=Path, required=True)
    v.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "capture":
            result = capture(Mcp(args.service_url, Path(args.token_file).read_text().strip()),
                             str(uuid.UUID(args.account_id)), str(uuid.UUID(args.trade_id)), args.stage)
        else:
            manifest = json.loads(args.manifest.read_text())
            def artifact(item):
                path = args.manifest.parent / item["path"]
                require(path.stat().st_size <= 10 * 1024 * 1024, "Evidence artifact exceeds budget")
                raw = path.read_bytes()
                require(hashlib.sha256(raw).hexdigest() == item["sha256"], "Evidence artifact hash mismatch")
                return raw
            require(manifest["broker_sources"], "Independent original broker artifacts are required")
            for item in manifest["broker_sources"]:
                artifact(item)
            captures = {stage: json.loads(artifact(manifest[stage])) for stage in STAGES}
            with localcontext() as context:
                context.prec = 80
                result = verify(captures, json.loads(artifact(manifest["statement"])), fingerprint())
        write_new(args.output, result)
        print(json.dumps({"status": "captured" if args.command == "capture" else result["status"], "output": str(args.output), "level_4_complete": False}))
        return 0
    except (AcceptanceError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error) if isinstance(error, AcceptanceError) else "Invalid or incomplete evidence; no broker action was attempted", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
