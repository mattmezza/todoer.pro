#!/usr/bin/env python3
"""Run real Google authentication against a separate local database."""
import argparse
import base64
import json
import os
from pathlib import Path
import secrets
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--with-local-storage', action='store_true',
                    help='Use the bundled loopback MinIO attachment service')
args = parser.parse_args()

root = Path(__file__).resolve().parent.parent
os.chdir(root)
# Askama templates are compiled into the server; always launch the current source.
subprocess.run(['cargo', 'build', '--locked', '--bin', 'todoer-server'], check=True)
credential_path = Path.home() / '.config/todoer/google-local.json'
credentials = json.loads(credential_path.read_text())['web']
origin = 'https://localhost:8443'
if origin + '/auth/google/callback' not in credentials.get('redirect_uris', []):
    raise SystemExit('Register https://localhost:8443/auth/google/callback first')
email = os.environ.get('TODOER_REGISTRATION_ALLOWLIST', '').strip()
if not email:
    raise SystemExit('Set TODOER_REGISTRATION_ALLOWLIST to your Google test email')
state = root / '.local/rebuild'
state.mkdir(parents=True, exist_ok=True)
key_path = state / 'google-auth-server.key'
if not key_path.exists():
    fd = os.open(key_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, 'w') as file:
        file.write(secrets.token_hex(32))
push_key_path = state / 'push-local.key'
if not push_key_path.exists():
    fd = os.open(push_key_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(fd, 'w') as file:
        file.write(base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('='))
# Use only deliberate configuration, rather than inherited development adapters/workers.
env = {key: value for key, value in os.environ.items() if not key.startswith('TODOER_')}
env.update({key:value for key,value in os.environ.items() if key.startswith(('TODOER_SMTP_', 'TODOER_PUSH_')) or key in ('TODOER_AUTH_ADMIN_EMAILS','TODOER_GOOGLE_LOGIN_ENABLED','TODOER_SIGNUP_ENABLED')})
if args.with_local_storage:
    # Fixed local fixture settings, never inherited production storage credentials.
    for line in (root / 'deploy/local/attachments.env').read_text().splitlines():
        if line.startswith('export TODOER_ATTACHMENTS_'):
            key, value = line.removeprefix('export ').split('=', 1)
            env[key] = value
    env['TODOER_ATTACHMENTS_PREFIX'] = 'local-google-auth'
env.update({
    'TODOER_PUSH_VAPID_PRIVATE_KEY': env.get('TODOER_PUSH_VAPID_PRIVATE_KEY', push_key_path.read_text().strip()),
    'TODOER_PUSH_VAPID_SUBJECT': env.get('TODOER_PUSH_VAPID_SUBJECT', 'mailto:' + email.split(',')[0].strip()),
    'TODOER_DEV_AUTH': '0',
    'TODOER_RUST_DB': str(state / 'google-auth.db'),
    'TODOER_RUST_PORT': '8094',
    'TODOER_RUST_BIND': '127.0.0.1',
    'TODOER_PUBLIC_URL': origin,
    'TODOER_GOOGLE_CLIENT_ID': credentials['client_id'],
    'TODOER_GOOGLE_CLIENT_SECRET': credentials['client_secret'],
    'TODOER_SERVER_KEY': key_path.read_text().strip(),
    'TODOER_REGISTRATION_ALLOWLIST': email,
})
os.execve(root / 'target/debug/todoer-server', ['todoer-server'], env)
