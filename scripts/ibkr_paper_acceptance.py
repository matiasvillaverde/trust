#!/usr/bin/env python3
"""Supervised, one-unit IBKR paper submit/cancel smoke through authenticated MCP.

Uses an operator-prepared trade and existing policies. Never deposits ledger cash,
creates a trade, changes policy, confirms broker warnings, or retries a mutation.
This smoke is evidence toward level 4, not complete level-4 acceptance.
"""
import argparse
import datetime as dt
from decimal import Decimal, InvalidOperation
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid

from assurance import EVIDENCE, current_report, fingerprint

ACK = "I_UNDERSTAND_PAPER_ORDERS_ARE_REAL_SIMULATIONS"


class AcceptanceError(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise AcceptanceError("Redirect refused; broker acceptance stopped")


class Mcp:
    def __init__(self, origin, token):
        url = urllib.parse.urlsplit(origin)
        if (url.username or url.password or url.query or url.fragment or url.path not in ("", "/")
                or not url.hostname or (url.scheme != "https" and not
                    (url.scheme == "http" and url.hostname in {"localhost", "127.0.0.1", "::1"}))):
            raise AcceptanceError("Use an HTTPS service origin or a loopback HTTP origin without credentials")
        if not token or any(c.isspace() for c in token):
            raise AcceptanceError("Invalid bearer token file")
        self.url = origin.rstrip("/") + "/mcp"
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect())

    def call(self, name, arguments):
        body = {"jsonrpc": "2.0", "id": str(uuid.uuid4()), "method": "tools/call",
                "params": {"name": name, "arguments": arguments}}
        request = urllib.request.Request(self.url, data=json.dumps(body).encode(), method="POST",
            headers={"Authorization": "Bearer " + self.token, "Content-Type": "application/json",
                     "Accept": "application/json, text/event-stream"})
        try:
            with self.opener.open(request, timeout=50) as response:
                raw = response.read(1024 * 1024 + 1)
            if len(raw) > 1024 * 1024:
                raise AcceptanceError("MCP response exceeded the evidence budget")
            return json.loads(raw)["result"]["structuredContent"]
        except (urllib.error.URLError, KeyError, ValueError, TimeoutError, OSError) as error:
            # Neither URLs, raw responses, nor bearer tokens belong in diagnostics.
            raise AcceptanceError("No confirmed MCP response; inspect the receipt and original operation key") from error


def decimal(value):
    try:
        result = Decimal(str(value))
    except InvalidOperation as error:
        raise AcceptanceError("Invalid financial evidence") from error
    if not result.is_finite():
        raise AcceptanceError("Non-finite financial evidence")
    return result


def validate_trade(account, trade, account_id, broker_account, max_notional):
    if (str(account.get("id")) != account_id or account.get("environment") != "Paper"
            or str(account.get("broker_kind")).lower() != "ibkr"
            or account.get("broker_account_id") != broker_account or not broker_account.startswith("DU")):
        raise AcceptanceError("Exact paper account identity is required")
    if (str(trade.get("account_id")) != account_id or trade.get("status") != "New"
            or trade.get("category") != "Long"
            or trade.get("trading_vehicle", {}).get("category") not in {"Stock", "Etf"}):
        raise AcceptanceError("Use a fresh operator-prepared long stock/ETF trade in this paper account")
    for role in ("entry", "safety_stop", "target"):
        if decimal(trade[role]["quantity"]) != 1:
            raise AcceptanceError("Paper smoke permits exactly one unit in each bracket leg")
    entry = decimal(trade["entry"]["unit_price"])
    if max_notional <= 0 or entry <= 0 or entry > max_notional:
        raise AcceptanceError("Prepared order exceeds the explicitly authorized notional")


def run(client, args, receipt, persist):
    def call(name, arguments, mutation=False):
        if mutation:
            arguments = {**arguments, "idempotency_key": f"paper-{args.run_id}-{name}"}
        entry = {"tool": name, "idempotency_key": arguments.get("idempotency_key"), "state": "started"}
        receipt["steps"].append(entry)
        persist()
        reply = client.call(name, arguments)
        entry.update(request_id=reply.get("request_id"), operation_id=reply.get("operation_id"))
        if reply.get("error"):
            entry["state"] = "unconfirmed" if mutation else "rejected"
            persist()
            raise AcceptanceError("Service did not confirm the step; inspect its original operation before taking another action")
        entry["state"] = "confirmed"
        persist()
        return reply["data"]

    ids = {"account_id": args.account_id, "trade_id": args.trade_id}
    account = call("account_get", {"account_id": args.account_id})["account"]
    trade = call("trade_get", ids)
    validate_trade(account, trade, args.account_id, args.broker_account_id, args.max_notional)
    call("broker_preview", ids)
    call("trade_fund", ids, True)
    call("trade_submit", ids, True)
    call("trade_cancel", ids, True)
    final = call("trade_get", ids)
    if final.get("status") != "Canceled" or any(
            decimal(final[role]["filled_quantity"]) != 0 for role in ("entry", "target", "safety_stop")):
        raise AcceptanceError("Paper trade is not confirmed canceled without fills; operator reconciliation is required")
    call("trade_audit", ids)
    receipt["status"] = "smoke_passed"
    receipt["level_4_complete"] = False
    persist()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("service-url", "token-file", "account-id", "broker-account-id", "trade-id", "run-id", "max-notional"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--acknowledge-paper-orders", required=True)
    args = parser.parse_args()
    if args.acknowledge_paper_orders != ACK:
        parser.error("Explicit paper-order acknowledgment is required")
    try:
        args.account_id = str(uuid.UUID(args.account_id))
        args.trade_id = str(uuid.UUID(args.trade_id))
        args.run_id = str(uuid.UUID(args.run_id))
        args.max_notional = decimal(args.max_notional)
        digest = fingerprint()
        if any(current_report(level, digest)["status"] != "passed" for level in (1, 2, 3)):
            raise AcceptanceError("Current levels 1-3 must pass before a supervised paper order")
        client = Mcp(args.service_url, Path(args.token_file).read_text().strip())
        EVIDENCE.mkdir(parents=True, exist_ok=True)
        path = EVIDENCE / f"paper-{args.run_id}.json"
        # A durable receipt is created before I/O. Re-running this script with the
        # same run ID must inspect the original evidence, not create a new intent.
        descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        receipt = {"schema_version": 1, "status": "started", "source_fingerprint": digest,
                   "run_id": args.run_id, "account_id": args.account_id, "trade_id": args.trade_id,
                   "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(), "steps": [],
                   "scope": "one-unit paper submit/cancel smoke", "level_4_complete": False}
        with os.fdopen(descriptor, "w") as handle:
            def persist():
                handle.seek(0)
                json.dump(receipt, handle, indent=2)
                handle.truncate()
                handle.flush()
                os.fsync(handle.fileno())
            persist()
            try:
                run(client, args, receipt, persist)
            except (AcceptanceError, KeyError, TypeError, ValueError, KeyboardInterrupt):
                receipt["status"] = "operator_review_required"
                persist()
                raise AcceptanceError("Acceptance stopped. Inspect the receipt and original operation keys; do not rerun with a new intent")
        print(json.dumps({"status": receipt["status"], "receipt": str(path), "level_4_complete": False}))
        return 0
    except (AcceptanceError, OSError, ValueError) as error:
        print(str(error) if isinstance(error, AcceptanceError) else "Invalid setup or existing receipt; no automatic retry", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
