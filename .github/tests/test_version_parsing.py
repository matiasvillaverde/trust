import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from read_version import parse_version


class ReleaseVersionTests(unittest.TestCase):
    def test_accepts_semantic_versions_from_package_or_workspace(self):
        for table in ("package", "workspace.package"):
            for version in ("0.3.5", "2.15.7", "1.0.0-alpha.1", "3.2.1-beta.2+exp.sha.abc123"):
                with self.subTest(table=table, version=version):
                    self.assertEqual(parse_version(f'[{table}]\nversion = "{version}"\n'), version)

    def test_rejects_shell_payloads_and_invalid_versions(self):
        for version in ("$(touch /tmp/trust-injected)", "`id`", "1.0.0;id", "", "01.0.0", "1.0.0-01"):
            with self.subTest(version=version):
                with self.assertRaises(ValueError):
                    parse_version(f'[workspace.package]\nversion = "{version}"\n')

    def test_rejects_missing_nonstring_and_malformed_manifests(self):
        for document in ('[package]\nname="trust"', '[package]\nversion=3', '[package]\nversion="broken'):
            with self.subTest(document=document):
                with self.assertRaises(ValueError):
                    parse_version(document)

    def test_uses_workspace_version(self):
        self.assertEqual(parse_version('[workspace.package]\nversion="1.2.3"\n[package]\nversion={workspace=true}'), "1.2.3")


if __name__ == "__main__":
    unittest.main()
