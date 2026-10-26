# Tags, sections, task ordering and references

Tags have opaque UUIDs, display names and revisions. Their home is the task's workspace, or its list for legacy lists without a workspace. Names are trimmed and compared with Rust Unicode lowercase; this is not Unicode NFC normalization or locale-specific case folding. A name contains 1–100 non-control characters. A task accepts at most 50 tags, supplied as names or explicit `{ "id": "TAG_UUID" }` selectors. Duplicate normalized names share one tag in a home. Task tag changes require the current task revision. Renaming a workspace tag requires workspace editing permission; a guest's access to one shared list does not grant that authority.

A guest sees only tags attached to tasks they can currently read. They cannot enumerate the parent workspace's other tags. Cross-workspace moves remap tags by normalized name into the destination home and record old/new tag IDs in `tag_move_audit`. The move preserves display names without exposing the source home. Existing destination names are reused. The move hook runs inside the same transaction and clears section assignment.

Section assignment applies to a root and its whole active subtree. Descendants inherit the root section, including children created later. Assigning a child independently is rejected. A section must belong to the task's list. Same-list reparenting rejects cycles, inherits the new parent's section and reopens a completed new ancestor when necessary. Cross-list moves use the existing subtree move operation and its access-change acknowledgement.

Sibling ordering takes a list revision and the exact set of active sibling IDs. Missing, extra or repeated IDs fail; stale list revisions conflict. The transaction bumps the list revision, updates sibling positions and revisions, and writes task history. Reparenting and section assignment likewise require the current task revision.

Reference prefixes are:

| Prefix | Target | Example |
| --- | --- | --- |
| `@` | Workspace | `@[Weekly planning]` |
| `#` | List | `#Groceries` or `#LIST_UUID` |
| `$` | Section | `$[Next steps]` |
| `%` | Task | `%T-existing-reference`, `%TASK_UUID`, or an unambiguous UUID prefix of at least eight characters |
| `+` | Tag | `+urgent` or `+[high priority]` |

Bare names use letters, digits, underscores and hyphens; brackets allow spaces and other literal name characters. Trailing punctuation remains ordinary text. Code spans, fenced code blocks, existing Markdown links/images, escaped prefixes, email addresses and prefixes embedded in words do not become references. Parsing uses CommonMark events and source ranges. Rendering injects typed link/text events and sanitizes the resulting HTML with the same image-removal policy as ordinary Markdown.

Resolution checks live authorization before returning any candidate. Exact opaque IDs and full task references identify their targets directly. Duplicate names remain plain text and return accessible choices; the system does not pick a target by ordering. Autocomplete returns at most 30 choices and examines at most 1000 candidates per entity kind. Name resolution shares that bounded candidate window; an explicit full ID can resolve targets outside it. Each description or comment indexes at most 200 reference occurrences. Use full IDs for reliable disambiguation in large collections.

`reference_edges` stores source kind/ID, occurrence index, target kind/ID and the original token. `reference_sources` stores the exact content hash. The application indexes saved task descriptions and comment bodies inside their write transaction. An unchanged hash preserves every existing binding: renaming or moving an entity never silently redirects a link to a different entity with the old name. Unbound occurrences can return fresh accessible choices for explicit review without being automatically bound.

Rendering requires the saved source text and a matching hash, then checks each target's current visibility. It displays the current target label and URL. Deleted, inaccessible or unavailable targets fall back to the original token as plain text; their current title is not revealed. Unindexed text uses ordinary sanitized Markdown. Backlinks return source kind/ID plus the authorized `source_task_id`, require access to the target and each current, non-deleted source, and ignore stale source hashes. Listing backlinks is capped at 100 visible sources.

The domain tests exercise Unicode names and collisions, revision conflicts, shared-list privacy, scope remapping and audit records, section inheritance, cycle rejection, sibling ordering, CommonMark exclusions, bracketed names, duplicate ambiguity, stable links after renames, hidden/deleted targets, safe HTML labels and authorized backlinks. Run `cargo test --locked taxonomy::tests --lib`.

REST exposes the same operations through `/api/v1/lists/{id}/tags`, `/api/v1/tags/{id}`, task `/tags`, `/section`, `/reparent`, and list `/task-order` endpoints. Compact typed actions are available at `POST /api/v1/organize`. Workspace members can list the tag registry; direct list guests see only tags currently used in that list. Standalone workspace-tag creation requires workspace editing permission; a direct guest can create and assign a name in an authorized task update without enumerating unused tags.

Reference resolve/autocomplete, source unresolved occurrences, explicit binding, and target backlinks are exposed under `/api/v1/references`, with a compact action endpoint at `POST /api/v1/references`. Resolve always returns a structured `choices` array, including zero/multiple choices. Explicit binding supplies the current saved source revision, occurrence index, and chosen target kind/ID. The target must remain an accessible exact candidate. A successful bind increments the task or comment revision, keeps the saved text unchanged, and preserves other bindings; stale requests conflict. Task source IDs can be opaque IDs or full task references. The CLI and MCP schemas document the same actions; no adapter implements separate mutation rules.
