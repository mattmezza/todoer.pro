#!/usr/bin/env python3
"""Explicit releases from the clean, current origin/main commit."""
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path


TAG_PREFIX = {'app': '', 'cli': 'cli-', 'extension': 'extension-'}


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in TAG_PREFIX:
        raise SystemExit('Usage: release.py app|cli|extension vX.Y.Z')
    kind, version = sys.argv[1:]
    if not re.fullmatch(r'v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise SystemExit('Expected name=vX.Y.Z (stable semantic version)')
    if git('status', '--porcelain'):
        raise SystemExit('Commit or stash changes before releasing')
    if git('branch', '--show-current') != 'main':
        raise SystemExit('Release from main only')
    subprocess.run(['git', 'fetch', 'origin', 'main', '--tags'], check=True)
    revision = git('rev-parse', 'HEAD')
    if revision != git('rev-parse', 'origin/main'):
        raise SystemExit('Local HEAD must equal current origin/main')
    tag = TAG_PREFIX[kind] + version
    if git('tag', '--list', tag):
        raise SystemExit('Tag already exists; choose a new version')
    command = ['gh', 'release', 'create', tag, '--repo', 'mattmezza/todoer.pro',
               '--target', revision, '--title', f'Todoer {kind} {version}',
               '--generate-notes', '--latest=' + ('true' if kind == 'app' else 'false')]
    if kind != 'extension':
        subprocess.run(command, check=True)
        return

    manifest = json.loads(git('show', f'{revision}:extension/manifest.json'))
    if version != 'v' + manifest['version']:
        raise SystemExit(f"Expected name=v{manifest['version']} to match extension/manifest.json")
    with tempfile.TemporaryDirectory(prefix='todoer-extension-release-') as directory:
        asset = Path(directory) / f'todoer-pro-{manifest["version"]}-chromium.zip'
        subprocess.run(['git', 'archive', '--format=zip', '--output', str(asset),
                        f'{revision}:extension'], check=True)
        command.extend(['--notes', 'For Helium or Chrome, extract the Chromium ZIP. Open '
                        '`chrome://extensions`, enable Developer mode, choose **Load unpacked**, '
                        'and select the extracted folder containing `manifest.json`.'])
        subprocess.run([*command, str(asset)], check=True)


if __name__ == '__main__':
    main()
