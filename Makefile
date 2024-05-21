.PHONY: help
help: ## show help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m\033[0m\n"} /^[$$()% a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: serve-reload tailwind-watch tailwind
serve-reload:  ## serves the app with debug and auto reload on
	uvicorn app.main:app --reload
tailwind-watch:  ## starts tailwindcss compilation and watches files for change
	npx tailwindcss -i styles/global.css -o public/style.css --watch
tailwind:  ## compiles css with tailwindcss
	npx tailwindcss -i styles/global.css -o public/style.css --minify

.PHONY: release
release:  ## creates a release on github (e.g. make release VERSION=v0.20)
	gh release create $(VERSION) --generate-notes
	git fetch --all

.PHONY: build
build:  ## builds a docker image for specific env (e.g. make build ENV=local VERSION=v0.20)
	git fetch --all
	git checkout $(VERSION)
	./scripts/build.sh $(ENV)

.PHONY: deploy
deploy: build ## deploys a specific release onto specific env (e.g. make deploy ENV=prod VERSION=v0.20)
	./scripts/deploy.sh -e$(ENV)
