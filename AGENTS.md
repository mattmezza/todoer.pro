# Todoer Pro

FastAPI + Jinja2/JinjaX app with HTMX frontend, SQLite DB, Tailwind CSS.

## Setup

```bash
uv sync                # install deps
cp .env.example .envrc # configure env vars
make serve-reload      # run dev server (default :8000, override: port=N)
make tailwind-watch    # run tailwind in watch mode (separate terminal)
```

## Project structure

- `app/` - FastAPI application
  - `main.py` - app entrypoint, mounts routers
  - `routers/` - route handlers (return HTML responses)
  - `repo/` - data access layer (raw SQL via `databases` lib)
  - `svc/` - service/business logic
  - `deps.py` - FastAPI dependencies
  - `security.py` - auth (JWT via python-jose, passwords via passlib)
  - `tpl.py` - template rendering helpers
- `components/` - JinjaX components (`.jinja` files)
- `styles/` - Tailwind CSS source
- `migrations/` - numbered SQL migrations (`NNN.up.sql` / `NNN.down.sql`)
- `scripts/` - build, deploy, migrate shell scripts
- `deploy/` - docker-compose files per environment

## Conventions

- Formatting: `black` (line-length 79, skip-string-normalization) + `isort`
- Env vars prefixed with `TODOER_`
- Repo layer uses raw SQL strings, not an ORM
- Components are PascalCase JinjaX (e.g. `<Section />`)
- Migrations are sequential numbered files; some are `.py` for data migrations
