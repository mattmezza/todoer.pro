dev:
	uvicorn app.main:app --reload
tw:
	npx tailwindcss -i global.css -o public/style.css --watch
migrate-db:
	cat "$(file)" | sqlite3 ../todo.db
