todoer.pro
===

My personal TODO app.

## Stack

- `htmx`
- `FastAPI`
- `TailwindCSS`
- `SQLite`

## Makefile targets

- `dev` starts a development server of fastapi
- `cssd` compiles tailwindcss and starts watching for edits
- `css` compiles tailwindcss and minifies the CSS
- `release` uses `gh` to create a release (`make release VERSION=v0.20`)
- `build` uses `build.sh` script to build a release for an environment
- `deploy` uses `deploy.sh` script to deploy a release for an environment

## Scripts

- `build.sh`: builds a docker image tagged with the git short hash
- `deploy.sh`: deploys a stack via docker-compose
- `logs.sh`: attaches to the logs of a running stack
- `migrate.sh`: utility to migrate the DB up or down

## Environments

- `local` the one you run on your machine to develop (http://localhost:8000)
- `dev` the one you run remotely to test the release (https://dev.todoer.pro)
- `prod` the prod one (https://todoer.pro)
