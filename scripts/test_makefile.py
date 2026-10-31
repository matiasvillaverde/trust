"""Exercise Make's dependency graph without compiling or touching a real database."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class BuildIsolationTests(unittest.TestCase):
    def run_target(self, target):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "owner.db"
            database.write_bytes(b"operator-owned fixture")
            trace = root / "commands.jsonl"
            stub = root / "tool.py"
            stub.write_text(
                "import json, os, sys\n"
                "with open(os.environ['TRUST_BUILD_TRACE'], 'a') as trace:\n"
                "    trace.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            )
            environment = dict(os.environ, TRUST_BUILD_TRACE=str(trace))
            for name in ("MAKEFLAGS", "MFLAGS", "MAKEOVERRIDES"):
                environment.pop(name, None)
            result = subprocess.run(
                ["make", "--no-print-directory", target,
                 "CARGO=" + shlex.join([sys.executable, str(stub), "cargo"]),
                 "DIESEL_CLI=" + shlex.join([sys.executable, str(stub), "diesel"]),
                 "RUSTC=false", "CLI_DATABASE_URL=" + str(database)],
                cwd=ROOT, env=environment, capture_output=True, text=True, timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(database.read_bytes(), b"operator-owned fixture")
            commands = [json.loads(line) for line in trace.read_text().splitlines()]
            return commands, str(database)

    def test_build_and_test_targets_do_not_run_database_commands(self):
        for target in ("build", "build-release", "test", "test-single", "ci-test", "ci-build"):
            with self.subTest(target=target):
                commands, database = self.run_target(target)
                self.assertTrue(commands)
                self.assertTrue(all(command[0] == "cargo" for command in commands), commands)
                self.assertTrue(all("--locked" in command for command in commands), commands)
                self.assertTrue(all(database not in command for command in commands), commands)

    def test_database_setup_and_migrations_remain_explicit(self):
        for target in ("setup", "migration", "clean-db"):
            with self.subTest(target=target):
                commands, database = self.run_target(target)
                self.assertEqual(len(commands), 1)
                self.assertEqual(commands[0][0], "diesel")
                self.assertIn(database, commands[0])


if __name__ == "__main__":
    unittest.main()
