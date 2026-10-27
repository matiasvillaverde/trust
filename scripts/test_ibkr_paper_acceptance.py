import copy
from decimal import Decimal
from types import SimpleNamespace
import unittest

from ibkr_paper_acceptance import AcceptanceError, Mcp, run, validate_trade


class Client:
    def __init__(self, fail=None):
        self.calls = []
        self.fail = fail
        self.trade = {"account_id":"account", "category":"Long", "status":"New", "currency":"USD",
                      "trading_vehicle":{"category":"Stock"},
                      **{key:{"quantity":"1","unit_price":"100","filled_quantity":"0"}
                         for key in ("entry","target","safety_stop")}}
        self.account = {"id":"account", "environment":"Paper", "broker_kind":"ibkr", "broker_account_id":"DU_PAPER"}

    def call(self, name, args):
        self.calls.append((name, args))
        if name == self.fail:
            raise AcceptanceError("connection lost")
        if name == "trade_cancel":
            self.trade["status"] = "Canceled"
        data = {"account":self.account} if name == "account_get" else self.trade if name == "trade_get" else {}
        return {"data":data, "error":None, "request_id":"request", "operation_id":"operation"}


class PaperTests(unittest.TestCase):
    def test_requires_exact_paper_account_and_bounded_order(self):
        c = Client()
        validate_trade(c.account, c.trade, "account", "DU_PAPER", Decimal("100"))
        for field, value in [("environment","Live"), ("id","other"), ("broker_account_id","DU_OTHER")]:
            account = {**c.account, field:value}
            with self.assertRaises(AcceptanceError):
                validate_trade(account, c.trade, "account", "DU_PAPER", Decimal("100"))
        for quantity in ["0", "2", "NaN"]:
            trade = copy.deepcopy(c.trade)
            trade["entry"]["quantity"] = quantity
            with self.assertRaises(AcceptanceError):
                validate_trade(c.account, trade, "account", "DU_PAPER", Decimal("100"))
        with self.assertRaises(AcceptanceError):
            validate_trade(c.account, c.trade, "account", "DU_PAPER", Decimal("99.99"))

    def test_unknown_submission_never_retries_or_automatically_cancels(self):
        client = Client("trade_submit")
        receipt = {"steps":[]}
        persisted = []
        args = SimpleNamespace(account_id="account", trade_id="trade", broker_account_id="DU_PAPER", max_notional=Decimal("100"), run_id="stable")
        with self.assertRaises(AcceptanceError):
            run(client, args, receipt, lambda: persisted.append(copy.deepcopy(receipt)))
        self.assertEqual([name for name, _ in client.calls].count("trade_submit"), 1)
        self.assertNotIn("trade_cancel", [name for name, _ in client.calls])
        self.assertEqual(persisted[-1]["steps"][-1]["state"], "started")
        self.assertEqual(persisted[-1]["steps"][-1]["idempotency_key"], "paper-stable-trade_submit")

    def test_successful_smoke_cannot_certify_full_paper_level(self):
        receipt = {"steps":[]}
        args = SimpleNamespace(account_id="account", trade_id="trade", broker_account_id="DU_PAPER", max_notional=Decimal("100"), run_id="stable")
        run(Client(), args, receipt, lambda: None)
        self.assertEqual(receipt["status"], "smoke_passed")
        self.assertFalse(receipt["level_4_complete"])

    def test_credential_urls_remote_http_and_whitespace_tokens_are_rejected(self):
        for url, token in [("https://name:secret@example.com","token"), ("http://example.com","token"),
                           ("https://example.com?secret=x","token"), ("https://example.com","line\nbreak")]:
            with self.assertRaises(AcceptanceError):
                Mcp(url, token)


if __name__ == "__main__":
    unittest.main()
