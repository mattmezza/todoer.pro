# Migration and export

## Source safety

The importer opens the legacy database read-only, runs SQLite's integrity check,
and uses SQLite's online backup API to build a consistent in-memory snapshot.
It does not migrate, delete, vacuum or write into the legacy database. Keep an
independent backup before applying an import. For a live SQLite database, use its
backup API or `.backup` command rather than copying only its main file while WAL
writers are active. The old application and its object bucket remain intact.

The actual legacy schema was inspected using schema queries only. Personal data
is not included in committed fixtures, examples, screenshots or command logs.
The optional real-copy validation test logs no records. All checked-in fixtures
are synthetic.

## Preview, then apply

```sh
cargo run -p todoer --bin todoer-migrate -- legacy \
  --source /private/legacy-copy.db --destination /private/rust.db

# Explicit application; use the same mapping for future reconciliations.
cargo run -p todoer --bin todoer-migrate -- legacy \
  --source /private/legacy-copy.db --destination /private/rust.db \
  --identity-map /private/identity-map.json --apply
```

Dry-run is the default. It validates the complete transaction in memory and
rolls it back. Existing destination data is copied read-only for a dry run; the
source and destination must be independent paths, not symlinks or hard links to
the same file. `--apply` alone authorizes writes to the separate Rust destination.
Reports contain counts, validation categories and credential-exclusion counts,
not task titles, descriptions, filenames or email addresses.

Identity maps have this form:

```json
{"users":{"OLD-USER-UUID":"EXISTING-RUST-USER-ID"}}
```

A flat old-ID-to-new-ID JSON object is also accepted. Targets must already exist
in the destination. Emails never match or merge identities automatically. Without
a mapping, users receive a `legacy:` identity with no authentication identity or
session, so they cannot sign in. To adopt real accounts, choose the explicit
mapping before first apply, or repeat into a fresh independent destination.
Changing a mapping on an existing imported source is a conflict, not an implicit
permission grant. Existing Rust edits are never overwritten by a reconciliation.

If the dry-run reports missing task sections, pass `--recovery-owner OLD-USER-UUID`
to preview recovery into a private workspace for that explicitly chosen legacy
user. The report counts missing sections and affected root tasks. Applying recovery
also requires that user to be explicitly mapped to an existing Rust account and
`--ack-unsupported` after reviewing every reported category. The original section
identifiers are retained; the original list and ownership cannot be inferred.
Use the same recovery owner and mapping on subsequent runs.

## Preserved data

Workspaces, lists, sections, task IDs, short references, sibling positions,
descriptions, completion state, nested subtasks, date-only due dates, priorities,
tags, comments and attachment references are materialized. Legacy archived tasks
become trash and retain their source archive flag in metadata. Archived spaces
keep their lists archived. Default flags, colors, sorting preferences, original
creation/update/completion timestamps, bookmarks, audit records, notification
read state, nonsecret settings and migration shadow tables are retained verbatim
in sanitized legacy metadata even where the new UI has no matching field.

Old migration shadow tables are archives. Only rows still referenced by live
tasks are materialized; unused historical rows remain in the metadata archive.
Every recognized sanitized source row is reconciled against an archived record.
Source and destination fingerprints detect source drift, changed mappings,
removed imported entities and concurrent edits. Re-running unchanged input is
safe. Updates that require reconciliation fail transactionally; no changed row
is silently replaced. Cycles, missing task parents, unsupported priorities/dates and invalid foreign-key
relationships fail validation. Recoverable missing hierarchy is handled explicitly
as described below, with acknowledgement required before apply.

Passwords, confirmation codes, TOTP material, AI API keys and unknown settings
values are excluded. Unknown tables are listed as unsupported and block apply
unless the operator explicitly passes `--ack-unsupported`; those tables are not
imported by that acknowledgement. Preserve the original backup for any future
conversion of unsupported structures. No source row is printed to diagnose it.

Legacy attachment keys are the original UUID object keys, not signed URLs.
Their filename, MIME, size and source timestamps remain in
`legacy_attachment_refs`. The importer does not claim an original checksum that
did not exist. A separately configured read-only legacy blob adapter verifies
size and computes a fresh baseline digest when a ZIP is created.

## Authorized exports

```sh
cargo run -p todoer --bin todoer-migrate -- export \
  --database /private/rust.db --actor EXISTING-USER-ID \
  --format json --output /private/export.json
```

Choose `--list ID` or `--workspace ID` to narrow the export; otherwise it covers
currently readable lists. The CLI is an administrative local tool. A future HTTP
adapter must derive actor identity from authentication, never a request-supplied
user ID. Exports use a consistent snapshot and current shared authorization
views before including data. ACLs appear only for owned workspaces. A directly
shared list does not disclose its original workspace title or other recipients.
Sessions, auth identities, Google claims, token hashes, invitation secrets,
provider secrets and delivery queues are never exported.

JSON format version 1 contains business tables plus lossless sanitized legacy
metadata. Plaintext is a readable task tree with IDs, completion status and
descriptions. CSV is a flat task export with explicit task/list/parent IDs,
position and revision; it does not encode sections, comments, attachments, tags,
ACLs or recurrence rules. Formula-like CSV cells are prefixed with an apostrophe;
newlines, quotes and delimiters use the maintained CSV encoder. Use JSON for a
full-fidelity metadata restore.

