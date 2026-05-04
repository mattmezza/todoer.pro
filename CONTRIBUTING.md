# Contributing to Todoer Pro

Thanks for your interest in contributing! This guide will help you get started.

## Development setup

**Prerequisites:** Python 3.13+, Node.js, [uv](https://docs.astral.sh/uv/)

```bash
git clone https://github.com/mattmezza/todoer.pro.git
cd todoer.pro
uv sync
npm install
cp .env.example .envrc
```

Edit `.envrc` and set your `TODOER_SECRET_KEY` (generate one with `openssl rand -hex 32`).

Then, in two terminals:

```bash
# Terminal 1 — app server
export TODOER_PORT=8000
make dev

# Terminal 2 — CSS watcher
make cssd
```

The app will be at `http://localhost:8000`.

## Project structure

See the [README](README.md#project-structure) for a full overview.

Key directories:
- `app/routers/` — route handlers
- `app/repo/` — data access (raw SQL)
- `app/svc/` — business logic
- `components/` — JinjaX templates
- `migrations/` — SQL migration files

## Making changes

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Format your code: `uv run black . && uv run isort .`
4. Test locally — run the dev server and verify in the browser
5. Open a pull request

## Code style

- **Python:** `black` (79 char line length, skip string normalization) + `isort`
- **Templates:** JinjaX components in PascalCase with dot notation
- **CSS:** Tailwind utility classes, dark mode via `dark:` prefix
- **SQL:** raw queries in the repo layer, no ORM

## Database migrations

Migrations are numbered SQL files in `migrations/`:
- `NNN.up.sql` — forward migration
- `NNN.down.sql` — rollback

To add a migration, create the next numbered file pair and test both directions.

## Reporting bugs

Open an issue with:
- Steps to reproduce
- Expected vs actual behavior
- Browser and OS info (for UI bugs)

## Suggesting features

Open an issue describing the feature and why it's useful. Discussion before implementation is encouraged.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be respectful.
