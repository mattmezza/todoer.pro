# Identity, permissions and agent authorization

The Rust application uses its own database; it never authenticates against or
modifies the Python database. This rebuild still has substantial unfinished
product scope; implementing authentication does not authorize a cutover.

## Browser sign-in

With `TODOER_DEV_AUTH` unset, startup requires an HTTPS `TODOER_PUBLIC_URL`, an
independent `TODOER_RUST_DB`, and a stable 64-hex-character `TODOER_SERVER_KEY`.
Google web-client credentials are required when Google login is enabled. Register the exact
`{TODOER_PUBLIC_URL}/auth/google/callback` redirect with Google. Pass configuration
through deployment environment/files, not command-line secrets or tracked files.
See `app/environment.example`. The existing reverse proxy terminates HTTPS;
listen on loopback unless container networking requires another bind address.

Google OpenID Connect discovery and authorization-code exchange use
`openidconnect` 4.0.1: PKCE S256, browser-bound single-use state, nonce, signature,
issuer, audience, expiry and access-token hash validation. Signing-key failure
triggers one bounded rediscovery and verification of the same token; authorization
codes are never retried. Login state expires after ten minutes. The initiating
cookie is HttpOnly/Secure/SameSite=Lax, allowing Google's cross-site callback.
Only a verified email on the exact registration allowlist may create an account.
Accounts use issuer+subject, never an email match to join an existing identity.
Existing identities survive email changes. Sessions are independent opaque
credentials, hashed in SQLite, expire after twelve hours, and are revoked at
logout. Browser writes require their session-specific CSRF token.

Local development requires **explicit** `TODOER_DEV_AUTH=1` and a token of at least
24 characters. It binds loopback, seeds synthetic identities, and disables Google
and OAuth credential issuance. This development alternative is not a production
login option. It does not access an old account or database.

## Permission matrix

| Operation | Workspace owner | Editor | Viewer | Direct list recipient |
|---|---|---|---|---|
| Read inherited lists | Yes | Yes | Yes | Granted list only |
| Edit tasks/discussion | Yes | Yes | No | Editor grant only |
| Read restricted list | Yes | Explicit grant | Explicit grant | Explicit grant |
| Administer workspace/sharing | Yes | No | No | No |
| Reorder workspace lists | Yes | No | No | No |
| Reorder sections | Yes | Yes, if list accessible | No | Editor grant only |

Direct grants can name an existing user outside the workspace; they reveal the
list, not workspace enumeration or unrelated lists. Owners always retain access.
Removing the final owner is rejected. Audience or role broadening requires an
explicit acknowledgement, checked inside the same transaction as the change.
Email invitation onboarding is not implemented yet; the current sharing form
requires an existing stable user ID.

## Agent credentials and MCP

Settings creates opaque credentials once, separately for REST (`/api/v1`) or MCP
(`/mcp`). Store only SHA-256 digests. Read/write/admin ceilings intersect live
membership and optional selected-list bounds on every operation. A credential
cannot manufacture another credential by calling the browser-only issuer.
Revocation is persistent and applies to existing MCP sessions and SSE requests.
Manual REST and MCP credentials can optionally be created without an expiry;
they remain active until revoked. In the credential-creation API, set `ttl_secs`
to `0` for no expiry. Responses show `expires_at: null` for these credentials;
omitting `ttl_secs` keeps the 30-day default.

MCP publishes protected-resource and authorization-server metadata and a 401
`WWW-Authenticate` metadata challenge. Public clients use dynamic registration,
PKCE S256, exact registered redirects and resource binding. The browser explicitly
approves read/write scope and selected lists (or all current/future accessible
lists). Access tokens expire after fifteen minutes; refresh tokens rotate, and
reuse revokes the family. No Google/provider access token is accepted as a Todoer
credential. Current supported scopes are `tasks:read` and `tasks:write`.

The official Python MCP 2.3 SDK has exercised discovery, registration, consent,
PKCE exchange, refresh, tool calls, capture retry and selected-list denial over a
real loopback HTTP listener. This is not a claim that a hosted ChatGPT/Claude
connector has been tested. Real Google tenant login remains unverified without
owner credentials; signed ephemeral-provider HTTP tests exercise validation and
key rotation locally. Public registration currently has a global 1,000-client
cap; operational rate limiting and invitation onboarding remain on the ledger.

Run the isolated OAuth SDK integration with:

```sh
cargo test --locked --lib official_sdk_discovers_oauth -- --ignored --nocapture
```

The fixture supplies a synthetic browser session to the test script; the script
refuses non-loopback servers. It does not bypass Google on a production server.

## Sources checked

- https://docs.rs/openidconnect/4.0.1/openidconnect/
- https://developers.google.com/identity/openid-connect/openid-connect
- https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- https://www.rfc-editor.org/rfc/rfc7636
- https://www.rfc-editor.org/rfc/rfc8707
- https://www.rfc-editor.org/rfc/rfc9700

## Sign-in presentation and Google profile

`/signin` and `/signup` use the same centered authentication card and a Google
logo button. Both delegate to the verified Google flow, preserve a validated local
return path and enforce the registration allowlist. The development sign-in stays
explicitly separate. Email/password, email verification/reset, TOTP with recovery codes and
WebAuthn/passkeys are available from sign-in and Account security settings.

