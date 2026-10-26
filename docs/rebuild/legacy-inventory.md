# Legacy inventory for the Rust rebuild

Source review: 2026-10-26. Read-only inspection of tracked application, migration, and component source; no databases, environment files, credentials, user content, or remote storage inspected. This describes source behavior, not a claim about the deployed schema. Scope: retain useful product behavior while removing Python/JinjaX/HTMX coupling.

## Data and migration contract

The existing database is SQLite accessed through `databases` and parameterized raw SQL. `app/repo/utils.py` creates canonical UUIDv4 strings and seven-character prefixes of shortuuid encodings. Preserve UUIDs and SIDs verbatim during import: URLs and object keys depend on them. IDs are not permissions. Queries authorize through task → section → list → space → space.creator; mutations use correlated EXISTS checks, and creation validates parent ownership with INSERT SELECT WHERE EXISTS. Reproduce this boundary on every Rust read, mutation, reorder, move, and download.

| Entity | Persisted fields beyond IDs/timestamps |
|---|---|
| users | name, unique email, bcrypt password, disabled, confirm_code, confirmed, last_login, invited_by, encrypted totp_secret, totp_enabled, JSON bcrypt totp_backup_codes |
| spaces | title, descr, creator, default, archived |
| lists | title, descr, hex color (legacy default FF0000), space, creator, default, archived, sort_by (manual default) |
| sections | title, order, list, creator, default, archived; descr removed by 003 |
| tasks | title, descr, order, section, complete, completed, archived, due_date TEXT, comma-separated tags, nullable priority |
| subtasks | title, descr, order, task, complete, completed, parent self-reference, nullable priority, due_date TEXT, comma-separated tags |
| comments | content, author, task |
| attachments | mime, fname, byte size, task; UUID is storage object key; original url column removed by 005 |
| task_audit_log | integer id, task, action, detail, actor, created |
| bookmarks | user, entity_type constrained to space/list, entity_uuid, order; unique user/type/entity |
| ai_usage | user, prompt_length, tasks_generated, created_at |
| notification_reads | creator, task_uuid, kind; unique creator/task/kind; no SID |
| user_settings | creator, key, value; unique creator/key |

Migration files 001–019 are individually executed by `scripts/migrate.sh`; the script has no applied-migration ledger. Migration 004 is Python and constructs spaces, assigns lists, rebuilds lists/sections, and retains renamed `_lists_20240428` and `_sections_20240428` tables. Renames may have rewritten dependent foreign keys: inspect actual sqlite_master, PRAGMA foreign_key_list, integrity_check, and foreign_key_check on a protected copy before deciding what to import. Do not replay 004 against a live populated database. Defaults in 006/007 use GROUP BY without reliable earliest-row selection. 012 adds list default_view/show_completed; 019 drops both. 018 adds nested subtask hierarchy and task-like metadata. Infer current shape from actual schema plus these changes, never from 001 alone.

