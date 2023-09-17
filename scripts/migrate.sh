cat "migrations/$1.sql" | sqlite3 ../todo.db
