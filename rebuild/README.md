# Todoer Rust rebuild — development increment

This application runs alongside the preserved Python app. **The full requested
rebuild is unfinished and this is not a production release.** See the
[implementation ledger](../docs/rebuild/IMPLEMENTATION.md) for the full scope.

From repository root:

```sh
make rust-setup
mkdir -p .local/rebuild
export TODOER_DEV_AUTH=1
export TODOER_DEV_TOKEN="$(openssl rand -hex 32)"
export TODOER_TOKEN="$TODOER_DEV_TOKEN"
make rust-dev
# another terminal, with the same TODOER_TOKEN:
cargo run --locked --bin todoer -- list
cargo run --locked --bin todoer -- capture --list groceries --key grocery-request-1 milk bread eggs
```

Open http://127.0.0.1:8092/signin. Development sign-in accesses synthetic lists,
not the old account. The listener is loopback-only. Without explicit dev auth,
startup fails closed. A new random session is generated on each process start.
The development token hash is held in memory; production scoped/revocable tokens
and Google login are not delivered. Do not expose this server through a proxy.

`make rust-check` runs formatting, lint, Rust unit and in-process HTTP tests, and
builds. `npx playwright install chromium firefox webkit` installs browsers;
`make rust-e2e` exercises a running development instance using synthetic tasks.
Browser tests require the explicitly documented test token in their fixture;
use a separate test instance rather than an instance containing personal data.
Screenshots and databases belong in ignored `.local/`, never git.

The development SQLite file is independent. Opening a legacy database is rejected
before schema changes. No importer or production cutover is provided yet. Do not
point the Rust app at existing data. To stop, Ctrl-C; existing committed writes
remain in SQLite. Do not delete the old application or its storage.

## Current behavior

- Groceries/Inbox, persistent quick capture, task details, completion/reopening.
- Core operations shared by HTML/REST/CLI. Atomic capture batches (1–100), retry
  keys scoped to actor, payload mismatch rejection, required entity revisions.
- Nested tasks via API, atomic subtree moves with identity retention, and access
  broadening preview/acknowledgement. An incomplete descendant blocks ordinary
  parent completion; explicit complete-subtree is available. Reopening reopens
  ancestors. Adding an unfinished child also reopens ancestors.
- Title/description prefix search uses FTS5, Unicode tokenization, literal quoted
  terms, completed filters and stable ID pagination. No typo/fuzzy matching.
- Live permission checks on reads/writes, replay, search and change history.
- Polling change feed; open task drafts are preserved, stale saves conflict,
  draft text survives page reload within the browser session. No full-page live
  replacement while drafting. Persistent offline capture is still unfinished.
- Light/dark media themes, phone navigation, keyboard quick-add, 44px controls.

Descriptions currently display escaped plain text. Markdown/reference parsing,
most planning screens, production sharing/auth, attachments, notifications,
recurrence, exports/import/recovery and production delivery remain unfinished.
