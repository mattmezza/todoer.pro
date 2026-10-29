# Todoer Pro

Task management for a clearer head, with a Rust web app, REST API, MCP server
and CLI. Original project code and documentation are licensed under [MIT](LICENSE).

| Directory | Purpose | Production host |
| --- | --- | --- |
| [`app/`](app/) | Rust/Axum app, Askama templates, CLI, migrations | app.todoer.pro |
| [`www/`](www/) | Static public website | todoer.pro · Cloudflare Pages |
| [`docs/`](docs/) | Public documentation website and Markdown sources | docs.todoer.pro · GitHub Pages |
| [`deploy/`](deploy/) | Docker, Compose, nginx examples and migration runbook | VPS |
| [`legacy/`](legacy/) | Python app and supporting files retained as reference | Not hosted |
| [`extension/`](extension/) | Browser extension | Separate client |
| [`skills/`](skills/) | Agent integration resources | Source only |

## Develop

Requires Rust 1.99+, Node.js 24, npm, a C compiler and platform build tooling
(Linux: pkg-config and OpenSSL development libraries). Python 3.11+ builds docs
and runs administrative scripts. Cargo and npm dependencies are locked.

```sh
make setup
export TODOER_DEV_AUTH=1
export TODOER_DEV_TOKEN=synthetic-local-token-at-least-24-characters
make dev
```

Open http://127.0.0.1:8092. Synthetic development mode is explicitly enabled and
loopback only; it never uses the Python database. Default data lives under
`.local/rebuild/` (retained for compatibility with existing local checkouts).
See [`app/.env.example`](app/.env.example) and
[`app/environment.example`](app/environment.example) for configuration.

```sh
make check             # formatting, Clippy, tests, debug build
make e2e               # browser tests against an already-running synthetic app
make build             # release binaries
make www-build         # stage only public website files under www/_site
python3 -m venv .local/docs-venv
.local/docs-venv/bin/pip install -r docs/requirements.txt
.local/docs-venv/bin/python docs/build.py
```

Run CLI commands with `cargo run --bin todoer -- --help`. CLI authentication,
REST and MCP setup are documented at [docs.todoer.pro](https://docs.todoer.pro).

## Explicit releases

Releases require a clean local `main` equal to current `origin/main`, `gh` login,
and a new stable semantic version. These commands create a GitHub release;
GitHub Actions performs the builds. They do not deploy to the VPS.

```sh
make release name=v1.0.0       # publishes v1.0.0; builds ghcr.io/mattmezza/todoer.pro:v1.0.0
make cli-release name=v1.0.0   # publishes cli-v1.0.0; attaches platform executables
```

Tag pushes, normal commits and draft releases publish nothing. There is no
mutable `latest` image: deploy an explicit version or digest. App image currently
supports Linux amd64. CLI downloads cover Linux amd64/arm64, macOS Intel/Apple
Silicon and Windows amd64, each with a SHA-256 sidecar. Linux CLI binaries require
glibc (amd64 ≥2.35, arm64 ≥2.39); they are not Alpine/musl executables. Download
the matching `todoer-cli-vX.Y.Z-TARGET` asset, verify its checksum and `chmod +x`
on Unix. `.exe` is directly executable on Windows. CLI tags don't replace the
app release marked as latest in GitHub.

## Hosting setup

- **App:** shallow clone `main` on the VPS and follow [deploy/README.md](deploy/README.md).
  Existing host nginx terminates HTTPS. Docker binds loopback only. Compose needs
  no `--wait` support. Initial database import is a separate, reviewed operation:
  [migration runbook](deploy/MIGRATION.md). No existing todos are migrated by CI.
- **Docs:** set repository Pages source to **GitHub Actions** and custom domain to
  `docs.todoer.pro`. Set DNS CNAME to `mattmezza.github.io` and enforce HTTPS after
  domain verification. The `docs.yml` workflow publishes only when `docs/**`
  changes on `main`; unrelated main pushes run nothing.
- **Public website:** create a **Direct Upload** Cloudflare Pages project named
  `todoer-www` (production branch `main`). Add its custom domain `todoer.pro` in
  Cloudflare and follow its DNS instructions. Disable Git auto-deploy if using an
  existing Git-connected project. Configure environment `cloudflare-pages` with
  secrets `CLOUDFLARE_API_TOKEN` (scoped to Pages editing) and
  `CLOUDFLARE_ACCOUNT_ID`; optionally set variable `CLOUDFLARE_PAGES_PROJECT`.
  Publish explicitly with `make www-publish` or `make www-deploy`, or run the
  workflow manually. Those commands dispatch `main` on GitHub, not local files.
- **Registry:** after the first app release, set GHCR package visibility to public
  separately, or authenticate VPS pulls with a read:packages token.

PR checks run on hosted runners with read-only permissions and synthetic data,
including fork PRs. No workflow runs on ordinary main pushes except docs changes;
no workflow automatically deploys the VPS. Release jobs require a published
GitHub release and verify that its source belongs to main.

## Data and licensing

Never commit production environment files, databases, exports, backups, identity
maps or credentials. Keep verified SQLite backups, attachment storage and the
stable server key together in your recovery plan. Snapshot before every schema
upgrade. See [recovery docs](docs/rebuild/local-recovery.md).

The MIT license covers original code, docs, website and reference implementation.
Bundled third-party fonts and libraries retain their licenses and notices;
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
