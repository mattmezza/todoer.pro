start:
	uvicorn app.main:app --reload
tw:
	npx tailwindcss -i styles/global.css -o public/style.css --watch

local:
	./scripts/build.sh local
dev:
	./scripts/build.sh dev
prod:
	./scripts/build.sh prod

release:
	gh release create $(VERSION) --generate-notes
