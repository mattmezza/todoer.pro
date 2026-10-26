# REST client workflow

The Rust rebuild provides the `todoer` CLI as a REST client. Run it from the repository with `cargo run --manifest-path rebuild/Cargo.toml --bin todoer -- ...` or install/build the binary and use the examples below.

Set `TODOER_TOKEN` to your server-issued bearer credential. `TODOER_URL` defaults to `http://127.0.0.1:8092`; `--url` overrides it. Keep tokens out of shell history and source control. The CLI does not print the token.

The current server token adapter is development-only. It is not a production authentication or self-hosting authentication solution. Remote MCP has not yet been delivered; the workflow described here uses REST directly.

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

Verified against the local synthetic development server: initialization, all eight tool registrations, destination resolution, identical capture retry, and subtree read passed. SDK 2.3.0 currently logs `Session termination failed: 202` when this rmcp transport acknowledges session cleanup; the request assertions complete and the check exits successfully.
