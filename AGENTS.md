# Todoer Pro

FastAPI + Jinja2/JinjaX app with HTMX frontend, SQLite DB, Tailwind CSS.

## Setup

```bash
uv sync                 # install deps
cp .env.example .envrc  # configure env vars
make dev                # run dev server (port is $TODOER_PORT)
make cssd               # run tailwind in watch mode (separate terminal)
```

## Project structure

- `app/` - FastAPI application
  - `main.py` - app entrypoint, mounts routers
  - `routers/` - route handlers (return HTML responses via `@tpl.htmx()` decorator)
  - `repo/` - data access layer (raw SQL via `databases` lib)
  - `svc/` - service/business logic
  - `deps.py` - FastAPI dependencies (DB, auth, renderer)
  - `security.py` - auth (JWT via python-jose, passwords via passlib)
  - `tpl.py` - template rendering helpers, markdown, linkify, JinjaX catalog
  - `http.py` - `Opt` monad for chaining `.or_404()` / `.or_400()` on nullable results
  - `blob.py` - S3-compatible blob storage for attachments (presigned URLs)
- `components/` - JinjaX components (`.jinja` files)
- `styles/` - Tailwind CSS source
- `migrations/` - numbered SQL migrations (`NNN.up.sql` / `NNN.down.sql`)
- `scripts/` - build, deploy, migrate shell scripts
- `deploy/` - docker-compose files per environment

## Conventions

- Formatting: `black` (line-length 79, skip-string-normalization) + `isort`
- Env vars prefixed with `TODOER_`
- Repo layer uses raw SQL strings, not an ORM
- Components are PascalCase JinjaX with dot notation (e.g. `<Section />`, `"Page.Task"`)
- Migrations are sequential numbered files; some are `.py` for data migrations

## Architecture patterns

### Request lifecycle
1. Router handler decorated with `@tpl.htmx("Component.Name")`
2. Handler returns `dict` (200), or `(dict, status)`, or `(dict, status, headers_dict)`
3. The `htmx` decorator renders the JinjaX component with the returned dict as template vars
4. HTMX headers (especially `HX-Trigger`) drive client-side reactivity

### Data access (repo layer)
- `RepoDB` → `.for_user(user)` returns `ForUser` → `.tasks()`, `.sections()`, etc.
- Each repo class extends `LoggedInUserRepo` which holds `self.db` and `self.user`
- IDs: every entity has both `uuid` (UUIDv4 string) and `sid` (7-char shortuuid) — `sid` is used in URLs
- `new_ids()` in `repo/utils.py` generates both
- Authorization: SQL queries JOIN through the chain (task→section→list→space) checking `sp.creator = :creator`
- UPDATE/DELETE use correlated `EXISTS` subqueries for auth (correlated via `WHERE ta.uuid = tasks.uuid`)
- CREATE uses `INSERT...SELECT...WHERE EXISTS` to validate foreign key ownership

### HTMX + HX-Trigger event patterns
- Events follow the pattern: `entity-action` (global) + `entity-{sid}-action` (specific)
- Examples: `task-create`, `task-update`, `task-{sid}-update`, `section-{sid}-update`
- Components listen via `hx-trigger="event-name from:body"` to reactively re-fetch
- Section components self-refresh on `section-{sid}-update from:body`

### JinjaX components
- Props declared with `{# def prop1, prop2, optional=default #}`
- Nested via dot notation: `components/Tasks/TaskItem.jinja` → `<Tasks.TaskItem />`
- `content` variable holds child content (slot)
- Common patterns: `<Menu.Ctx>` for dropdown menus, `<Btn.Create>` for add buttons

### Inline editing pattern (used throughout)
1. Display element (h1/span) with hyperscript: `_="on click hide me then show next <form /> then set el to next <form /> then call el.querySelector('input').select()"`
2. Hidden `<form>` with htmx PUT, `style="display: none"`
3. Cancel via `type="reset"` button + hyperscript to re-show display element
4. Escape key: `_="on keyup[key is 'Escape'] reset() the closest <form />"`
5. Cmd/Ctrl+Enter submit via Alpine: `@keydown.meta.enter="...dispatchEvent(new Event('submit'))"`

