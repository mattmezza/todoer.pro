# Structured search

The Rust query API uses SQLite FTS5 with Unicode token-prefix matching. It searches workspace and list titles/descriptions, section titles/descriptions, task titles/descriptions and references, individual comments, tags, and attachment filenames. It does not perform semantic search, typo correction, arbitrary substring matching, or fuzzy matching. `cafe` matches `café`, `Üb` matches `Über`, and `proposla` does not match `proposal`.

`POST /api/v1/query` accepts this JSON shape; omitted fields take the defaults shown:

```json
{
  "text": "proposal",
  "entities": ["task", "comment"],
  "status": "open",
  "limit": 50,
  "sort": "relevance"
}
```

Use your configured REST bearer credential. `GET /api/v1/query?text=proposal&status=open&limit=50` supports scalar filters. For arrays or projection, URL-encode the canonical JSON in the GET `query` parameter. JSON and scalar GET forms cannot be mixed. Browser and bearer requests use the same live authorization checks as other reads.

Supported entity names are `workspace`, `list`, `section`, `task`, `comment`, `tag`, and `attachment`. Omit `entities` to search all of them. Each comment and attachment has its own result, associated task, and breadcrumbs. Archived lists, deleted tasks and their descendants, deleted comments, and deleted attachments are excluded. Attachments on deleted comments are excluded.

Optional filters:

| Field | Meaning |
| --- | --- |
| `workspace_id`, `list_id` | Exact destination scope; no broader fallback |
| `section_id` | Section itself and associated tasks/comments/attachments; descendants inherit the root task's section |
| `tag` | Existing tag ID, or normalized exact name; duplicate accessible names require an ID or narrower scope |
| `status` | `open` (default), `completed`, or `all`; completion filters apply to task-associated results |
| `priority` | Integer 1, 2, or 3 |
| `planned_from`, `planned_to` | Inclusive date-only planning range |
| `due_from`, `due_to` | Inclusive due calendar-date range in each task's stored local timezone |
| `recurrence` | `none`, `any`, `active`, `paused`, or `stopped` |
| `fields` | Optional array drawn from `title`, `snippet`, `context`, `schedule`, `timestamps`; omitted means all |

Date filters use `YYYY-MM-DD`. A due time `2026-10-27T00:30:00` in `Europe/Zurich` belongs to October 27 even when its UTC date differs. Result schedule fields retain `due_timezone`. Unscheduled tasks do not match date or priority filters. Task-dependent filters exclude unrelated entity results. A tag filter selects associated tasks/comments/attachments; it does not list the tag entity itself.

`sort` accepts `relevance`, `title`, `created`, `updated`, `planned`, `due`, or `id`; all sort ascending with stable entity-kind/ID ties. Relevance puts exact titles first, then title prefixes, other literal title matches, and description/comment matches. FTS5 selects literal token-prefix candidates, but ranking uses authorized document-local title weights, avoiding global corpus statistics that could make private documents affect ordering. Title ordering uses SQLite `lower()`; full locale collation is not implemented. Missing planned/due dates sort last. Earlier core tables did not retain creation/update timestamps: existing rows receive null timestamps at index migration, newly inserted rows record creation time, and subsequent writes record update time. Unknown timestamps sort before known ones. These are search-observed timestamps, not recovered historical audit times.

The response contains `hits`, the authorized matching `total`, and nullable `next_cursor`. IDs, kinds, association IDs, and completion state remain available with every projection. Excerpts are plain text bounded to about 220 Unicode characters, with optional ellipses; clients must render them as text. Description bodies and object storage keys are never returned by this API. Breadcrumbs include only accessible workspace, list, inherited section, and parent task titles. They include at most 16 nearest ancestors and bound each title to 120 characters; `context_truncated` signals a shortened path or title. Direct list guests do not receive an inaccessible workspace's name.

Pass `next_cursor` back unchanged in the same query to request another page. Cursors bind the canonical query (including projection, limit and sort), actor, current accessible scope, and an opaque index generation. Index changes, filter changes, or permission changes return HTTP 409 with `resync_required`; restart without a cursor. This conservative rule also invalidates pagination after unrelated indexed writes. It prevents silent skips, duplicates, or stale-scope continuations. A cursor is an opaque continuation, not an authorization grant.

Search text is limited to 500 characters; result pages contain 1–100 hits. FTS operators, quotes, punctuation and reference prefixes never become executable query syntax. `%T-...`, full task IDs, and unique-looking task ID prefixes of at least eight characters can match task references; multiple accessible matching prefixes remain separate results. Exact identifiers are preferable for subsequent writes. Text tokens are combined with AND; at most 32 tokens are accepted. The API does not interpret reference names as authority to read otherwise inaccessible entities.

The index backfills existing entities during migration 016 and maintains one FTS document per source row through SQLite triggers. Live source joins apply authorization and deletion state before counts, pagination and result construction. Scope, scheduling, tag, section and recurrence mutations invalidate cursors. Search does not concatenate every comment into a task document or load private result bodies for post-filtering.

The domain tests cover authorization, live revocation, counts/excerpts, Unicode prefixes, literal operators, references, typo boundaries, inherited sections, direct guest breadcrumbs, completion, date/timezone/priority/recurrence filters, projection, tag ambiguity, and cursor invalidation. An ignored synthetic test builds 10,000 tasks plus 10,000 independent comments and measures an actual authorized query:

```sh
cargo test --manifest-path rebuild/Cargo.toml search::tests::ten_thousand_tasks_and_comments -- --ignored --nocapture
```

Saved filter storage, a visual filter builder, and a new HTML results layout are not provided by this module. Clients can reuse the same query document directly.

Observed locally on 2026-10-26: the sequential debug-build fixture returned 100 full-context comment hits out of 10,000 authorized matches across 10,000 tasks plus 10,000 comments in **705 ms**. The fixture, including indexed insertion, completed in roughly six seconds; the full sequential search test selection completed in 7.5 seconds. This is a synthetic development measurement, not a release-build latency guarantee or a production load test.

Quick search exposes a copy-reference action beside each result. The optional
`reference` projection returns the canonical prefixed identifier (`%task`,
`#list`, `@space`, `$section`, `+tag`); it is included by default. Comments and
attachments use their parent task's reference. Copying keeps quick search open
and preserves keyboard navigation.
