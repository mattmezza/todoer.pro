#!/usr/bin/env python3
"""Run real Google authentication against a separate local database."""
import json
import os
from pathlib import Path
import secrets

root = Path(__file__).resolve().parent.parent
os.chdir(root)
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
# Use only deliberate configuration, rather than inherited development adapters/workers.
env = {key: value for key, value in os.environ.items() if not key.startswith('TODOER_')}
env.update({key:value for key,value in os.environ.items() if key.startswith('TODOER_SMTP_') or key in ('TODOER_AUTH_ADMIN_EMAILS','TODOER_GOOGLE_LOGIN_ENABLED','TODOER_SIGNUP_ENABLED')})
env.update({
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
