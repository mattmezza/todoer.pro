<div align="center">
  <img src="public/icon512_maskable.png" alt="Todoer Pro" width="120" height="120" />
  <h1>Todoer Pro</h1>
  <p><strong>Get things done, like a pro.</strong></p>
  <p>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
    <a href="https://www.python.org/downloads/"><img src="https://img.shields.io/badge/python-3.13%2B-blue" alt="Python 3.13+" /></a>
    <a href="https://github.com/mattmezza/todoer.pro/releases"><img src="https://img.shields.io/github/v/release/mattmezza/todoer.pro" alt="Latest Release" /></a>
    <a href="https://github.com/mattmezza/todoer.pro/actions/workflows/docker-publish.yml"><img src="https://github.com/mattmezza/todoer.pro/actions/workflows/docker-publish.yml/badge.svg" alt="Docker Build & Push" /></a>
    <a href="https://github.com/mattmezza/todoer.pro/blob/main/CONTRIBUTING.md"><img src="https://img.shields.io/badge/contributions-welcome-brightgreen.svg" alt="Contributions Welcome" /></a>
  </p>
</div>

---

**Todoer Pro** is a fast, modern task management app built with **FastAPI**, **HTMX**, **Alpine.js**, **Tailwind CSS**, and **SQLite**. It features keyboard-driven navigation, real-time updates via HTMX, drag-and-drop reordering, AI-assisted task generation, and a full browser extension for quick capture.

