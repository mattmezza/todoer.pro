# REST client workflow

The Rust rebuild provides the `todoer` CLI as a REST client. Run it from the repository with `cargo run --manifest-path rebuild/Cargo.toml --bin todoer -- ...` or install/build the binary and use the examples below.

Set `TODOER_TOKEN` to your configured Todoer bearer credential. `TODOER_URL` defaults to `http://127.0.0.1:8092`; `--url` overrides it. Keep tokens out of shell history and source control. The CLI does not print the token.

The development token adapter is restricted to explicit development mode. With configured production authentication, create a REST credential in browser Settings; the CLI uses that credential and does not manage browser sessions. MCP at `/mcp` supports the implemented OAuth flow, tested with the official Python SDK against an isolated provider fixture. This is not a claim of compatibility with every remote connector. The CLI examples use REST directly.

```sh
todoer list
todoer --json tasks LIST_ID
todoer search --query 'proposal' --list LIST_ID --complete false --limit 50
todoer search --query 'proposal' --after LAST_TASK_ID
todoer capture --list LIST_ID --key daily-plan-2026-10-26 'Draft proposal' 'Review notes'
todoer edit TASK_ID --revision 1 --title 'Draft revised proposal'
todoer complete TASK_ID --revision 2
todoer complete TASK_ID --revision 2 --subtree
todoer reopen TASK_ID --revision 3
todoer subtree TASK_ID
todoer move-preview TASK_ID --destination DESTINATION_LIST_ID
todoer move TASK_ID --destination DESTINATION_LIST_ID --revision 4
# After reviewing a preview that adds access for new members:
todoer move TASK_ID --destination DESTINATION_LIST_ID --revision 4 --acknowledge-access-change
todoer changes --after 0
```

Quote each capture title containing spaces. Each positional argument becomes one task with an empty description and no parent. Reuse the same idempotency key when retrying the same capture request; choose a new key for a new capture. The server checks idempotency.

Read the current task revision before editing, completing, reopening, or moving it. On a conflict, fetch the latest tasks or subtree and reconcile the desired change before retrying with its new revision. `complete --subtree` explicitly requests completion of the subtree. `--description ''` clears a description.

Search defaults to an empty query and at most 50 results. Use `--list` to narrow the list, `--complete true` or `--complete false` to filter status, and `--after` with the last task ID to request the next page. Query parameters are URL encoded.

Before moving a subtree, use `move-preview` to inspect whether the destination adds members who can access it. Moves that broaden access require `--acknowledge-access-change` after reviewing that preview. The default is false.

Use `--json` for scripts: successful response bodies are printed unchanged to stdout. Human output shows task completion, identifiers, titles, and revisions. Errors go to stderr. Exit codes are 0 for success, 2 for invalid CLI arguments or HTTP validation errors, 3 for missing credentials or HTTP authentication/authorization errors, 4 for HTTP revision/conflict errors, and 1 for other failures. Network requests time out after 30 seconds and redirects are not followed.

The CLI calls `/api/v1/search`, `/api/v1/lists`, `/api/v1/lists/{id}/tasks`, `/api/v1/capture`, `/api/v1/tasks/{id}`, `/api/v1/tasks/{id}/subtree`, `/api/v1/tasks/{id}/move-preview?destination=...`, `/api/v1/tasks/{id}/move`, and `/api/v1/changes?after=N`. Capture sends `Idempotency-Key`; all requests send bearer authentication. No remote MCP transport is implied by these endpoints.

The official Python MCP SDK check is available as `uv run --no-project scripts/rebuild-mcp-check.py`. It pins `mcp==2.3.0` with inline script metadata, leaving project dependencies unchanged. Set `TODOER_TOKEN` and optionally `TODOER_MCP_URL` (default `http://127.0.0.1:8092/mcp`) before running it against the development server. The check initializes a real HTTP MCP session, lists the tools, resolves the inbox, retries an identical capture, and reads the resulting subtree. Each run creates one synthetic inbox task with a fresh idempotency key.

The check requires the implemented tool subset, including bulk preview/apply and saved destinations. It exercises initialization, destination resolution, identical capture retry, subtree read, edit, guarded undo, trash, restore, destination preferences, bulk dry-run, guarded apply, retry, and rollback. SDK 2.3.0 currently logs `Session termination failed: 202` when this rmcp transport acknowledges session cleanup; the request assertions complete and the check exits successfully.


Implemented recovery commands: `todoer trash-list`, `todoer trash TASK_ID --revision N`,
`todoer restore TASK_ID --revision N`, `todoer undo CHANGE_SEQ --revision N`, and
`todoer capabilities`. `make rust-integration` exercises the Rust CLI over real
HTTP and the official Python MCP client against an isolated running server. It
creates synthetic tasks; never run verification scripts against a personal or
production account.


