#!/usr/bin/env python3
"""Explicit releases from the clean, current origin/main commit; no builds here."""
import re
import subprocess
import sys


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ('app', 'cli'):
        raise SystemExit('Usage: release.py app|cli vX.Y.Z')
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
    tag = ('cli-' if kind == 'cli' else '') + version
    if git('tag', '--list', tag):
        raise SystemExit('Tag already exists; choose a new version')
    subprocess.run(['gh', 'release', 'create', tag, '--repo', 'mattmezza/todoer.pro',
                    '--target', revision, '--title', f'Todoer {kind} {version}',
                    '--generate-notes', '--latest=' + ('true' if kind == 'app' else 'false')], check=True)


if __name__ == '__main__':
    main()
