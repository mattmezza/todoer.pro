dev:
	uvicorn app.main:app --reload
tw:
	npx tailwindcss -i styles/global.css -o public/style.css --watch
migrate:
	cat "$(file)" | sqlite3 "$(db)"
