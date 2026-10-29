# Local attachment storage

Start the private S3-compatible MinIO service and initialize its buckets:

```sh
docker compose -f deploy/local/compose.yml up -d
source deploy/local/attachments.env
```

The environment file contains deliberately public **local-only** credentials.
Start the app from that same shell so it inherits the six storage settings.
For development authentication with the existing local database:

```sh
TODOER_DEV_AUTH=1 \
TODOER_DEV_TOKEN=local-integration-token-not-for-production \
TODOER_RUST_DB=.local/rebuild/rust.db \
TODOER_RUST_PORT=8092 target/debug/todoer-server
```

MinIO console: http://127.0.0.1:9001. User `synthetic-test`, password
`synthetic-test-secret`. Both console and S3 ports bind only to loopback.
Application uploads use `todoer-attachments/local-dev`; automated fixtures use
a separate `todoer-synthetic-attachments` bucket. Data persists in a Docker volume.

In the app, open a task's full page, select a file in Files, upload, reload,
and download it. Check that the downloaded bytes match the original. Delete the
attachment and confirm it disappears after reload (metadata deletion retains bytes).
To test an outage, stop storage, try a new upload, and confirm the error retains
its retry state; restart storage and retry the same file:

```sh
docker compose -f deploy/local/compose.yml stop storage
docker compose -f deploy/local/compose.yml start storage
```

Run the isolated HTTP/storage integration tests:

```sh
TODOER_TEST_S3_ENDPOINT=http://127.0.0.1:9000 \
TODOER_TEST_S3_BUCKET=todoer-synthetic-attachments \
CARGO_INCREMENTAL=0 cargo test -p todoer --lib local_s3_ -- --ignored --nocapture
```

Verified against this MinIO setup: both tests passed, including signed and proxy
uploads, 200 KiB HTTP uploads, checksums, immutable finalization/retries, private
anonymous denial, downloads and access revocation. These tests use in-memory
application databases and do not add tasks to the development database.

Stop containers while keeping uploads: `docker compose -f deploy/local/compose.yml down`.

## Real Google login behind local nginx

This uses a separate `.local/rebuild/google-auth.db`, port 8094 internally and
https://localhost:8443 externally. It does not enable the development login or
seed development identities. The Google Web client JSON must be saved at
`~/.config/todoer/google-local.json`, with the exact authorized redirect
`https://localhost:8443/auth/google/callback`.

Create the local certificate (first setup only):

```sh
mkdir -p .local/rebuild/auth-tls
openssl req -x509 -newkey rsa:2048 -nodes -days 30 \
  -keyout .local/rebuild/auth-tls/localhost.key \
  -out .local/rebuild/auth-tls/localhost.crt \
  -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1'
chmod 600 .local/rebuild/auth-tls/localhost.key
```

Build the current app and start MinIO (plus local Mailpit):

```sh
make assets
cargo build --locked --bin todoer-server
docker compose -f deploy/local/compose.yml up -d
```

Start the app in one terminal and nginx in another:

```sh
TODOER_REGISTRATION_ALLOWLIST=your-google-test-email@example.com \
  python3 scripts/local-google-auth.py --with-local-storage
```

```sh
docker compose -f deploy/local/auth-compose.yml up -d
```

The compose proxy uses host networking for Linux local development, with nginx
listening only on loopback. The self-signed certificate requires a local browser
exception or explicit trust. Use a trusted local certificate before testing
WebAuthn; do not copy local TLS keys or Google credentials into source control.
The launcher reads credentials directly into the child process environment and
uses an untracked, persistent server key. Other TODOER settings, including
development authentication and notification workers, are deliberately
not inherited. `--with-local-storage` explicitly selects the bundled loopback
MinIO with a separate `local-google-auth` attachment prefix; it does not inherit
production storage credentials. MinIO is at http://127.0.0.1:9001 with the local-only
credentials listed above. Stop the authentication app with Ctrl+C and the proxy with
`docker compose -f deploy/local/auth-compose.yml down`.

Manual acceptance: sign in with your allowlisted Google test user, verify the new
account has its own Inbox/Personal space, sign out, and sign in again to check the
same account is reused. Finally, sign out and verify a private task URL redirects
to sign-in. Do not expect the seeded port-8092 workspace in this separate database.

## Password accounts, MFA and captured account email

Start Mailpit (messages stay local):

```sh
docker compose -f deploy/local/compose.yml up -d mail
```

Launch the real-auth instance with these additional settings:

```sh
TODOER_AUTH_ADMIN_EMAILS=your-google-email@example.com \
TODOER_REGISTRATION_ALLOWLIST=your-google-email@example.com,local-test@example.invalid \
TODOER_SMTP_HOST=127.0.0.1 TODOER_SMTP_PORT=1025 \
TODOER_SMTP_INSECURE_LOCAL=1 TODOER_SMTP_FROM='Todoer <test@todoer.invalid>' \
python3 scripts/local-google-auth.py
```

Sign in with Google again, then open https://localhost:8443/settings/security.
Add a password to the existing Google account, or sign up separately with
`local-test@example.invalid`. Verification and reset messages are captured at
http://127.0.0.1:8025, not delivered to a real inbox. Test TOTP or passkey enrollment
from Account security and save recovery codes before signing in again. Test signup
and Google switches using the administrator's Deployment authentication section.

The initial values can be provided with `TODOER_GOOGLE_LOGIN_ENABLED` and
`TODOER_SIGNUP_ENABLED`; saved settings take precedence on subsequent restarts.
The primary port-8092 development bypass does not exercise these authentication
features. Existing port-8092 tasks are not copied into the authentication test instance.

For automated acceptance with a separate temporary database:

```sh
docker compose -f deploy/local/auth-compose.yml --profile tests up -d test-proxy
node scripts/test-account-security.mjs
```

The test server uses loopback port 8095, HTTPS proxy port 8445 and Mailpit. Stop the
optional test proxy afterward if desired:

```sh
docker compose -f deploy/local/auth-compose.yml --profile tests stop test-proxy
```