### Hyperscript gotchas (v0.9.11)
- `close me` is NOT valid — use `call me.close()` for dialog elements
- CSS selectors must use `<.class/>` query syntax, not bare `.class`
- Chained method calls (`next <.foo/>.querySelector('x').focus()`) fail — use `set el to next <.foo/> then call el.querySelector('x').focus()`
- Always test hyperscript in browser console after changes

### SortableJS notes
- `chosenClass` / `dragClass` / `ghostClass` only accept a SINGLE class token (no spaces)
- Use CSS classes with `@apply` for multi-property styles (see `.sortable-chosen` in global.css)
- `forceFallback: true` prevents text selection during drag (bypasses native HTML5 drag API)

### Textarea auto-resize (Alpine.js pattern)
```
x-data="{ resize() { $el.style.height = '0px'; $el.style.height = $el.scrollHeight + 'px' } }"
x-init="$nextTick(() => resize())"
@input="resize()"
```

### Migrations
- Files: `migrations/NNN.up.sql` and `migrations/NNN.down.sql`
- Python data migrations: `migrations/NNN.up.py`
- Run via: `./scripts/migrate.sh -eprod migrations/NNN.up.sql`
- SQLite DB lives at `deploy/data/todoer_{env}.db`
- Schema: users → spaces → lists → sections → tasks → subtasks/comments/attachments

### DB schema (key relationships)
- `spaces` has `creator` (FK users), `default` flag
- `lists` has `space` (FK spaces), `creator`, `default` flag, `color`
- `sections` has `list` (FK lists), `creator`, `order`, `default` flag
- `tasks` has `section` (FK sections), `order`, `complete`, `completed`
- `subtasks` has `task` (FK tasks), `order`, `complete`
- `comments` has `task` (FK tasks), `author` (FK users)
- `attachments` has `task` (FK tasks)
- All entities have `uuid` (PK) and `sid` (unique short ID for URLs)

### Frontend stack
- HTMX for server-driven interactivity
- Alpine.js for client-side state (dropdowns, auto-resize, toggle)
- Hyperscript (`_="..."`) for simple DOM manipulation (show/hide, focus)
- Tailwind CSS for styling (dark mode via `media` strategy, use `dark:` prefix)
- SortableJS for drag-and-drop reordering (tasks, sections)
- Group hover pattern: `group` on parent, `group-hover:flex` on child for hover-reveal actions
- Native date inputs: `color-scheme: dark` in CSS for dark calendar picker UI

### Keyboard navigation
- Vim-style: `j`/`k` (move), `G` (last), `gg` (first), `{`/`}` (section bounds)
- Numeric prefix: `2j` = move 2 down, `3k` = move 3 up
- Actions: `x` (complete), `Enter` (open), `e` (edit title), `d` (due date), `s` (select mode)
- `n` = quick add, `Cmd+k` = search, `?` = shortcuts dialog
- Implementation: single `keydown` listener in `Layout/App.jinja` with state vars for `gg` sequence and numeric prefix

### Alpine.js x-data scopes
- Any page with TaskItem components MUST have an `x-data` scope with `selectMode`, `toggleSelect()`, etc.
- Without `x-data`, Alpine `@click` directives are dead — tasks won't be clickable
- Both `Page.List` and `Page.Space` have this scope

### Hot-reload (dev only)
- arel library provides WebSocket-based hot-reload
- WS URL uses `TODOER_PORT` env var (set by Makefile from `port=` arg)
- Script injected only when `ENV == "local"`
- Must run both `make serve-reload` and `make tailwind-watch` in separate terminals


### Development pattern

- Python 3.14+, managed via `uv` (pyproject.toml + uv.lock)
- commit frequently
- always judge before doing a modification: are you adding or removing complexity?