Saved destinations make repeated agent requests explicit without conversation state:

```sh
todoer destination set capture LIST_ID
todoer destination resolve --preference-key capture
todoer destination resolve --fallback-inbox
todoer destination search --prefix Work --limit 20
todoer destination preferences
```

An explicit destination takes precedence over preferences. Exact duplicate titles return accessible choices; select an ID before writing. A missing explicit destination never silently falls back. Preferences are filtered against current access.

Bulk writes use a server-stored preview with a ten-minute expiry. Review its before/after states and access changes, then apply its exact hash:

```sh
todoer --json bulk preview --body '{"operations":[{"op":"update","id":"TASK_ID","revision":1,"title":"Reviewed title"}]}'
todoer --json bulk apply PREVIEW_ID --hash EXPECTED_HASH --key UNIQUE_RETRY_KEY
```

At most 50 operations and 500 affected task states fit one preview. Supported operations are title/description update, move, trash, restore, existing tags, section assignment, and same-list reparenting. Completion and entity creation use their individual APIs. Existing tag names must already exist in the task's destination scope. Preview runs the common operations and rolls them back; apply rechecks current revisions, permissions, and metadata and commits all operations together. Reuse the apply key only for the same preview and hash. A later failure rolls back earlier operations.

The compact MCP tools are `preview_changes`, `apply_changes`, `resolve_destination`, and `configure_destination`, alongside the existing task tools. Each tool rechecks the current request credential. The OAuth SDK fixture check in `scripts/rebuild-oauth-mcp-check.py` is test-only: it requires an isolated loopback server and synthetic browser session inputs supplied by the Rust integration harness. It exercises discovery, registration, PKCE, consent, token exchange, scoped tools, saved destinations, bulk retry, and optional token refresh without logging credentials.

Structured search and organization use the common domain operations:

```sh
todoer --json query --text proposal --entity task --entity comment --status all --list LIST_ID --due-from 2026-10-26 --due-to 2026-10-30 --sort due --field title --field context
todoer --json query --body '{"text":"proposal","entities":["task"],"fields":[],"limit":20}'
todoer tags list LIST_ID --prefix urgent
todoer tags create LIST_ID urgent
todoer tags get TASK_ID
todoer tags set TASK_ID --revision 1 --tag urgent --tag-id EXISTING_TAG_UUID
todoer tags rename TAG_UUID --revision 1 'high priority'
todoer task-section TASK_ID
todoer task-section ROOT_TASK_ID --revision 2 --section SECTION_ID
todoer task-section ROOT_TASK_ID --revision 3 --clear
todoer reparent TASK_ID --revision 4 --parent PARENT_TASK_ID
todoer reparent TASK_ID --revision 5
todoer reorder LIST_ID --revision LIST_REVISION FIRST_TASK_ID SECOND_TASK_ID
```

`tags set` replaces the complete tag set; no `--tag` or `--tag-id` clears it. Names create missing tags atomically with assignment. Standalone workspace tag creation requires workspace editor access; direct list guests cannot list unused workspace tags. Section writes apply to roots and their descendants. Reparenting is same-list only, rejects cycles, and follows the new parent's section; omitting `--parent` creates a root. Sibling reorder requires every active sibling exactly once and the current **list** revision. Read current revisions after each write. The compact `organize --body` command accepts the OpenAPI `OrganizeAction` schema when scripting more involved actions.

```sh
todoer references resolve '#Inbox'
todoer references autocomplete '%T-' --limit 20
todoer references unresolved task TASK_ID
todoer references bind task TASK_ID --revision TASK_REVISION --occurrence 0 --target-kind list --target-id CHOSEN_LIST_ID
todoer references backlinks list LIST_ID
```

Resolve returns zero, one, or multiple accessible choices; choose explicitly when ambiguous. Occurrences come from the saved description/comment parser, excluding code and existing links. Binding requires the current source task/comment revision, increments it, and preserves other stable bindings. A target must still be an accessible exact candidate for the original token. Backlinks recheck both target and source access. These commands never expose another workspace's inaccessible candidates.

MCP adds three compact tools: `query` takes the canonical structured query, `organize_tasks` takes an `action` discriminated by `op`, and `references` takes a reference action. Their complete schemas are returned by `list_tools` and share the checked-in OpenAPI schemas. The original `search` tool remains available for its older task-only contract. Query cursors bind filters, actor, access and index state: on `resync_required`, restart without a cursor. See [structured search](search.md) and [references](references.md) for limits and exact semantics.
