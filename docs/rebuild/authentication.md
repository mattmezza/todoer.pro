# Identity, permissions and agent authorization

The Rust application uses its own database; it never authenticates against or
modifies the Python database. This rebuild still has substantial unfinished
product scope; implementing authentication does not authorize a cutover.

## Browser sign-in

With `TODOER_DEV_AUTH` unset, startup requires an HTTPS `TODOER_PUBLIC_URL`, an
independent `TODOER_RUST_DB`, and Google web-client credentials. Register the exact
`{TODOER_PUBLIC_URL}/auth/google/callback` redirect with Google. Pass configuration
through deployment environment/files, not command-line secrets or tracked files.
See `rebuild/environment.example`. The existing reverse proxy terminates HTTPS;
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
