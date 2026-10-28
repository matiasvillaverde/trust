import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import package_skills as package


class PackageTests(unittest.TestCase):
    def test_agent_and_owner_packages_preserve_source_and_hash_every_file(self):
        for audience, count in (("agent", 2), ("owner", 3)):
            with self.subTest(audience=audience), tempfile.TemporaryDirectory() as tmp:
                output = Path(tmp) / "plugin"
                with patch("urllib.request.urlopen", side_effect=AssertionError("Packaging must stay offline")):
                    package.build(output, "https://trust.example.com/mcp", audience)
                plugin = json.loads((output / "plugin.json").read_text())
                self.assertEqual(plugin["$schema"], package.SCHEMA + "plugin.schema.json")
                self.assertEqual(plugin["name"], "trust-" + audience)
                mcp = json.loads((output / "mcp.json").read_text())
                self.assertEqual(mcp["mcpServers"]["trust"]["type"], "streamable-http")
                self.assertEqual(set(mcp["mcpServers"]["trust"]), {"type", "url"})
                manifest = json.loads((output / "skills-manifest.json").read_text())
                self.assertEqual(len(manifest["skills"]), count)
                for skill in manifest["skills"]:
                    self.assertEqual(skill["version"], plugin["version"])
                    for entry in skill["files"]:
                        content = (output / "skills" / skill["name"] / entry["path"]).read_bytes()
                        self.assertEqual(entry["size_bytes"], len(content))
                        self.assertEqual(entry["sha256"], hashlib.sha256(content).hexdigest())
                        if entry["path"].endswith(".md"):
                            self.assertEqual(content, (package.ROOT / "skills" / skill["name"] / entry["path"]).read_bytes())
                        else:
                            self.assertIn(b'url: "https://trust.example.com/mcp"', content)
                self.assertEqual((output / "skills/trust-admin").exists(), audience == "owner")

    def test_endpoints_reject_credentials_insecure_hosts_and_parser_ambiguities(self):
        bad = ["http://example.com/mcp", "https://user:secret@example.com/mcp", "https://user@example.com/mcp",
               "https://example.com/mcp?access_token=secret", "https://example.com/mcp?", "https://example.com/mcp#",
               "https://example.com/mcp#secret", "file:///mcp", "https://${TRUST_HOST}/mcp", "https://example.com:bad/mcp",
               "https://example.com:0/mcp", "https://example.com:65536/mcp", "https://example.com/../mcp",
               "https://example.com/%2e/mcp", "https://example.com\\@evil.com/mcp", "https://example.com/ mcp",
               "https://example.com/\nmcp", "https://example.com/mcp\x7f", "https://example.com/not-mcp",
               "http://localhost.evil.com/mcp"]
        for value in bad:
            with self.subTest(value=value), self.assertRaises(ValueError):
                package.endpoint(value)
        for value in ["https://trust.example.com/mcp", "http://localhost:8000/mcp", "http://127.0.0.1:8000/mcp", "http://[::1]:8000/mcp"]:
            self.assertEqual(package.endpoint(value), value)

    def test_existing_output_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "existing"
            output.mkdir()
            sentinel = output / "do-not-change"
            sentinel.write_text("original")
            with self.assertRaises(ValueError):
                package.build(output, "https://trust.example.com/mcp")
            self.assertEqual(sentinel.read_text(), "original")
            self.assertEqual(list(output.iterdir()), [sentinel])

    def test_symlink_sources_and_unexpected_private_files_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            directory = root / "skills/trust-risk"
            directory.mkdir(parents=True)
            (directory / "SKILL.md").write_text((package.ROOT / "skills/trust-risk/SKILL.md").read_text())
            link = directory / "secret.md"
            link.symlink_to(package.ROOT / "Cargo.toml")
            with self.assertRaises(ValueError):
                package.source_files(root, "trust-risk")
            link.unlink()
            (directory / "credentials.json").write_text('{}')
            with self.assertRaises(ValueError):
                package.source_files(root, "trust-risk")


if __name__ == "__main__":
    unittest.main()