> 🚀 **Live demo:** [todoer.pro](https://todoer.pro)

---

## ✨ Features

- **Keyboard-first** — Vim-style navigation (`j`/`k`, `G`, `gg`, `{`/`}`), actions (`x`, `e`, `d`, `s`), quick add (`n`), search (`Cmd+K`), shortcuts dialog (`?`)
- **Spaces & Lists** — Organize tasks into spaces, each with multiple lists and sections
- **Drag-and-drop** — Reorder tasks, sections, and lists with SortableJS
- **Rich task editing** — Subtasks, comments, tags, priorities, due dates, bulk actions
- **Inline editing** — Click any title or description to edit in place with hyperscript
- **AI generation** — Generate task suggestions from natural language descriptions
- **2FA** — Time-based one-time passwords (TOTP) with backup codes
- **Notifications** — Real-time notification panel with read tracking
- **Browser extension** — Chrome extension for quick task capture from any page
- **Dark mode** — Full dark theme via Tailwind's `dark:` prefix
- **Mobile-responsive** — Works on desktop and mobile browsers
- **Self-hostable** — Single Docker image, SQLite-backed, deploy anywhere

---

## 🖥 Tech Stack

| Layer            | Technology                                                   |
| ---------------- | ------------------------------------------------------------ |
| Backend          | [FastAPI](https://fastapi.tiangolo.com/) + Python 3.13+      |
| Frontend         | [HTMX](https://htmx.org/) + [Alpine.js](https://alpinejs.dev/) + [Hyperscript](http://hyperscript.org/) |
| Templating       | [JinjaX](https://jinjax.scaletti.dev/) component system      |
| CSS              | [Tailwind CSS v4](https://tailwindcss.com/)                  |
| Database         | SQLite via [`databases`](https://www.encode.io/databases/)   |
| Auth             | JWT (python-jose) + bcrypt passwords + TOTP 2FA              |
| Storage          | S3-compatible (presigned URLs for attachments)               |
| Drag & Drop      | [SortableJS](https://sortablejs.github.io/Sortable/)         |
| CI/CD            | GitHub Actions → GHCR (Docker image)                         |

---

## 🚀 Quick Start (Development)

**Prerequisites:** Python 3.13+, Node.js 20+, [uv](https://docs.astral.sh/uv/)

```bash
# Clone the repo
git clone https://github.com/mattmezza/todoer.pro.git
cd todoer.pro

# Install Python dependencies
uv sync

# Install Node dependencies (Tailwind CLI)
npm install

# Configure environment
cp .env.example .envrc
# Edit .envrc — set TODOER_SECRET_KEY (generate with: openssl rand -hex 32)
```

Then, in **two separate terminals**:

```bash
# Terminal 1 — Start the app server
export TODOER_PORT=8000
make dev

# Terminal 2 — Compile Tailwind CSS (watch mode)
make cssd
```

Open [http://localhost:8000](http://localhost:8000), sign up, and start managing tasks.

---

## 🐳 Docker (Production / Self-Hosting)

### Pre-built images

Pre-built images are available on [GitHub Container Registry](https://github.com/mattmezza/todoer.pro/pkgs/container/todoer.pro):

```bash
docker pull ghcr.io/mattmezza/todoer.pro:latest
```

### Run with Docker

```bash
docker run -d \
  --name todoer \
  -p 80:80 \
  -e TODOER_SECRET_KEY="$(openssl rand -hex 32)" \
  -e TODOER_ENV=prod \
  -e TODOER_DB_URL="sqlite:////var/todoer.db" \
  -v todoer_data:/var/todoer.db \
  ghcr.io/mattmezza/todoer.pro:latest
```

### Run with Docker Compose

See `deploy/docker-compose.prod.yml` for a production-ready compose file:

```bash
# Set required env vars (see deploy/secrets.env.example)
export TAG=latest

# Start the stack
docker compose -f deploy/docker-compose.prod.yml up -d
```

### Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `TODOER_SECRET_KEY` | ✅ | — | JWT signing key (generate with `openssl rand -hex 32`) |
| `TODOER_ENV` | ❌ | `local` | `local`, `dev`, or `prod` |
| `TODOER_DB_URL` | ❌ | `sqlite:////var/todoer.db` | Database connection string |
| `TODOER_ACCESS_TOKEN_EXPIRE_MINUTES` | ❌ | `43200` | JWT access token lifetime |
| `TODOER_AUTH_COOKIE_EXPIRE_MINUTES` | ❌ | `43200` | Auth cookie lifetime |
| `TODOER_STORAGE_*` | ❌ | — | S3 storage config for attachments |

---

## 📚 Project Structure

```
├── app/                # FastAPI application
│   ├── main.py         # Entry point, router mounts
│   ├── routers/        # Route handlers (decorated with @tpl.htmx)
│   ├── repo/           # Data access layer (raw SQL)
│   ├── svc/            # Business logic
│   ├── deps.py         # FastAPI dependencies
│   ├── security.py     # JWT auth, password hashing, 2FA
│   ├── tpl.py          # Template rendering, JinjaX catalog
│   ├── blob.py         # S3-compatible blob storage
│   └── http.py         # Opt monad for nullable results
├── components/         # JinjaX components (.jinja)
├── styles/             # Tailwind CSS source
├── migrations/         # SQL migration files (NNN.up.sql / NNN.down.sql)
├── scripts/            # Build, deploy, migrate shell scripts
├── deploy/             # Docker Compose files per environment
├── extension/          # Chrome browser extension
├── .github/workflows/  # CI/CD (Docker build & push to GHCR)
└── public/             # Static assets
```

---

## 🧰 Makefile Targets

| Target | Description |
|---|---|
| `make dev` | Start dev server on `$TODOER_PORT` with hot-reload |
| `make cssd` | Compile Tailwind CSS and watch for changes |
| `make css` | Compile and minify Tailwind CSS |
| `make release name=v1.0` | Build Docker image, push to GHCR, create GitHub release |
| `make build env=local name=v0.20` | Build Docker image for an environment |
| `make deploy env=prod name=v0.20` | Build and deploy via Docker Compose |

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Development setup guide
- Code style (Black + isort)
- Database migration workflow
- Pull request process

Please also read our [Code of Conduct](CODE_OF_CONDUCT.md).

---

## 🛡 Security

Report vulnerabilities privately to **mattmezza@gmail.com**. See [SECURITY.md](SECURITY.md) for details.

---

## 📄 License

Todoer Pro is open source under the [MIT License](LICENSE).

Copyright (c) 2024 Matteo Merola
