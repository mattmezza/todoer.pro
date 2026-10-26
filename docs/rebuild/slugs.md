# Persisted display slugs

Workspace, list and section names retain their original human-readable `title`.
Migration 022 adds a separate `entity_slugs` table; it does not rename entities,
change task references, replace IDs or change existing ID-based URLs. UI clients
can use `Store::entity_slugs(actor)` to obtain authorized ID-to-slug maps and
render `@workspace`, `#list` and `$section`. This bulk read returns only entities
the current actor can read. Direct list guests receive no parent workspace slug,
ID, scope or name. Permission revocation is effective on the next read.

The canonical base lowercases Unicode text, retains letters and numbers, changes
runs of punctuation/whitespace into one ASCII hyphen, and trims leading/trailing
hyphens. It uses no transliteration or Unicode normalization dependency. For
example `My Next_Project` becomes `my-next-project`, while `CAFÉ 東京` becomes
`café-東京`. Emoji/punctuation-only names become `untitled`. The base is limited
to 64 Unicode characters. The stored slug is at most 80 characters and contains
no whitespace. Raw titles remain available for reading and rename editors.

Workspace collisions are scoped to the initial owner's account. This allocation
scope persists when ownership changes rather than silently renaming a shared
workspace. Lists are scoped to their workspace; legacy lists without a workspace
use their initial member account. Sections are scoped to their list. When the
same base is occupied, allocation selects the first available `-2`, `-3`, …
suffix, bounded to 100,000 candidates. Renames regenerate the slug in the current
scope; moving a list recalculates it in its destination scope. IDs and ordinary
revision/CAS checks remain authoritative; slugs are display names, not new lookup
or authorization keys. Existing URLs keep working after a rename or collision.

Normal workspace/list/section create and rename operations allocate slugs in the
same write transaction. A list workspace move changes its slug scope in the same
transaction as authorization, audience acknowledgement, task history and tag
mapping. Store startup runs incremental backfill within its migration transaction;
development seed and trusted JSON/legacy imports backfill within their write
transactions too. Backfill processes missing/changed entries by stable ID,
retains an unchanged entry's suffix, and never changes raw titles or revisions.
It does not run from read operations. Delete triggers remove stale allocations.
Direct SQL fixture writes outside these entry points should call backfill before
exposing data, or reopen through Store; such writes are not application APIs.

The maps deliberately avoid adding fields to every existing DTO constructor.
All REST, CLI and MCP writes already use these shared domain operations, so a UI
rename and an agent rename produce the same persisted slug behavior. Derived
slugs can be regenerated from trusted exported business rows; stable URLs and
references continue to use their original IDs, not the derived labels.
