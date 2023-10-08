td
===

My personal TODO app.

## Dev

- `make start` starts a development server of fastapi
- `make tw` compiles tailwindcss and starts watching for edits

#### Stack

- `htmx`
- `FastAPI`
- `TailwindCSS`
- `SQLite`

## Deploy

- `local`
- `dev`
- `prod`


### Scripts

- `build.sh`: builds a docker image tagged with the git short hash
- `deploy.sh`: deploys a stack via docker-compose
- `logs.sh`: attaches to the logs of a running stack
- `migrate.sh`: utility to migrate the DB up or down
