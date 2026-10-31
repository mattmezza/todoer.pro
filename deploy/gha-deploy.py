#!/usr/bin/env python3
"""Forced SSH command for an explicitly selected production release."""
import fcntl
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    request = os.environ.get('SSH_ORIGINAL_COMMAND', '')
    match = re.fullmatch(r'deploy (v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)) (sha256:[a-f0-9]{64})', request)
    if not match:
        raise SystemExit('Expected deploy vX.Y.Z sha256:<digest>')
    version, digest = match.groups()
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)
    private = root / '.private'
    private.mkdir(mode=0o700, exist_ok=True)
    with (private / 'gha-deploy.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        status = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], text=True)
        if status:
            raise SystemExit('Tracked checkout changes must be resolved before deployment')
        config = root / 'deploy/.env'
        if sum(line.startswith('TODOER_IMAGE=') for line in config.read_text().splitlines()) != 1:
            raise SystemExit('Expected exactly one TODOER_IMAGE entry in deploy/.env')
        run('git', 'fetch', '--depth=1', 'origin', f'refs/tags/{version}:refs/tags/{version}')
        image = f'ghcr.io/mattmezza/todoer.pro:{version}@{digest}'
        run('docker', 'pull', image)
        original = subprocess.check_output(['docker', 'inspect', '--format', '{{.Id}}', 'todoer-app-1'], text=True).strip()
        # Freeze writes before waiting for a confirmed remote replication position.
        run('docker', 'stop', '--time', '45', 'todoer-app-1')
        try:
            run('docker', 'exec', 'todoer-litestream-1', 'litestream', 'sync',
                '-socket', '/tmp/litestream.sock', '-wait', '-timeout', '60', '/data/todoer.db')
            run('git', 'checkout', '--detach', version)
            env = os.environ.copy()
            env['TODOER_IMAGE'] = image
            deploy = root / 'deploy'
            run('docker', 'compose', '--project-name', 'todoer', 'config', '--quiet', cwd=deploy, env=env)
            run('docker', 'compose', '--project-name', 'todoer', 'up', '-d', '--no-deps', 'app', cwd=deploy, env=env)
        except BaseException:
            current = subprocess.run(['docker', 'inspect', '--format', '{{.Id}}', 'todoer-app-1'], capture_output=True, text=True)
            if current.returncode == 0 and current.stdout.strip() == original:
                subprocess.run(['docker', 'start', 'todoer-app-1'], check=False)
            raise
        for _ in range(60):
            health = subprocess.check_output(['docker', 'inspect', '--format', '{{.State.Health.Status}}', 'todoer-app-1'], text=True).strip()
            if health == 'healthy':
                break
            if health == 'unhealthy':
                raise SystemExit('New app is unhealthy; database rollback was not attempted')
            time.sleep(2)
        else:
            raise SystemExit('Timed out waiting for app health')
        lines = config.read_text().splitlines(keepends=True)
        if sum(line.startswith('TODOER_IMAGE=') for line in lines) != 1:
            raise SystemExit('Expected exactly one TODOER_IMAGE entry; update deploy/.env manually')
        tmp = deploy / '.env.gha-new'
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, 'w') as out:
            out.writelines('TODOER_IMAGE=' + image + '\n' if line.startswith('TODOER_IMAGE=') else line for line in lines)
        os.replace(tmp, config)
        run('curl', '--fail', '--silent', '--show-error', '--max-time', '15', 'https://app.todoer.pro/ready', stdout=subprocess.DEVNULL)
        replication = subprocess.check_output(['docker', 'inspect', '--format', '{{.State.Running}}', 'todoer-litestream-1'], text=True).strip()
        if replication != 'true':
            raise SystemExit('App is healthy, but Litestream is not running')
        print(f'Deployed {version} at {digest}; app and public readiness checks passed')


if __name__ == '__main__':
    try:
        main()
    except (OSError, subprocess.CalledProcessError) as error:
        print(f'Deployment failed: {error}', file=sys.stderr)
        sys.exit(1)
