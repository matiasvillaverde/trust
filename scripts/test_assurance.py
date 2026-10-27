import contextlib
import datetime as dt
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import assurance


class EvidenceTests(unittest.TestCase):
    def test_missing_ignored_failed_and_incomplete_tests_cannot_pass(self):
        good = "test safety ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n"
        self.assertEqual(assurance.evaluate(good, [0], {"safety"})["status"], "passed")
        for text, code, required in [(good, [1], {"safety"}), (good, [0], {"other"}),
                (good.split("test result")[0], [0], {"safety"}),
                ("test safety ... ignored\ntest result: ok. 0 passed; 0 failed; 1 ignored;", [0], {"safety"}),
                ("test safety ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;", [0], {"safety"})]:
            self.assertEqual(assurance.evaluate(text, code, required)["status"], "failed")

    def test_source_changes_expiry_and_tampering_invalidate_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = "test safety ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n"
            (root / "level-1.log").write_text(log)
            report = {"generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
                      "source_fingerprint": "current", "log_sha256": hashlib.sha256(log.encode()).hexdigest(),
                      "status": "passed", "returncodes": [0]}
            path = root / "level-1.json"
            path.write_text(json.dumps(report))
            with patch.dict(assurance.REQUIRED, {1: {"safety"}}):
                self.assertEqual(assurance.current_report(1, "current", root)["status"], "passed")
                self.assertEqual(assurance.current_report(1, "changed", root)["status"], "stale")
                (root / "level-1.log").write_text(log + "tamper")
                self.assertEqual(assurance.current_report(1, "current", root)["status"], "invalid_evidence")
                (root / "level-1.log").write_text(log)
                report["generated_at_utc"] = (dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=8)).isoformat()
                path.write_text(json.dumps(report))
                self.assertEqual(assurance.current_report(1, "current", root)["status"], "stale")

    def test_higher_levels_and_unapproved_paper_connection_never_dispatch(self):
        for level in [3, 4, 5, 6]:
            with patch("sys.argv", ["assurance", "run", "--level", str(level)]), \
                    patch.object(assurance, "fingerprint", return_value="current"), \
                    patch.object(assurance, "current_report", return_value={"status":"passed"}), \
                    patch.object(assurance.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    assurance.main()
                run.assert_not_called()

    def test_paper_preflight_only_selects_non_order_tests(self):
        for command in assurance.commands(3):
            self.assertIn("--ignored", command)
            self.assertIn("--exact", command)
            self.assertTrue(any(name in command for name in assurance.REQUIRED[3]))
            self.assertFalse(any("submits_cancels" in arg for arg in command))


if __name__ == "__main__":
    unittest.main()
