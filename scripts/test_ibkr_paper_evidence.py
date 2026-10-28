import copy
import unittest

from ibkr_paper_evidence import AcceptanceError, money, verify


def evidence():
    account = {"id": "account", "broker_account_id": "DU_TEST", "environment": "Paper", "broker_kind": "ibkr"}
    trade = {"id": "trade", "account_id": "account", "currency": "USD", "category": "Long", "status": "New",
             "trading_vehicle": {"symbol": "TEST", "category": "Stock"}}
    for role, oid in [("entry", "100"), ("safety_stop", "101"), ("target", "102")]:
        trade[role] = {"broker_order_id": oid, "quantity": "1", "filled_quantity": "0", "average_filled_price": None, "status": "New"}
    snapshot = {"scope": {"broker": "ibkr", "environment": "paper", "account_id": "DU_TEST"}, "currency": "USD",
                "complete": True, "consistent": True, "positions": [], "working_orders": [], "cash": "1000"}
    captures = {}
    for index, stage in enumerate(("before", "protected", "reconnected", "closed")):
        stamp = f"2026-10-27T12:0{index}:00+00:00"
        s, t = copy.deepcopy(snapshot), copy.deepcopy(trade)
        s["observed_at"] = stamp
        if stage != "before":
            t["status"] = "Filled"
            t["entry"].update(filled_quantity="1", average_filled_price="100", status="Filled")
            s["positions"] = [{"quantity": "1"}]
            s["working_orders"] = [{"order_id": oid, "side": "sell", "remaining_quantity": "1"} for oid in ("101", "102")]
        captures[stage] = {"stage": stage, "source_fingerprint": "source", "captured_at": stamp,
                           "account": copy.deepcopy(account), "trade": t, "executions": [], "transactions": [],
                           "readiness": {"snapshot": s, "unresolved_operation_count": 0}}
    final = captures["closed"]
    final["trade"]["status"] = "ClosedTarget"
    final["trade"]["target"].update(filled_quantity="1", average_filled_price="110", status="Filled")
    final["trade"]["safety_stop"]["status"] = "Canceled"
    final["readiness"]["snapshot"].update(cash="1008", positions=[], working_orders=[])
    fills = [{"account_id": "account", "trade_id": "trade", "symbol": "TEST", "broker_execution_id": f"fill-{oid}",
              "broker_order_id": oid, "side": side, "qty": "1", "price": price}
             for oid, side, price in (("100", "Buy", "100"), ("102", "Sell", "110"))]
    final["executions"] = copy.deepcopy(fills)
    final["transactions"] = [{"id": name, "account_id": "account", "currency": "USD", "category": {name: "trade"}, "amount": "1"}
                             for name in ("FeeOpen", "FeeClose")]
    statement = {"schema_version": 1, "complete": True, "broker_account_id": "DU_TEST", "currency": "USD",
                 "period_start": "2026-10-27T12:00:00Z", "period_end": "2026-10-27T12:04:00Z",
                 "opening_positions": [], "closing_positions": [], "closing_working_orders": [], "external_cash_flows": [],
                 "executions": fills, "fees_final": True, "opening_cash": "1000", "closing_cash": "1008",
                 "fees": [{"id": oid, "currency": "USD", "broker_order_id": oid, "amount": "1"} for oid in ("100", "102")]}
    return captures, statement


class EvidenceTests(unittest.TestCase):
    def test_exact_reconciliation_is_narrow_evidence_not_global_acceptance(self):
        result = verify(*evidence(), "source")
        self.assertEqual(result["status"], "reconciled")
        self.assertEqual(result["cash_delta"], "8")
        self.assertFalse(result["level_4_complete"])
        self.assertTrue(result["operator_review_required"])

    def test_mismatches_cannot_certify_money_or_protection(self):
        for fault in ("fees", "cash", "identity", "currency", "duplicate", "missing_fill", "no_fill", "stale",
                      "missing_stop", "oversize_stop", "unknown", "changed_ids", "residual", "sibling", "pending_fees",
                      "page_duplicate", "external_flow", "source", "sequence"):
            with self.subTest(fault=fault):
                captures, statement = evidence()
                final = captures["closed"]
                if fault == "fees": statement["fees"][0]["amount"] = "2"
                elif fault == "cash": statement["closing_cash"] = "1009"
                elif fault == "identity": statement["broker_account_id"] = "DU_OTHER"
                elif fault == "currency": statement["currency"] = "EUR"
                elif fault == "duplicate": statement["executions"].append(statement["executions"][0])
                elif fault == "missing_fill": final["executions"].pop()
                elif fault == "no_fill": final["trade"]["entry"]["filled_quantity"] = "0"
                elif fault == "stale": final["readiness"]["snapshot"]["observed_at"] = "2026-10-26T12:00:00Z"
                elif fault == "missing_stop": captures["protected"]["readiness"]["snapshot"]["working_orders"].pop(0)
                elif fault == "oversize_stop": captures["protected"]["readiness"]["snapshot"]["working_orders"][0]["remaining_quantity"] = "2"
                elif fault == "unknown": final["readiness"]["unresolved_operation_count"] = 1
                elif fault == "changed_ids": captures["reconnected"]["trade"]["entry"]["broker_order_id"] = "new-id"
                elif fault == "residual": final["readiness"]["snapshot"]["positions"] = [{"quantity": "1"}]
                elif fault == "sibling": final["trade"]["safety_stop"]["status"] = "New"
                elif fault == "pending_fees": statement["fees_final"] = False
                elif fault == "page_duplicate": final["transactions"].append(final["transactions"][0])
                elif fault == "external_flow": statement["external_cash_flows"] = ["unknown"]
                elif fault == "source": captures["protected"]["source_fingerprint"] = "old"
                elif fault == "sequence": captures["protected"]["captured_at"] = captures["before"]["captured_at"]
                with self.assertRaises(AcceptanceError):
                    verify(captures, statement, "source")

    def test_no_float_nan_exponent_or_implicit_money(self):
        for value in [1.1, 1, None, "NaN", "Infinity", "1e3", "1,000", " 1", "1" * 65]:
            with self.assertRaises(AcceptanceError):
                money(value)


if __name__ == "__main__":
    unittest.main()
