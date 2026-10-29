.DEFAULT_GOAL := help
.PHONY: help setup assets dev format check test build e2e release cli-release www-build www-dev www-publish www-deploy docs-build deploy backup restore migrate image
help: ## List commands
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*##/ {printf "  %-20s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
setup: ## Install locked frontend dependencies and fetch Rust crates
	npm ci
	cargo fetch --locked
assets: ## Compile app styles and self-host htmx
	npm run assets
	cp node_modules/htmx.org/dist/htmx.min.js app/assets/htmx.min.js
dev: assets ## Run local app (set TODOER_DEV_AUTH=1 and TODOER_DEV_TOKEN)
	mkdir -p .local/rebuild
	cargo run --locked --bin todoer-server
format: ## Format active Rust and frontend source
	cargo fmt --all
	npm run format
check: assets ## Check formatting, Clippy, unit/integration tests and build
	npm run format:check
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	node scripts/rebuild-sw-check.mjs
	cargo test --locked --workspace
	cargo build --locked --workspace
test: ## Run Rust and service-worker tests
	node scripts/rebuild-sw-check.mjs
	cargo test --locked --workspace
build: assets ## Build release binaries
	cargo build --locked --release --workspace
e2e: ## Run browser checks against a synthetic server already running
	npx playwright test
release: ## Publish app GitHub release; CI builds image (name=vX.Y.Z)
	python3 scripts/release.py app "$(name)"
cli-release: ## Publish CLI GitHub release; CI attaches binaries (name=vX.Y.Z)
	python3 scripts/release.py cli "$(name)"
www-build: ## Generate deployable public website into www/_site
	python3 www/tools/build.py
	npx tailwindcss -i www/source.css -o www/style.css --minify
	python3 scripts/www-stage.py
www-dev: ## Serve public website locally (port=8000 by default)
	python3 -m http.server $(or $(port),8000) --bind 127.0.0.1 --directory www
www-publish www-deploy: ## Manually dispatch Cloudflare Pages publishing on main
	gh workflow run www.yml --ref main
docs-build: ## Build public docs (install docs/requirements.txt first)
	python3 docs/build.py
deploy: ## Deploy configured pinned image on VPS; never deploys automatically
	./deploy/deploy.sh
backup: ## Verified backup to a new path (source=... destination=...)
	python3 scripts/rebuild-backup.py backup "$(source)" "$(destination)"
restore: ## Verify and restore to a new path (source=... destination=...)
	python3 scripts/rebuild-backup.py restore "$(source)" "$(destination)"
migrate: ## Apply Rust schema only; requires explicit TODOER_RUST_DB
	@test -n "$$TODOER_RUST_DB" || (echo 'Export TODOER_RUST_DB and back up first.' >&2; exit 2)
	TODOER_DEV_AUTH=0 cargo run --locked --bin todoer-server -- --migrate-only
image: ## Build production Docker image locally
	docker build -f deploy/Dockerfile -t todoer:local .

# Compatibility commands used by local integration tools.
.PHONY: rust-assets rust-check rust-test rust-build rust-dev rust-release-prepare
rust-assets: assets
rust-check: check
rust-test: test
rust-build: build
rust-dev: dev
rust-release-prepare:
	python3 scripts/rebuild-release-prepare.py self-test