SQLite UUID/VARCHAR/TIMESTAMP declarations are affinities, not strict validation. Preserve nullable parents, historical timestamp formats, integer booleans, order ties/gaps, tags and missing dates before normalizing. Parent/task consistency and cycles in nested subtasks require validation. comments.author is NOT NULL but has ON DELETE SET NULL, a contradictory deletion contract. Some foreign keys lack cascades (including bookmarks' polymorphic target), and migration 004 rebuild changes list foreign-key definitions. Foreign-key enforcement must be explicitly enabled per connection in Rust. Preserve bcrypt hashes rather than forcing resets; import default/archive flags and completed timestamps separately. Back up SQLite plus its WAL consistently and inventory attachment objects independently. An importer should be transactional, repeatable, reject dangling references with counts only, and produce per-table aggregate reconciliation without personal content.

## Authentication and security behavior

`app/security.py` directly uses bcrypt (AGENTS.md's passlib description is stale). Email lookup is exact equality and has a unique index. Signup requires consent and a password of at least eight characters; it transactionally creates an inbox hierarchy and immediately signs in. Confirmation updates a timestamp; confirmation email delivery is a TODO. Forgot-password POST only returns sent=True: no reset or delivery currently exists.

Auth is an HS256 JWT in an `auth` cookie whose subject is the user's email; signing key and token/cookie expiry are environment configured separately. Cookie is HttpOnly, SameSite=Lax, Secure except local, with default root path. Missing/expired/unknown-user cookies redirect 303 to signin with goto=current path. Ordinary signin validates goto as a local slash path and defaults to /inbox. Signout is GET and deletes both auth and pending_2fa cookies. API dependency supports Bearer JWTs, although this review found no /token handler. Current API and browser JWT decoding do not reject purpose=2fa_pending; a rebuild must enforce token purpose explicitly. Disabled browser users return a RedirectResponse from a dependency rather than raising, which needs deliberate correction and tests. Signin itself does not check disabled or confirmed.

TOTP uses pyotp defaults (30-second, six-digit SHA-1) with ±1 interval allowed, issuer Todoer. The five-minute pending_2fa JWT carries sub=email and purpose=2fa_pending in a separate HttpOnly cookie. TOTP secret ciphertext is Fernet with urlsafe-base64(SHA256(TODOER_SECRET_KEY)) as key. Preserve that compatibility or perform an explicit protected conversion; changing the JWT secret also affects decryption. Ten uppercase eight-hex-character recovery codes are individually bcrypt hashed into a JSON array and removed when used. Rust must consume recovery codes atomically. Existing successful 2FA accepts goto without the normal signin local-path check. Setup submits the secret in a hidden form field, and generated plaintext recovery codes are put into a redirect query string; replace with server-owned setup state and a one-time display. No explicit CSRF middleware, origin enforcement, rate limiting, token revocation, or TOTP replay guard was found in reviewed auth/main source. Add these deliberately instead of reproducing those gaps. Never log passwords, cookies, codes, ciphertexts, or presigned URLs.

## Attachments and rich text

`app/blob.py` uses S3-compatible storage configured through TODOER_STORAGE_* names; no values were read. Key is attachment UUID without extension, filename/MIME/size are in SQLite. Task rendering creates expiring GET presigned URLs using configured duration. Upload POST /attachments reads the whole file twice (size then upload) and uses public-read ACL by default. Object upload occurs before parent ownership is validated by repository INSERT; failed creation can orphan objects. Delete removes metadata first, then attempts object deletion and only logs storage failure. Cascaded task deletion has no object cleanup in this path. Implement bounded streaming, authorization before upload, private objects, filename/header safety, failure compensation, and reconciliation of orphaned objects. Migrate bytes and metadata together without renaming keys unless an explicit mapping is retained.

`app/tpl.py` escapes raw text before Markdown conversion, then marks its rendered HTML safe. Preserve user-visible Markdown/link behavior with an explicit sanitization policy in Rust, including safe URL schemes. Do not render imported descriptions/comments as trusted HTML.

## Visual and interaction reference

These are source tokens, not a new visual specification. `styles/global.css` uses Tailwind v4 via @import tailwindcss and forms/typography plugins; there is no root tailwind.config.js. Dark mode is system media based. The body is Inter/sans-serif, antialiased, light slate-100/slate-600 and dark gray-950/slate-300; main pane dark black. Source declares Inter but this review does not establish font-file loading. Preserve or explicitly bundle the font when implementing.

| Token | Source value |
|---|---|
| Font sizes | xs .75rem/1.5; sm .875rem/1.5715; base 1rem/1.5; lg 1.125rem/1.5; xl 1.25rem/1.5; 2xl 1.5rem/1.33; 3xl 1.88rem/1.33; 4xl 2.25rem/1.25; 5xl 3rem/1.25; 6xl 3.75rem/1.2 |
| Letter spacing | base–3xl -.01em; 4xl–6xl -.02em |
| Breakpoints | custom xs=480px; standard Tailwind sm/md/lg/xl/2xl used throughout |
| Width tokens | min widths 9/11/14/15/18/20rem; max widths 88/96rem |
| Shadow | 0 1px 3px 0 rgba(0,0,0,.08), 0 1px 2px 0 rgba(0,0,0,.02) |
| Medium shadow | 0 4px 6px -1px rgba(0,0,0,.08), 0 2px 4px -1px rgba(0,0,0,.02) |
| Large shadow | 0 10px 15px -3px rgba(0,0,0,.08), 0 4px 6px -2px rgba(0,0,0,.01) |
| XL shadow | 0 20px 25px -5px rgba(0,0,0,.08), 0 10px 10px -5px rgba(0,0,0,.01) |
| Focus/drag | ring-2 ring-indigo-500 rounded; outline token 2px solid rgba(0,112,244,.5) |

`components/Layout/App.jinja` uses a 100dvh clipped flex shell, independently scrolling main pane, lg rounded top/left pane corners, and 4rem mobile bottom allowance. `Nav/Menu.jinja` sidebar is 16rem wide, collapsible to 5rem at lg, always 16rem at 2xl; mobile is an absolute z-40 off-canvas panel, animated 200ms. Search and quick-add dialogs use max-w-4xl, full width, 15vh top margin; search uses zinc-950, slate-700 borders, rounded-lg and shadow-2xl with a black/50 backdrop. Search debounce is 300ms and results scroll to 60vh.

Tasks use compact row/card presentation: px-2 py-2, rounded-sm, shadow-lg, bottom slate-200/dark slate-800 border. Checkbox is 1.25rem circular; title font-semibold and responsive xs:text-xs/sm:text-sm/md:text-base/lg:text-lg. Completed rows opacity .4 and strikethrough; descriptions indent 2rem, use Markdown typography and 200-character previews. Priority P1 red, P2 orange, P3 yellow, P4 blue, using 500/20 backgrounds and 400 text; tag pills indigo-500/20 with indigo-400 text. Primary actions are indigo-600 → indigo-700 hover. Global buttons have 150ms transitions, px-3 py-2, rounded, medium weight; small px-2 py-1 and extra-small px-2 py-.5. Inputs use py-2 px-3, rounded, slate text and transparent dark background; native date inputs force dark color-scheme. Use CSS-generated actual color values when matching screenshots because Tailwind v4 palette is compiler-owned.

Interaction contracts: inline title/description editing with Escape reset and Cmd/Ctrl+Enter save; bulk selection and move/delete/complete; cross-section drag/reorder; mobile drag handle; quick add; search; date/tag pickers; hover-revealed menus; native dialogs. Vim navigation j/k, gg/G, numeric prefixes, section bounds, x/Enter/e/d/s actions, n quick-add, Cmd+k search, ? help. Settings support custom bindings. Existing HTMX fragments and HX-Trigger events refresh task/section indicators and parent views. Rust may replace transport but must preserve immediate updates, focus, selection, keyboard handling, and back/forward URL behavior. Reuse visual references from Page.List/Board/Space/Task/Dashboard/Calendar/Priority and Nav components, rather than redesigning from the landing page alone.

## Source route inventory

Routes below are extracted from router decorators, with /nav prefix applied from main.py. Protected routers use get_current_active_user; auth, landing, static, and manifest are public. Most responses are HTML or HTMX fragments, not JSON APIs. Main also serves GET /, GET /manifest.json, /public static assets, and a development hot-reload websocket. Keep durable page URLs or provide redirects when replacing fragment endpoints.

### account.py

- `GET /account`
- `PUT /account/fullname`
- `GET /account/2fa/setup`
- `POST /account/2fa/setup`
- `GET /account/2fa/backup-codes`
- `POST /account/2fa/disable`

### ai.py

- `POST /ai/generate-tasks`
- `POST /ai/add-tasks`

### attachments.py

- `DELETE /attachment/{sid}`
- `POST /attachments`

### auth.py

- `GET /signin`
- `POST /signin`
- `GET /auth/2fa/verify`
- `POST /auth/2fa/verify`
- `GET /signout`
- `GET /signup`
- `POST /signup`
- `GET /{sid}/confirmation/{code}`
- `GET /forgot-password`
- `POST /forgot-password`

### board.py

- `GET /list/{sid}/board`

### bookmarks.py

- `POST /bookmarks`
- `DELETE /bookmark/{sid}`

### calendar.py

- `GET /calendar`

### comments.py

- `DELETE /comment/{sid}`
- `PUT /comment/{sid}`
- `POST /comments`

### dashboard.py

- `GET /dashboard`

### lists.py

- `PATCH /list/{sid}/archive`
- `DELETE /list/{sid}`
- `GET /list/{sid}`
- `GET /list/{sid}/export.txt`
- `GET /list/{sid}/export.csv`
- `PUT /list/{sid}/color`
- `PUT /list/{sid}/title`
- `PUT /list/{sid}/descr`
- `PUT /list/{sid}/space`
- `POST /lists`

### nav.py

- `GET /nav/lists`
- `GET /nav/spaces`
- `GET /nav/bookmarks`

### notifications.py

- `GET /notifications`
- `GET /notifications/badge`
- `GET /notifications/all`
- `GET /notifications/all/content`
- `POST /notifications/read`
- `POST /notifications/unread`
- `POST /notifications/read-full`
- `POST /notifications/unread-full`
- `POST /notifications/read-all`
- `POST /notifications/unread-all`

### priority.py

- `GET /priority`
- `GET /priority/{level}`

### search.py

- `GET /search`

### sections.py

- `PATCH /section/{sid}/archive`
- `DELETE /section/{sid}`
- `GET /section/{sid}`
- `GET /section/{sid}/title`
- `PUT /section/{sid}/title`
- `POST /sections`
- `PUT /section/{sid}/list`
- `PUT /sections/reorder`
- `PATCH /section/{sid}/move-up`
- `PATCH /section/{sid}/move-down`
- `PATCH /section/{sid}/move-top`
- `PATCH /section/{sid}/move-bottom`

### settings.py

- `GET /settings`
- `PUT /settings/ai`
- `GET /settings/keybindings`
- `PUT /settings/keybindings`
- `POST /settings/keybindings/reset`

### spaces.py

- `GET /spaces`
- `GET /space/{sid}`
- `POST /spaces`
- `PATCH /space/{sid}/archive`
- `DELETE /space/{sid}`
- `PUT /space/{sid}/title`
- `PUT /space/{sid}/descr`
- `GET /inbox`

### subtasks.py

- `GET /subtask/{sid}`
- `DELETE /subtask/{sid}`
- `POST /subtasks`
- `PUT /subtask/{sid}`
- `PUT /subtask/{sid}/title`
- `PUT /subtask/{sid}/descr`
- `PUT /subtask/{sid}/complete`
- `PUT /subtask/{sid}/priority`
- `PUT /subtask/{sid}/due-date`
- `PUT /subtask/{sid}/tags`

### tasks.py

- `POST /tasks`
- `PUT /tasks/reorder`
- `POST /tasks/bulk/complete`
- `POST /tasks/bulk/delete`
- `POST /tasks/bulk/move`
- `GET /tasks/quick-add`
- `GET /tasks/quick-add/lists`
- `GET /tasks/quick-add/sections`
- `GET /task/{sid}`
- `GET /task/{sid}/indicators`
- `GET /task/{sid}/item`
- `PUT /task/{sid}`
- `PUT /task/{sid}/priority`
- `PUT /task/{sid}/due-date`
- `GET /api/tags`
- `PUT /task/{sid}/tags`
- `PUT /task/{sid}/descr`
- `PUT /task/{sid}/section`
- `PUT /task/{sid}/complete`
- `PUT /task/{sid}/title`
- `PATCH /task/{sid}/archive`
- `DELETE /task/{sid}`
- `PATCH /task/{sid}/move-up`
- `PATCH /task/{sid}/move-down`
- `PATCH /task/{sid}/move-top`
- `PATCH /task/{sid}/move-bottom`