On Google login, the verified display name replaces only the initial `Todoer user`
placeholder; a custom name is retained. The provider photo is stored and Google
image hosts are allowed by the page CSP. Avatar failure, including an error before
JavaScript attaches its listener, falls back to initials. Outside a space, sidebar
email labels reveal two leading and two trailing local-part characters, separated
by `***`; very short local parts reveal fewer to avoid overlapping characters.


## Local accounts and deployment policy

- `TODOER_GOOGLE_LOGIN_ENABLED=true|false` and `TODOER_SIGNUP_ENABLED=true|false`
  initialize the singleton authentication policy only when it does not exist.
  Both default to true, preserving the existing allowlist-gated Google behavior.
  A saved setting wins over later environment changes, including after restart.
- `TODOER_AUTH_ADMIN_EMAILS` grants deployment administrator access to matching
  verified identities when they sign in or complete email verification. It is
  separate from workspace ownership. Agent/API credentials cannot manage policy.
- Administrators use **Settings → Account security → Deployment authentication**.
  Changes require a recent authenticated browser session and CSRF; revision checks
  reject stale saves. Policy changes are audited without credentials or tokens.
- Disabling Google hides its button and denies its start/callback endpoints.
  Google credentials may be omitted entirely on a fresh email-only deployment
  initialized with Google disabled. Re-enabling Google requires configured client
  credentials. An administrator must have a password or passkey before disabling
  Google, preventing accidental loss of their only sign-in method.
- Closing signup blocks both email registration/verification and creation of new
  Google accounts. Existing users can still sign in, recover their password and
  add sign-in methods. The registration allowlist remains an independent gate;
  enabling signup does not make registration open to arbitrary email addresses.

Passwords use Argon2id with independent salts and accept 15–128 characters.
Email signup sends a single-use verification link expiring after one hour; the
account and its initial workspace are created transactionally after verification.
Pending passwords are stored only as hashes. Google accounts can add a password
from Account security after a recent Google sign-in; this retains the same user
and task data. Accounts are never silently joined by matching email addresses.

Forgot-password responses are generic and reset tokens are hashed, single-use,
valid for 30 minutes and tied to the account security epoch and current verified
email. Email links carry tokens in fragments, which the browser removes from
history before submitting them in a JSON request. Password resets retain MFA and
require an enrolled authenticator/recovery code or a verified passkey when MFA is
active. Password changes/resets revoke all sessions and invalidate outstanding
security proofs. Session issuance checks the epoch in the same write transaction
as issuing the new session. Recent-session checks are repeated after queued work.

## MFA and WebAuthn

Account security can enroll a TOTP authenticator, add/remove named passkeys, and
rotate recovery codes. Sensitive changes require a full sign-in within five
minutes. Enrollment revokes other sessions and displays ten single-use recovery
codes once; store them before signing in again. TOTP seeds are encrypted with
ChaCha20-Poly1305 under a domain-separated deployment key and owner-bound associated
data. Accepted time steps cannot be reused. Recovery codes are stored as hashes.
Back up the stable server key: changing it prevents decrypting existing TOTP seeds.

WebAuthn uses webauthn-rs, validates the configured public origin/RP ID and requires
user verification for passkeys. Challenge state is bounded, single-use, expires
after five minutes, and stays on the server, bound to the initiating browser or
session and the account security epoch. Credential IDs are globally unique; stored
counters and backup flags are updated after verification. Password and Google
sign-in require a second factor when any MFA method is enrolled; a user-verified
passkey can sign in directly. Credentials remain tied to their RP hostname, so
localhost test credentials will not work for the production hostname.

The current challenge store is process-local. A restart interrupts incomplete
ceremonies; retrying starts a fresh challenge. Persisted accounts, credentials,
policy and sessions survive restart. SQLite deployments should run one app process
for these flows (or require sticky routing before introducing multiple workers).

## Verification

`cargo test -p todoer --lib` covers policy authorization/revisions/lockout, verified
signup, token expiry/reuse, password hashing, encrypted TOTP seeds, code replay,
MFA-preserving reset, session revocation and existing application behavior.

`node scripts/test-account-security.mjs` starts a separate temporary database and
email-only backend at port 8095, accessed through the optional nginx test proxy on
8445. It requires local Mailpit and the test proxy; it never modifies the existing
development or Google-test database. Chromium exercises real forms and SMTP links,
TOTP/recovery, a CTAP2 virtual authenticator, WebAuthn login/reset, rejected replays,
CSRF/origin/admin boundaries and policy persistence after restart. This checks
browser WebAuthn protocol support; physical authenticators still need manual
acceptance on the user's device.

## Sign-in return paths and security freshness

Unauthenticated HTML page navigation redirects to `/signin?next=…`, preserving
its local path and query. APIs retain JSON 401 responses. Security mutations
require authentication within five minutes; an expired proof returns the
`reauthentication_required` error and opens a sign-in confirmation dialog.
Signing in returns to the selected security section so the user can retry the
action. Passwords and other sensitive form values are not persisted for replay.
New email-based passkey sign-ins use their own validated return path rather than
inheriting a prior MFA attempt. Failed MFA codes keep the five-minute pending flow
available for a throttled retry; successful verification consumes it once.
Displayed recovery codes can be downloaded locally as a plain text file.

Password changes use a dedicated modal with matching new-password fields.
Accounts with an existing password must also provide their current password;
Google/passkey-only accounts can set their first password after a fresh sign-in.
The server checks confirmation, verifies the current password using the bounded
password-worker pool, and rechecks credentials before committing the change.
Save failures release the modal controls; closing clears the password fields.
