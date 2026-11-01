# REST client workflow

For cloud, set `TODOER_URL=https://app.todoer.pro`; for self-hosting use your server’s HTTPS origin. Prefer `todoer login --permission read` (or `write` for authorized changes) and approve the selected lists in your browser. A saved login is used automatically. Remote MCP clients connect to the same origin’s `/mcp` endpoint with their own OAuth authorization; REST credentials and MCP credentials are distinct. See [CLI sign-in](cli-login.md), and the [Todoer skill](../../skills/todoer.pro/SKILL.md).

The Rust rebuild provides the `todoer` CLI as a REST client. Run it from the repository with `cargo run --manifest-path app/Cargo.toml --bin todoer -- ...` or install/build the binary and use the examples below.

Alternatively, set `TODOER_TOKEN` to your configured REST bearer credential. `TODOER_URL` defaults to `http://127.0.0.1:8092`; `--url` overrides it. Keep tokens out of shell history and source control. The CLI does not print the token.

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

Use `--json` for scripts: successful response bodies are printed unchanged to stdout. Human list views show titles and references, with completion status for tasks. Use `--json` for revisions. Errors go to stderr. Exit codes are 0 for success, 2 for invalid CLI arguments or HTTP validation errors, 3 for missing credentials or HTTP authentication/authorization errors, 4 for HTTP revision/conflict errors, and 1 for other failures. Network requests time out after 30 seconds and redirects are not followed.

`todoer space all`, `todoer lists all`, and `todoer sections all LIST` show names and short references. Task listings show status and task references; `todoer subtree TASK` indents descendants and shows a continuation cursor when needed. References use `@` for spaces, `#` for lists, `$` for sections, `%` for tasks, and `^` for subtasks. Use `todoer view REFERENCE` to show an entity and its immediate children. Add `--info` for its description and child counts: `s` sections, `o` open top-level tasks, and `d` completed top-level tasks. List views include sections and unsectioned top-level tasks; section views show their top-level tasks, and task views show direct subtasks. Subtasks are excluded from the task counts. `todoer --json view REFERENCE` includes revisions and the structured children. Quote references in a shell, for example `todoer view '#abc2345'` or `todoer view '$abc2345'`. List references also work with `lists get/edit`, `tasks`, `capture --list`, and `sections all/create`; space references work with `space get/edit`, and section references with `sections edit`. Other commands may still require the full ID, available through `--json`; older servers also show the full ID in the overview. `workspace` remains an alias for `space`.

The CLI calls `/api/v1/search`, `/api/v1/lists`, `/api/v1/lists/{id}/tasks`, `/api/v1/capture`, `/api/v1/tasks/{id}`, `/api/v1/tasks/{id}/subtree/query`, `/api/v1/tasks/{id}/move-preview?destination=...`, `/api/v1/tasks/{id}/move`, and `/api/v1/changes?after=N`. Capture sends `Idempotency-Key`; all requests send bearer authentication. No remote MCP transport is implied by these endpoints.

The official Python MCP SDK check is available as `uv run --no-project scripts/rebuild-mcp-check.py`. It pins `mcp==2.3.0` with inline script metadata, leaving project dependencies unchanged. Set `TODOER_TOKEN` and optionally `TODOER_MCP_URL` (default `http://127.0.0.1:8092/mcp`) before running it against the development server. The check initializes a real HTTP MCP session, lists the tools, resolves the inbox, retries an identical capture, and reads the resulting subtree before checking the implemented compact operations. Each run creates synthetic inbox tasks, a tag, two owned workspaces and a movable list, with fresh capture keys; the optional attachment fixture also creates and soft-deletes a synthetic file.

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

Scheduling uses `schedule_tasks` in MCP and typed CLI commands:

```sh
todoer schedule get TASK_ID
todoer schedule set TASK_ID --revision TASK_REVISION --body '{"planned":{"kind":"date","date":"2026-10-26"},"due":{"kind":"timed","local":"2026-10-27T16:00:00","timezone":"Europe/Zurich"},"priority":2,"reminders":[]}'
todoer schedule start ROOT_TASK_ID --revision TASK_REVISION --body '{"rule":{"kind":"daily","interval_days":1},"start_local":"2026-10-27T16:00:00","timezone":"Europe/Zurich","count":5}'
todoer schedule series SERIES_ID
todoer schedule state SERIES_ID --revision SERIES_REVISION paused
todoer schedule state SERIES_ID --revision SERIES_REVISION active
todoer schedule occurrences SERIES_ID
todoer schedule skip TASK_ID --revision TASK_REVISION
todoer schedule planning --from 2026-10-26 --to 2026-11-01
todoer schedule statistics --from 2026-10-26
```

