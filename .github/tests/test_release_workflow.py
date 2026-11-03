"""Release policy contracts against parsed jobs, dependency edges and action references."""
import unittest
import shlex
import json
from pathlib import Path
import subprocess
import tempfile

from workflow_support import ROOT, render, step, workflow


class ReleaseWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.workflow = workflow()
        self.jobs = self.workflow['jobs']

    def test_only_main_manifest_pushes_trigger_automatic_releases(self):
        self.assertEqual(self.workflow['on'], {'push': {'branches': ['main'], 'paths': ['Cargo.toml']}})

    def test_publication_requires_validation_and_a_changed_version(self):
        self.assertEqual(self.jobs['validate']['uses'], './.github/workflows/rust.yaml')
        self.assertEqual(set(self.jobs['create-release']['needs']), {'validate', 'check-version'})
        for name in ('create-release', 'build-and-upload', 'build-universal-macos'):
            with self.subTest(job=name):
                self.assertEqual(self.jobs[name]['if'], "needs.check-version.outputs.version_changed == 'true'")
        for name in ('build-and-upload', 'build-universal-macos'):
            self.assertEqual(set(self.jobs[name]['needs']), {'create-release', 'check-version'})
        for job in self.jobs.values():
            needs = job.get('needs', [])
            for dependency in ([needs] if isinstance(needs, str) else needs):
                self.assertIn(dependency, self.jobs)

    def test_write_authority_is_limited_to_publication_jobs(self):
        self.assertEqual(self.workflow['permissions'], {'contents': 'read'})
        writers = {'create-release', 'build-and-upload', 'build-universal-macos'}
        for name, job in self.jobs.items():
            expected = {'contents': 'write' if name in writers else 'read'}
            self.assertEqual(job.get('permissions', self.workflow['permissions']), expected, name)
        for name in writers:
            job = self.jobs[name]
            operation = 'Create Release' if name == 'create-release' else 'Upload Release Asset'
            self.assertEqual(step(job, operation)['env']['GITHUB_TOKEN'], '${{ secrets.GITHUB_TOKEN }}')

    def test_every_remote_action_is_immutable_and_checkout_drops_credentials(self):
        references = 0
        for job in self.jobs.values():
            for item in [job, *job.get('steps', [])]:
                uses = item.get('uses')
                if uses is None:
                    continue
                references += 1
                if uses.startswith('./'):
                    self.assertTrue((ROOT / uses).is_file(), uses)
                else:
                    self.assertRegex(uses, r'^[\w.-]+/[\w./-]+@[a-f0-9]{40}$')
                if uses.startswith('actions/checkout@'):
                    self.assertEqual(item.get('with', {}).get('persist-credentials'), 'false')
        self.assertGreater(references, 1)

    def test_build_matrix_matches_installer_platforms_and_pinned_rust(self):
        import tomllib
        toolchain = tomllib.loads((ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
        matrix = self.jobs['build-and-upload']['strategy']['matrix']['include']
        self.assertEqual({(r['target'], r['os'], r['archive_name']) for r in matrix}, {
            ('x86_64-unknown-linux-gnu', 'ubuntu-latest', 'x86_64-unknown-linux-gnu'),
            ('aarch64-apple-darwin', 'macos-latest', 'aarch64-apple-darwin'),
            ('x86_64-apple-darwin', 'macos-latest', 'x86_64-apple-darwin'),
        })
        self.assertEqual(len(matrix), 3)
        self.assertEqual(self.jobs['build-and-upload']['runs-on'], '${{ matrix.os }}')
        for name in ('build-and-upload', 'build-universal-macos'):
            self.assertEqual(step(self.jobs[name], 'Install Rust')['with']['toolchain'], toolchain)

    def test_platform_builds_the_installed_client_with_the_locked_dependency_graph(self):
        job = self.jobs['build-and-upload']
        for row in job['strategy']['matrix']['include']:
            with self.subTest(target=row['target']):
                script = render(step(job, 'Build')['run'], {'matrix.target': row['target']})
                self.assertEqual(shlex.split(script, comments=True), [
                    'cargo', 'build', '--locked', '--release', '--target', row['target'], '--bin', 'trust'])

    def invoke_publication_step(self, job_name, step_name, values):
        # Execute the real workflow shell with only the GitHub network boundary substituted.
        with tempfile.TemporaryDirectory(prefix='trust-release-command-') as directory:
            root = Path(directory)
            gh = root / 'gh'
            gh.write_text("#!/usr/bin/python3\nimport json,os,sys\nfrom pathlib import Path\nPath(os.environ['CAPTURE']).write_text(json.dumps(sys.argv[1:]))\n")
            gh.chmod(0o755)
            invocation = step(self.jobs[job_name], step_name)
            env = {'PATH': str(root) + ':/usr/bin:/bin', 'HOME': directory,
                   'CAPTURE': str(root / 'args.json'), 'GITHUB_SHA': 'a' * 40,
                   'GITHUB_TOKEN': 'test-release-token'}
            script = render(invocation['run'], values)
            subprocess.run(['/bin/bash', '-euo', 'pipefail', '-c', script], cwd=root,
                           env=env, check=True, capture_output=True, text=True, timeout=10)
            return json.loads((root / 'args.json').read_text())

    def test_release_creation_targets_the_validated_version_and_commit(self):
        args = self.invoke_publication_step('create-release', 'Create Release', {
            'needs.check-version.outputs.new_version': '1.2.3'})
        self.assertEqual(args, ['release', 'create', 'v1.2.3', '--target', 'a' * 40,
                               '--title', 'Release v1.2.3', '--notes', 'Automated release for version 1.2.3'])

    def test_each_upload_attaches_both_archive_and_checksum_to_the_same_release(self):
        for row in self.jobs['build-and-upload']['strategy']['matrix']['include']:
            with self.subTest(target=row['target']):
                args = self.invoke_publication_step('build-and-upload', 'Upload Release Asset', {
                    'needs.check-version.outputs.new_version': '1.2.3', 'matrix.archive_name': row['archive_name']})
                archive = f"./v1.2.3-{row['target']}.tar.gz"
                self.assertEqual(args, ['release', 'upload', 'v1.2.3', archive, archive + '.sha256'])
        args = self.invoke_publication_step('build-universal-macos', 'Upload Release Asset', {
            'needs.check-version.outputs.new_version': '1.2.3'})
        self.assertEqual(args, ['release', 'upload', 'v1.2.3', './v1.2.3-universal-apple-darwin.tar.gz',
                               './v1.2.3-universal-apple-darwin.tar.gz.sha256'])


if __name__ == '__main__':
    unittest.main()
