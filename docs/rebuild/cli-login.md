# CLI browser login

The standalone Rust CLI uses REST; it does not open the application database.

    todoer --url https://todoer.example login --permission write
    todoer auth-status
    todoer --json list
    todoer logout

TODOER_URL selects the server. TODOER_TOKEN overrides a saved login.
Use --no-browser to open the printed authorization URL yourself. Login waits
300 seconds by default; --timeout accepts 10–600 seconds.

The CLI requests write access by default; pass `--permission read` or
`--permission admin` to change the maximum offered at approval. The browser
approval chooses read, write or admin access within that maximum and specific spaces
and/or lists. Space access includes future lists. Individual list access does
not expose its entire space. Current membership and permissions always clamp
the credential's access. Scoped credentials cannot administer account settings.
All resources requires explicit approval.

Login uses a loopback callback, state and S256 PKCE. Requests expire after ten
minutes; approval codes expire after two minutes and are single use. Tokens expire after 30 days by default. You can instead choose “Never expires”
at approval and revoke access later in Settings → Connected tools. The
“Never expires” option requires a CLI version that supports permanent logins.
Run login
again to renew an expired token. HTTPS is required except for
local development. This handshake is separate from MCP OAuth; CLI credentials
have the REST audience and cannot authenticate to MCP.

Credentials are stored per server in credentials.json under TODOER_CONFIG_DIR,
otherwise XDG_CONFIG_HOME/todoer, otherwise ~/.config/todoer. Unix permissions
are 0700 for the directory and 0600 for the file; writes are atomic. The server
stores only a token hash. Command output never prints the issued token.

Logout revokes the saved credential before removing it locally. On a server
failure it retains the local credential so revocation can be retried. It does
not unset an environment token. Credentials can also be revoked under Settings
→ Connected tools. Auth-status reports local metadata, not a live validity check.

Endpoints: POST /api/cli/login/start, browser GET/POST /cli/authorize,
POST /api/cli/login/token and POST /api/cli/login/revoke.