Schedule set replaces the entire schedule: omitted planned/due/priority values clear them, and omitted reminders become empty. Task scheduling/start/skip use the task revision; series state changes use the series revision. Recurrence supports unfinished root tasks with bounded nested checklists of at
most 500 live tasks. The common operation enforces the bound, local-time validity,
and live access. Recurring checklist completion requires explicit subtree
completion, including when its children are already complete. Successors receive
new task IDs while copying hierarchy, order, tags, sections and schedules;
comments/files stay on their original occurrence. See [scheduling](scheduling.md)
for skip, pause/resume and historical reopening behavior. Planning returns `{tasks,truncated}` with at most 100 compact tasks and no description bodies. Series listing similarly returns `{series,truncated}` capped at 100. Use structured queries for paginated date/status exploration. Reminder transport configuration and actual delivery remain server concerns; setting a reminder does not prove delivery.

Comments and files share the `task_activity` MCP tool and these CLI commands:

```sh
todoer comments list TASK_ID --after 0 --limit 20
todoer comments list TASK_ID --full
todoer comments add TASK_ID --key UNIQUE_RETRY_KEY 'Review #Inbox'
todoer comments get COMMENT_ID
todoer comments edit COMMENT_ID --revision COMMENT_REVISION 'Reviewed text'
todoer comments delete COMMENT_ID --revision COMMENT_REVISION
todoer files list TASK_ID
todoer files upload TASK_ID --file ./report.pdf --dry-run
todoer files upload TASK_ID --file ./report.pdf
todoer files resume INTENT_ID --file ./report.pdf
todoer files info ATTACHMENT_ID
todoer files download ATTACHMENT_ID --output ./verified-report.pdf
todoer files delete ATTACHMENT_ID --revision ATTACHMENT_REVISION
```

Comment creation retries use the same key and exact body. Editing/deleting uses independent comment revisions and the common author-or-owner moderation rule. Compact comment lists return 300-character excerpts, cursor `next_after_seq`, and `page_limit`; full-body pages cap at 10 comments, while excerpt pages accept 1–50. An explicit comment get returns one full saved body. Attachment lists cap at 100 records with a `truncated` flag; structured filename queries can locate additional authorized IDs.

File transfer accepts regular files up to 25 MiB, computes SHA-256 locally, creates an intent, sends the exact bytes through the authenticated REST proxy, and finalizes only after the shared storage verifier confirms size and digest. Dry-run reads/checks the local file and prints its size/hash without contacting the server. On interruption the error includes the intent ID; resume requires the same size/hash. `files intent TASK_ID --body '{"filename":"report.pdf","size":123,"sha256":"64_HEX_CHARACTERS"}'` exposes the bounded direct-upload plan for a separate uploader. `files finalize INTENT_ID --revision INTENT_REVISION` (alias `files confirm`) verifies and completes that upload; it never trusts a client assertion that bytes were uploaded. File bytes are not kept in a hidden CLI cache. Signed upload URLs are temporary credentials and must not be logged.

Downloads verify the declared size and SHA-256 before writing and refuse an existing output path. Transfer requests time out after 60 seconds; ordinary single requests retain their 30-second timeout. Transfer JSON output is the final metadata/result; single API responses retain their original response body. Soft deletion removes attachment metadata from reads while retaining object bytes under the server's documented retention policy.

MCP attachment operations accept metadata/intents, not inline binary bodies. A checked `file_link` returns metadata plus an authenticated application download URL and explicitly reports `mcp_oauth_credential_accepted:false`. Download requires a browser session or a separate REST-audience credential; a production MCP OAuth token cannot be copied into the REST CLI to cross that audience boundary. The isolated development adapter is a shared synthetic fixture. The MCP client can use the signed upload plan with a separate HTTP uploader, then request verified finalization.

`manage_access` groups organization and sharing operations in MCP. The existing workspace/list/section CLI commands and `access --body` use the same authority checks. Access expansion requires explicit acknowledgement; missing acknowledgement or stale workspace/list revisions returns conflict. Invitations are email-bound grants and may queue email delivery:

```sh
todoer invitations list space SPACE_ID
todoer invitations create space SPACE_ID person@example.invalid --role viewer --key UNIQUE_RETRY_KEY --acknowledge-access-change
todoer invitations revoke INVITATION_ID
```

Invitation retries must retain the target, email, role and key. Creating or revoking an invitation does not sign in its recipient or claim access; acceptance keeps the browser/Google proof flow. Scoped read/write credentials do not gain workspace administration or personal credential-management authority. Workspace member output from the compact access adapter caps at 100 entries with a `truncated` flag.

The compact adapters are `POST /api/v1/agent/schedule`, `/activity`, and `/access`; their exact discriminated action schemas and response unions are in OpenAPI. The official SDK checks exercise actual scheduling/planning/statistics, comment retry/edit/delete, and access reads in addition to taxonomy/bulk. Set `TODOER_TEST_ATTACHMENTS=1` only with an isolated loopback S3 fixture to also exercise signed upload, SHA-256 finalization/retry, checked link metadata, listing and soft deletion. The test refuses a non-loopback object endpoint. These checks create synthetic records and do not establish compatibility with every connector or production object service.

