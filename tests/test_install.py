"""Installer safety regressions; all downloads are mocked and nothing is executed."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


class InstallerTests(unittest.TestCase):
    def run_installer(self, mode):
        with tempfile.TemporaryDirectory(prefix="trust-installer-test-") as directory:
            root = Path(directory)
            mock_bin = root / "bin"
            mock_bin.mkdir()
            archive = root / "fixture.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                data = b"fixture binary; never execute\n"
                member = tarfile.TarInfo("trust")
                member.size = len(data)
                bundle.addfile(member, io.BytesIO(data))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / "fixture.sha256").write_text(digest + "  fixture.tar.gz\n")
            curl = mock_bin / "curl"
            curl.write_text("""#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in -o) shift; output=$1 ;; http*) url=$1 ;; esac
  shift
done
case "$url" in
  *.sha256)
    [ "$TEST_MODE" != missing ] || exit 22
    if [ "$TEST_MODE" = mismatch ]; then
      printf '%064d\n' 0 > "$output"
    else cp "$TEST_FIXTURES/fixture.sha256" "$output"; fi ;;
  *.tar.gz) cp "$TEST_FIXTURES/fixture.tar.gz" "$output" ;;
  *) exit 23 ;;
esac
""")
            curl.chmod(0o755)
            environment = {
                "PATH": str(mock_bin) + os.pathsep + "/usr/bin:/bin",
                "HOME": directory,
                "INSTALL_DIR": str(root / "installed"),
                "TRUST_VERSION": "1.0.0",
                "TEST_MODE": mode,
                "TEST_FIXTURES": directory,
            }
            result = subprocess.run(["/bin/sh", str(INSTALLER)], env=environment,
                                    capture_output=True, text=True, check=False)
            return result.returncode, (root / "installed/trust").exists()

    def test_missing_checksum_never_installs(self):
        code, installed = self.run_installer("missing")
        self.assertNotEqual(code, 0)
        self.assertFalse(installed)

    def test_wrong_checksum_never_installs(self):
        code, installed = self.run_installer("mismatch")
        self.assertNotEqual(code, 0)
        self.assertFalse(installed)

    def test_verified_archive_installs(self):
        self.assertEqual(self.run_installer("valid"), (0, True))


if __name__ == "__main__":
    unittest.main()
