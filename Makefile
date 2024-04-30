serve-reload:
	uvicorn app.main:app --reload
tailwind-watch:
	npx tailwindcss -i styles/global.css -o public/style.css --watch
tailwind:
	npx tailwindcss -i styles/global.css -o public/style.css --minify

local:
	./scripts/build.sh local
dev:
	./scripts/build.sh dev
prod:
	./scripts/build.sh prod

release:
	gh release create $(VERSION) --generate-notes