Historical integration observation on 2026-10-26 (not rerun by the later runbook audit): the official OAuth SDK check passed with inbox-only consent, scheduling/planning/statistics, comment operations and actual token refresh. A separate bearer SDK check passed all 21 registered tools and real signed attachment operations against Moto 5.1.17. An isolated CLI HTTP/Moto check passed scheduling/recurrence, comments, a 256 KiB upload/resume/finalize/download with digest checks and overwrite refusal, plus acknowledged invitation creation/retry/list/revocation. Moto is an emulator; production storage acceptance remains separate.

Reproduce the isolated CLI, SDK and storage check with:

```sh
cargo build --manifest-path app/Cargo.toml --bin todoer --bin todoer-server
uv run --no-project scripts/rebuild-agent-check.py
```

This script launches its own temporary development server/database and pinned Moto fixture on free loopback ports, strips inherited Todoer configuration, uses only synthetic credentials, and removes its processes/data afterward. It requires `uv`, a Python 3.11 runtime for Moto, and local socket access; Moto packages may be downloaded into uv's cache. Fixture HTTP logs are suppressed so temporary signed upload URLs are not recorded.

Documentation status: examples above describe the implemented compact interfaces
and their recorded checks, not a claim that every later HTML/REST operation has
agent parity. Consult the current OpenAPI/tool schemas and delivery ledger for
new move/review/assistance increments. Recorded SDK tests use the official Python
client and synthetic local services; real hosted connector, Google tenant and
production-storage acceptance remain separate.

## Bounded subtree pages, review, and moving a whole list

`todoer subtree TASK_ID` now calls `POST /api/v1/tasks/{id}/subtree/query`. It returns a page with `items`, `context`, `snapshot`, `has_more`, and `next_cursor`. The default is 50 items (maximum 100), with a 256-KiB response cap. IDs, parent IDs and depth are always present. Default optional fields are reference, title, completion, revision and position; descriptions require explicit projection. This changes the earlier CLI/MCP array response to a bounded page. Legacy REST `GET .../subtree` remains available for the existing browser flow.

```sh
todoer --json subtree TASK_ID --limit 25 --fields title,revision,complete
todoer --json subtree TASK_ID --limit 25 --fields title,revision,complete --cursor CURSOR
todoer --json subtree TASK_ID --fields title,description,revision
todoer --json review --from 2026-10-26 --to 2026-11-01 --list LIST_ID
todoer --json lists move-preview LIST_ID --destination-space DESTINATION_WORKSPACE_ID
```

`read_subtree` is the same bounded MCP page, with optional `limit`, `fields`, and `cursor`. Reuse the same root and projection when paging. Mutation makes a cursor stale (`resync_required`); restart rather than combining snapshots. Ancestor context is authorization checked and bounded, with explicit omitted/truncated markers. Extremely deep trees require reading a deeper root.

`todoer review` calls the implemented weekly-review REST endpoint. Omitted dates default to the current local week; `--space` and `--list` restrict the scope. It retains counting and coverage notes: completion counts distinct recorded leaf work, older history can be incomplete, planned-versus-completed uses current leaf schedules, and overdue trends contain actual observations rather than invented historical zeroes. MCP `schedule_tasks` adds `{action:{op:"review",from:"YYYY-MM-DD",to:"YYYY-MM-DD",list_id:null|ID,workspace_id:null|ID}}`; explicit dates are required there. The compact MCP/agent response clears upcoming task descriptions and retains the notes.

Whole-list moves use existing stable IDs and common authorization. Both workspaces require live owner access; a credential restricted to only one workspace/list cannot move the whole list. `manage_access` supports `list_move_preview` with `list_id` and `destination_workspace_id`, then `list_move` with `list_id` and an `input` object containing the preview's `destination_workspace_id`, `list_revision`, `source_workspace_revision`, `destination_workspace_revision`, and `policy_hash`. Expanded access additionally requires `acknowledge_access_change:true` after reviewing the full audience. The CLI apply command is `todoer lists move LIST_ID --body 'JSON'` with that same exact input. Never replace stale revisions blindly or infer acknowledgement from an old preview. List contents, sections, comments/files, reference IDs, and audit history remain under the common atomic move operation.

Optional assistance REST endpoints and their privacy/budget/review contract are documented in [assist.md](assist.md). Agent clients do not receive server provider keys or silently classify captures.

On 2026-10-26, the updated isolated official-SDK/CLI/Moto fixture passed with bounded projected subtree pages, a successful continuation and stale-cursor refusal, weekly review, owner list-workspace move, and the existing capture/bulk/organization/scheduling/discussion/attachment/invitation assertions. This is local interoperability evidence with synthetic data and Moto, not a production-provider or universal-client claim.
