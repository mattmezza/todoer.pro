.PHONY: help
help: ## show help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m\033[0m\n"} /^[$$()% a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: dev cssd css
dev:  ## serves the app with debug and auto reload on (e.g. make serve-reload port=3000)
	uv run uvicorn app.main:app --reload --port $(TODOER_PORT)
cssd:  ## starts tailwindcss compilation and watches files for change
	npx @tailwindcss/cli -i styles/global.css -o public/style.css --watch
css:  ## compiles css with tailwindcss
	npx @tailwindcss/cli -i styles/global.css -o public/style.css --minify

.PHONY: release
release:  ## build & push docker image to GHCR, create github release (e.g. make release name=v1.0)
	docker build -t ghcr.io/mattmezza/todoer.pro:$(name) -t ghcr.io/mattmezza/todoer.pro:latest .
	docker push ghcr.io/mattmezza/todoer.pro:$(name)
	docker push ghcr.io/mattmezza/todoer.pro:latest
	gh release create $(name) --generate-notes
	git fetch --all

.PHONY: build
build:  ## builds a docker image for specific env (e.g. make build env=local name=v0.20)
	git fetch --all
	git checkout $(name)
	./scripts/build.sh $(env)

.PHONY: deploy
deploy: build ## deploys a specific release onto specific env (e.g. make deploy env=prod name=v0.20)
	./scripts/deploy.sh -e$(env)

# Rust rebuild stays alongside the legacy app until an authorized cutover.
.PHONY: rust-setup rust-dev rust-assets rust-format rust-lint rust-test rust-build rust-e2e rust-check
rust-setup: ## install locked frontend dependencies and fetch Rust crates
	npm ci
	cargo fetch --locked
rust-assets: ## compile Tailwind v4 and self-host htmx
	npm run assets
	cp node_modules/htmx.org/dist/htmx.min.js rebuild/assets/htmx.min.js
rust-dev: rust-assets ## start isolated loopback Rust app; requires TODOER_DEV_AUTH/TODOER_DEV_TOKEN
	mkdir -p .local/rebuild
	cargo run --locked --bin todoer-server
rust-format: ## format Rust and rebuild frontend source
	cargo fmt --all
	npm run format
rust-lint: ## deny compiler and Clippy warnings
	cargo clippy --locked --all-targets -- -D warnings
rust-test: ## core and HTTP integration tests
	cargo test --locked
rust-build: rust-assets ## build Rust binaries reproducibly
	cargo build --locked --release
rust-e2e: ## browser tests against already-running isolated Rust development server
	npx playwright test
rust-check: rust-assets ## format, lint, tests and debug build
	npm run format:check
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
	cargo build --locked
