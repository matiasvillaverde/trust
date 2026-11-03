"""Package and install a real compiled CLI using the workflow's archive/checksum commands.

Run after `cargo build --locked --release --bin trust`. Only downloads are substituted;
tar, checksums, install.sh and the installed executable are real. No release is published.
"""
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.github/tests'))
from workflow_support import render, step, workflow
sys.path.insert(0, str(ROOT / '.github/scripts'))
from read_version import parse_version


class ReleaseArtifactTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        binary = Path(os.environ.get('TRUST_RELEASE_TEST_BINARY', ROOT / 'target/release/trust')).resolve()
        if not binary.is_file():
            raise AssertionError('Build the CLI first: cargo build --locked --release --bin trust')
        cls.temporary = tempfile.TemporaryDirectory(prefix='trust-release-artifact-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name)
        cls.version = parse_version((ROOT / 'Cargo.toml').read_text())
        targets = {('Linux', 'x86_64'): 'x86_64-unknown-linux-gnu',
                   ('Darwin', 'arm64'): 'aarch64-apple-darwin', ('Darwin', 'x86_64'): 'x86_64-apple-darwin'}
        target = targets[(platform.system(), platform.machine())]
        job = workflow()['jobs']['build-and-upload']
        rows = [r for r in job['strategy']['matrix']['include'] if r['target'] == target]
        if len(rows) != 1:
            raise AssertionError(f'Expected one release target for {target}')
        if rows[0]['archive_name'] != target:
            raise AssertionError('Archive name must match the installer platform')
        cls.asset = f"v{cls.version}-{rows[0]['archive_name']}.tar.gz"
        destination = cls.root / 'target' / target / 'release/trust'
        destination.parent.mkdir(parents=True)
        shutil.copy2(binary, destination)
        cls.binary_digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        values = {'matrix.target': target, 'matrix.archive_name': rows[0]['archive_name'],
                  'needs.check-version.outputs.new_version': cls.version}
        tool_paths = {'/usr/bin', '/bin'}
        for name in ('tar', 'shasum'):
            executable = shutil.which(name)
            if executable is None:
                raise AssertionError(f'Install {name} to exercise the real packaging commands')
            tool_paths.add(str(Path(executable).resolve().parent))
        env = {'PATH': os.pathsep.join(sorted(tool_paths)), 'HOME': str(cls.root)}
        for name in ('Create archive', 'Generate checksum'):
            result = subprocess.run(['/bin/bash', '-euo', 'pipefail', '-c', render(step(job, name)['run'], values)],
                                    cwd=cls.root, env=env, capture_output=True, text=True, timeout=60)
            if result.returncode:
                raise AssertionError(f'{name} failed: {result.stderr}')

    def test_archive_contains_the_built_executable_and_matching_checksum(self):
        archive = self.root / self.asset
        expected = (self.root / (self.asset + '.sha256')).read_text().split()
        self.assertEqual(expected, [hashlib.sha256(archive.read_bytes()).hexdigest(), self.asset])
        with tarfile.open(archive) as bundle:
            self.assertEqual(bundle.getnames(), ['trust'])
            self.assertTrue(bundle.getmember('trust').mode & 0o111)
            self.assertEqual(hashlib.sha256(bundle.extractfile('trust').read()).hexdigest(), self.binary_digest)

    def install(self, tamper=False):
        directory = tempfile.TemporaryDirectory(prefix='trust-release-install-')
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        assets = root / 'assets'
        assets.mkdir()
        for name in (self.asset, self.asset + '.sha256'):
            shutil.copy2(self.root / name, assets / name)
        if tamper:
            with (assets / self.asset).open('ab') as archive:
                archive.write(b'changed-after-checksum')
        commands = root / 'commands'
        commands.mkdir()
        curl = commands / 'curl'
        curl.write_text('''#!/usr/bin/python3
import os, pathlib, shutil, sys, urllib.parse
args=sys.argv[1:]
output=args[args.index('-o')+1]
url=urllib.parse.urlparse(args[-1])
assert url.scheme == 'https' and url.netloc == 'github.com'
assert url.path.startswith('/matiasvillaverde/trust/releases/download/v'+os.environ['TRUST_VERSION']+'/')
shutil.copyfile(pathlib.Path(os.environ['TEST_ASSETS'])/pathlib.Path(url.path).name, output)
''')
        curl.chmod(0o755)
        env = {'PATH': str(commands) + ':/usr/bin:/bin', 'HOME': str(root),
               'INSTALL_DIR': str(root / 'installed'), 'TRUST_VERSION': self.version, 'TEST_ASSETS': str(assets)}
        result = subprocess.run(['/bin/sh', str(ROOT / 'install.sh')], env=env, cwd=root,
                                capture_output=True, text=True, timeout=60)
        return result, root / 'installed/trust'

    def test_installed_cli_runs_with_the_expected_version_and_commands(self):
        result, binary = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(hashlib.sha256(binary.read_bytes()).hexdigest(), self.binary_digest)
        self.assertTrue(os.access(binary, os.X_OK))
        for args, expected in [(['--version'], f'trust {self.version}'), (['--help'], 'Usage:')]:
            executed = subprocess.run([str(binary), *args], env={'PATH': '/usr/bin:/bin', 'HOME': str(binary.parent.parent)}, capture_output=True, text=True, timeout=10)
            self.assertEqual(executed.returncode, 0, executed.stderr)
            self.assertIn(expected, executed.stdout)

    def test_tampered_built_archive_is_rejected_before_installation(self):
        result, binary = self.install(tamper=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Checksum verification failed', result.stderr)
        self.assertFalse(binary.exists())


if __name__ == '__main__':
    unittest.main()
