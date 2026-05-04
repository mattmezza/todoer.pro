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