## JSON restore

```sh
cargo run -p todoer --bin todoer-migrate -- restore \
  --source /private/export.json --destination /private/restored.db \
  --identity-map /private/identity-map.json
# Add --apply only after reviewing the dry-run counts.
```

Only the versioned business-table allowlist is accepted. Mapping targets must
already exist. Unmapped users become disabled `import:` identities. Exported ACL
roles are preserved only through these explicit identities. Guest exports with
no disclosed owner ACL become private copied workspaces owned by the mapped
export recipient. Untracked existing entity IDs cause a conflict, preventing a
restore from granting access to an unrelated existing workspace. Repeating the
same bundle and mapping is safe; newer edits cause a conflict. Authentication
state and operational notification jobs are never imported.

## ZIP backup

Use `--format zip` with the normal private attachment configuration and, for old
blobs, separate `TODOER_LEGACY_STORAGE_*` configuration. ZIP files contain
`business.json`, attachment files named by safe UUIDs, and `manifest.json` with
MIME, byte length, SHA-256 and verification provenance. Legacy digests are labelled
as a first-capture baseline. New objects are checked against their stored digest.
Empty files are valid. Downloads are limited to 25 MiB per object and performed
one at a time; oversized, missing or unverifiable objects fail the entire export.
No partial archive is published. Business JSON is limited to 100 MiB.

Output files are created exclusively with mode 0600 on Unix. Existing files and
symlinks are never overwritten. Archive names do not use user filenames, so
`../` and path separators in filenames cannot escape the archive. Verification
checks every declared file without extracting it and rejects undeclared files.
The manifest excludes itself from its checksum list. JSON restore restores
metadata, not S3 bytes; ZIP restore verifies and restores those bytes using its
manifest as described below. No bucket object is mutated by export.

Legacy orphan hierarchy is recovered only for an existing source creator. Missing
lists receive a clearly named placeholder; lists with missing workspaces go into
a deterministic private recovery workspace owned by that creator. Original source
parent IDs remain in the sanitized archive. Dry-run reports recovery counts and
apply requires `--ack-unsupported`; deleted creator UUIDs are preserved as disabled local placeholder accounts, also requiring explicit acknowledgement. Non-UUID ownership references block import. Derived
reference indexes are excluded; original reference text is preserved.

## Complete ZIP restore

`todoer-migrate restore --source export.zip --destination restored.db` validates
without writing either storage or database. Add `--apply` only after reviewing
the report. ZIP is detected by its file signature. Apply uses the normal private
`TODOER_STORAGE_*` destination configuration, never the legacy source credentials.

The complete archive manifest, checksums, canonical attachment IDs, declared
sizes, metadata relationships and destination conflicts are checked before any
upload. Restores accept at most 10,000 manifest entries, 512 MiB of total declared
content, 100 MiB business JSON and 25 MiB per attachment. Empty objects work.
Every attachment must have exactly one matching archive entry; extra or missing
object files fail validation. Native metadata digests must match the manifest.

Objects use deterministic private restore keys derived from the bundle digest
and attachment UUID, not archive-supplied object keys or filenames. Uploads and
promotion use conditional creation and verify checksums, including existing
objects on retries. A different existing object is never overwritten. Original
attachment IDs remain stable. Legacy attachments become normal verified native
attachments; their original MIME, object key and metadata remain in sanitized
source records. Imported creation attribution for those legacy files is the
mapped export recipient.

The database commits only after all object staging and promotion checks succeed.
An interrupted restore can leave immutable objects with no committed metadata;
retrying the same archive safely reuses verified bytes. Database edits racing
with restore still cause the final metadata transaction to fail. There is no
cross-system rollback or deletion of preexisting objects. Retain source archives;
review unreachable restore objects separately before any retention cleanup.
JSON restore continues to restore metadata only; use ZIP for usable attachment
bytes. ZIP restore is covered by a local HTTP object-store roundtrip test that
retrieves the imported bytes, repeats conditional writes and rejects a corrupt
existing object without committing metadata or overwriting that object.

## HTTP transfers

Authenticated `GET /api/v1/export?format=json|text|csv|zip` exports the readable
account. Add either `list=ID` or `workspace=ID` to narrow it. Responses download
fixed filenames, use `Cache-Control: no-store, private` and `nosniff`, and stream
from exclusive mode-0600 temporary files which are removed after response or
cancellation. ZIP requires configured private storage, plus separate legacy blob
configuration when old attachment references are present.

`POST /api/v1/import` accepts a raw `application/zip` or `application/json` body.
It defaults to validation only. `?apply=true` explicitly applies. Browser session,
same-origin validation and `X-CSRF-Token` are required; bearer credentials cannot
import. Only the export recipient maps to the signed-in account. Other source
users stay separate disabled identities, and existing entity IDs or changed rows
are never implicitly overwritten. HTTP JSON with attachments is rejected: use
ZIP so attachment metadata cannot be restored without its bytes. Uploads stream
to private temporary files with a 512 MiB ZIP / 100 MiB JSON limit. No session,
provider token, password or delivery credential is included in exports.
