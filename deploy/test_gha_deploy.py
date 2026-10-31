"""Exercise deployment ordering and recovery without touching Docker or a server."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('gha_deploy', Path(__file__).with_name('gha-deploy.py'))
deploy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(deploy)


class DeployTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'deploy').mkdir()
        (self.root / 'deploy/.env').write_text('TODOER_IMAGE=old\n')
        self.old_cwd = os.getcwd()
        self.addCleanup(os.chdir, self.old_cwd)
        self.events = []

    def exercise(self, fail_sync=False):
        def run(*args, **kwargs):
            self.events.append(args)
            if fail_sync and args[:3] == ('docker', 'exec', 'todoer-litestream-1'):
                raise subprocess.CalledProcessError(1, args)

        def output(args, **kwargs):
            if args[0] == 'git':
                return ''
            if 'Health.Status' in args[-2]:
                return 'healthy\n'
            if 'State.Running' in args[-2]:
                return 'true\n'
            return 'old-container\n'

        def subprocess_run(args, **kwargs):
            self.events.append(tuple(args))
            return subprocess.CompletedProcess(args, 0, stdout='old-container\n')

        env = {'SSH_ORIGINAL_COMMAND': 'deploy v1.2.3 sha256:' + 'a' * 64}
        with mock.patch.dict(os.environ, env), mock.patch.object(deploy, '__file__', str(self.root / 'deploy/gha-deploy.py')), \
             mock.patch.object(deploy, 'run', side_effect=run), \
             mock.patch.object(deploy.subprocess, 'check_output', side_effect=output), \
             mock.patch.object(deploy.subprocess, 'run', side_effect=subprocess_run):
            deploy.main()

    def test_sync_precedes_deployment_and_pins_image(self):
        self.exercise()
        events = [event[:3] for event in self.events]
        self.assertLess(events.index(('docker', 'stop', '--time')), events.index(('docker', 'exec', 'todoer-litestream-1')))
        self.assertLess(events.index(('docker', 'exec', 'todoer-litestream-1')), events.index(('docker', 'compose', '--project-name')))
        self.assertIn('v1.2.3@sha256:' + 'a' * 64, (self.root / 'deploy/.env').read_text())

    def test_failed_sync_restarts_original_container_without_deploy(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.exercise(fail_sync=True)
        self.assertIn(('docker', 'start', 'todoer-app-1'), self.events)
        self.assertFalse(any(event[:2] == ('docker', 'compose') for event in self.events))
        self.assertEqual((self.root / 'deploy/.env').read_text(), 'TODOER_IMAGE=old\n')

    def test_rejects_shell_metacharacters(self):
        with mock.patch.dict(os.environ, {'SSH_ORIGINAL_COMMAND': 'deploy v1.2.3; id sha256:' + 'a' * 64}):
            with self.assertRaises(SystemExit):
                deploy.main()


if __name__ == '__main__':
    unittest.main()
