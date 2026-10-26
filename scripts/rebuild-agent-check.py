#!/usr/bin/env -S uv run --no-project --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Isolated real CLI + official MCP SDK + Moto attachment integration.

Build todoer/todoer-server first. Requires uv, Python3.11, and permission to bind
loopback sockets. Downloads pinned Moto into uv's cache when needed. It never
uses an existing server or inherited Todoer configuration. Credentials below
are public synthetic fixtures. Signed upload URL logging is suppressed.
"""

import hashlib
import json
import os
import pathlib
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
os.chdir(ROOT)
PHASE = "fixture-startup"


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


MOTO_PORT = free_port()
APP_PORT = free_port()
while APP_PORT == MOTO_PORT:
    APP_PORT = free_port()
fixture = [
    'uv',
    'run',
    '--no-project',
    '--python',
    '3.11',
    '--with',
    'moto[server]==5.1.17',
]


def ready(url):
    for _ in range(200):
        try:
            with urllib.request.urlopen(url, timeout=1):
                return
        except Exception:
            time.sleep(0.1)
    raise RuntimeError('isolated fixture startup timed out')


def stop(p):
    if p and p.poll() is None:
        os.killpg(p.pid, signal.SIGTERM)
        try:
            p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()


def main():
    global PHASE
    with tempfile.TemporaryDirectory(prefix='todoer-agent-features-') as tmp:
        moto = server = None
        try:
            moto = subprocess.Popen(
                fixture
                + ['moto_server', '-H', '127.0.0.1', '-p', str(MOTO_PORT)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            ready(f'http://127.0.0.1:{MOTO_PORT}/')
            bucket = 'todoer-synthetic-agent-' + uuid.uuid4().hex[:8]
            seed = (
                f"import boto3; boto3.client('s3',endpoint_url='http://127.0.0.1:{MOTO_PORT}',region_name='us-east-1',aws_access_key_id='synthetic-test',aws_secret_access_key='synthetic-test-secret').create_bucket(Bucket="
                + repr(bucket)
                + ")"
            )
            subprocess.run(
                fixture + ['python', '-c', seed],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith('TODOER_')
            }
            env.update(
                TODOER_DEV_AUTH='1',
                TODOER_DEV_TOKEN='isolated-agent-features-not-production',
                TODOER_TOKEN='isolated-agent-features-not-production',
                TODOER_RUST_DB=tmp + '/test.db',
                TODOER_RUST_PORT=str(APP_PORT),
                TODOER_URL=f'http://127.0.0.1:{APP_PORT}',
                TODOER_MCP_URL=f'http://127.0.0.1:{APP_PORT}/mcp',
                TODOER_TEST_ATTACHMENTS='1',
                TODOER_ATTACHMENTS_ENDPOINT=f'http://127.0.0.1:{MOTO_PORT}',
                TODOER_ATTACHMENTS_BUCKET=bucket,
                TODOER_ATTACHMENTS_REGION='us-east-1',
                TODOER_ATTACHMENTS_PREFIX='agent-fixture',
                TODOER_ATTACHMENTS_ACCESS_KEY_ID='synthetic-test',
                TODOER_ATTACHMENTS_SECRET_ACCESS_KEY='synthetic-test-secret',
            )
            server = subprocess.Popen(
                ['target/debug/todoer-server'],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            ready(f'http://127.0.0.1:{APP_PORT}/health')
            PHASE = 'official-MCP-SDK'
            subprocess.run(
                ['uv', 'run', '--no-project', 'scripts/rebuild-mcp-check.py'],
                env=env,
                check=True,
            )
            PHASE = 'CLI-operations'

            def cli(*args):
                return json.loads(
                    subprocess.check_output(
                        ['target/debug/todoer', '--json', *args], env=env
                    )
                )

            task = cli(
                'capture',
                '--list',
                'inbox',
                '--key',
                'cli-features',
                'CLI synthetic features',
            )[0]
            state = cli(
                'schedule',
                'set',
                task['id'],
                '--revision',
                '1',
                '--body',
                '{"planned":{"kind":"date","date":"2026-10-26"},"priority":2}',
            )
            assert (
                state['task_revision'] == 2
                and cli('schedule', 'get', task['id'])['schedule']['priority']
                == 2
            )
            assert any(
                t['task']['id'] == task['id']
                for t in cli(
                    'schedule',
                    'planning',
                    '--from',
                    '2026-10-26',
                    '--to',
                    '2026-10-28',
                )['tasks']
            )
            assert (
                cli('schedule', 'statistics', '--from', '2026-10-26')[
                    'scheduled_open'
                ]
                >= 1
            )
            series = cli(
                'schedule',
                'start',
                task['id'],
                '--revision',
                '2',
                '--body',
                '{"rule":{"kind":"daily","interval_days":1},"start_local":"2026-10-27T16:00:00","timezone":"Europe/Zurich","count":3}',
            )
            paused = cli(
                'schedule',
                'state',
                series['id'],
                '--revision',
                str(series['revision']),
                'paused',
            )
            assert paused['state'] == 'paused'
            cli(
                'schedule',
                'state',
                series['id'],
                '--revision',
                str(paused['revision']),
                'active',
            )
            assert len(cli('schedule', 'occurrences', series['id'])) == 1
            comment = cli(
                'comments',
                'add',
                task['id'],
                '--key',
                'cli-comment',
                'Synthetic CLI comment',
            )
            assert (
                cli(
                    'comments',
                    'add',
                    task['id'],
                    '--key',
                    'cli-comment',
                    'Synthetic CLI comment',
                )
                == comment
            )
            assert (
                'body_excerpt'
                in cli('comments', 'list', task['id'])['comments'][0]
            )
            assert (
                cli('comments', 'list', task['id'], '--full')['comments'][0][
                    'body'
                ]
                == 'Synthetic CLI comment'
            )
            content = b'Synthetic bounded CLI attachment\n' * 8000
            source = pathlib.Path(tmp) / 'source.txt'
            source.write_bytes(content)
            preview = cli(
                'files',
                'upload',
                task['id'],
                '--file',
                str(source),
                '--dry-run',
            )
            assert preview['sha256'] == hashlib.sha256(content).hexdigest()
            attachment = cli(
                'files', 'upload', task['id'], '--file', str(source)
            )
            assert attachment['sha256'] == preview['sha256']
            assert (
                cli(
                    'files',
                    'resume',
                    attachment['intent_id'],
                    '--file',
                    str(source),
                )['id']
                == attachment['id']
            )
            assert (
                cli(
                    'files',
                    'confirm',
                    attachment['intent_id'],
                    '--revision',
                    '1',
                )['id']
                == attachment['id']
            )
            info = cli('files', 'info', attachment['id'])
            assert (
                not info['mcp_oauth_credential_accepted']
                and 'object_key' not in info['attachment']
            )
            output = pathlib.Path(tmp) / 'verified.txt'
            cli('files', 'download', attachment['id'], '--output', str(output))
            assert output.read_bytes() == content
            refused = subprocess.run(
                [
                    'target/debug/todoer',
                    '--json',
                    'files',
                    'download',
                    attachment['id'],
                    '--output',
                    str(output),
                ],
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            assert refused.returncode == 2
            assert cli('files', 'delete', attachment['id'], '--revision', '1')[
                'deleted'
            ]
            edited = cli(
                'comments',
                'edit',
                comment['id'],
                '--revision',
                '1',
                'Revised CLI comment',
            )
            assert cli('comments', 'delete', edited['id'], '--revision', '2')[
                'deleted'
            ]
            workspace = cli(
                'workspace',
                'create',
                '--body',
                '{"title":"CLI isolated sharing"}',
            )
            invite = cli(
                'invitations',
                'create',
                'workspace',
                workspace['id'],
                'synthetic@example.invalid',
                '--key',
                'cli-invite',
                '--acknowledge-access-change',
            )
            assert (
                cli(
                    'invitations',
                    'create',
                    'workspace',
                    workspace['id'],
                    'synthetic@example.invalid',
                    '--key',
                    'cli-invite',
                    '--acknowledge-access-change',
                )['invitation']['id']
                == invite['invitation']['id']
            )
            assert (
                len(cli('invitations', 'list', 'workspace', workspace['id']))
                == 1
            )
            assert cli('invitations', 'revoke', invite['invitation']['id'])[
                'revoked'
            ]
            print(
                'CLI real HTTP + Moto check passed: schedules/planning/statistics/recurrence, comments/retry/edit/delete, SHA256 upload/resume/finalize/download/no-overwrite/delete, acknowledged invitation/retry/list/revoke'
            )
        finally:
            stop(server)
            stop(moto)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(
            f"Isolated agent check failed during {PHASE}: {type(error).__name__}",
            file=sys.stderr,
        )
        sys.exit(1)
