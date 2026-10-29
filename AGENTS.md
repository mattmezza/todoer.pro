# Todoer Pro monorepo

- `app/`: current Rust/Axum/Askama application, REST API, MCP, CLI and migrations.
- `www/`: static public website for todoer.pro (Cloudflare Pages).
- `docs/`: public documentation for docs.todoer.pro (GitHub Pages).
- `deploy/`: production Docker image, Compose, nginx examples and runbooks.
- `legacy/`: unhosted Python/FastAPI reference; its AGENTS.md applies there.
- Root Cargo workspace and npm tooling support the active application.

Run `make setup`, `make check`, and `make test`. Browser tests use synthetic data;
never point development checks at a real database. `make dev` requires explicit
TODOER_DEV_AUTH=1 and a synthetic TODOER_DEV_TOKEN. Production auth must stay on.
Rust formatting: cargo fmt; frontend: npm run format. Keep dependencies locked.
Before modifying anything, judge whether it adds or removes complexity.
Commit cohesive changes frequently. Keep secrets, databases, exports, identity
maps, private keys and backups out of Git, Docker contexts and public artifacts.
Do not modify legacy data during a cutover. Import from an independent backup,
review a dry-run, map real accounts explicitly, and verify counts and attachments.

CI runs on pull requests only. Docs additionally publish on docs-only changes to
main; www publishing is manual. Docker and CLI publishing require published
GitHub releases (vX.Y.Z and cli-vX.Y.Z); tag pushes alone never publish.
Do not add push-to-main checks or automatic production deployment.
All original project code and documentation are MIT; preserve third-party notices.
