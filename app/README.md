# Todoer application

The current Rust implementation: Axum HTTP server, Askama templates, SQLite,
REST and MCP interfaces, administrative importer, and standalone REST CLI.
Run commands from the monorepo root; see [README](../README.md).

- `src/bin/todoer-server.rs`: web/API/MCP server.
- `src/bin/todoer.rs`: user-facing CLI.
- `src/bin/todoer-migrate.rs`: explicit administrative import/export tool.
- `migrations/`: sequential Rust schema migrations.
- `templates/`, `assets/`: server-rendered UI and frontend assets.
- `browser/`, `tests/`: synthetic browser and integration checks.

The Rust database is independent from the historical Python schema. See the
[migration runbook](../deploy/MIGRATION.md) before importing real data.
