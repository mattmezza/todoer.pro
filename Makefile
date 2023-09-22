dev-api:
	DEBUG=true uvicorn app.main:app --reload
dev-tw:
	npx tailwindcss -i global.css -o public/style.css --watch
migrate-db:
	cat "$(file)" | sqlite3 ../todo.db
