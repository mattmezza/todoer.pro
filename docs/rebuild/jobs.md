# Account job status

`/settings/jobs` is a read-only browser page; `/api/v1/jobs` exposes the same
bounded metadata to a browser session or an account-wide admin REST credential.
Read/write credentials and any list- or workspace-bounded credential are rejected,
even when the bounded credential includes admin. Authentication and the common
live credential lookup run for each request. Revoked/expired credentials and
revoked browser sessions cannot read the page or API.

The view includes only reminder jobs whose recipient is the current account,
email/push digest jobs owned by that account, and invitation delivery jobs whose
inviter is that account. It does not enumerate other accounts or grant a workspace
owner access to other members' notification queues. No task title/description,
recipient address, invitation token, transport endpoint, device key, lease token,
lease owner or raw provider error is selected for the response.

Rows show queue/job ID, kind, channel, status, attempts, available time and lease
expiry. Push rows additionally show pending/sent/dead device delivery counts and
the number with a recorded failure. A parent can still be pending while some
devices have succeeded; the device counts make partial failures visible. Failure
codes come from a fixed safe mapping. Unknown stored error text becomes
`delivery_failed_or_ambiguous`; it is never rendered or returned. Dead rows with
no recorded error use `retry_exhausted`. A provider acceptance status is not proof
of actual user receipt, nor an exactly-once delivery guarantee.

Optional query parameters are `limit` (1–100, default 25), `status` (pending,
leased, delivered, sent, cancelled, dead or failed), and `cursor`. Empty/absent status
shows all states. `failed` includes recorded parent errors, stopped parent jobs and
partial push-device failures even when the parent is still pending or accepted. Pagination orders by queue source then stable job ID, not by
newest timestamp. Cursors bind the account and status filter. Changing a filter
starts a new traversal. Reads are consistent within each response; later pages
reflect live queue changes rather than a retained historical snapshot. The page
has native filters, pagination and refresh links without JavaScript. Responses
use `Cache-Control: no-store, private`.

Pending may mean a scheduled time, quiet hours or transport retry; the page does
not infer a provider outage from a pending row. Leased means a worker owns the job
until the displayed expiry; an expired lease can be reclaimed by the existing
worker. This feature performs no mutation, purge, forced resend, retry-all or
provider request. Correct notification preferences, storage/SMTP/VAPID service
configuration or expired device subscriptions through their existing settings;
operational repair and retention policies remain explicit administrator work.

## Verification

Synthetic Store tests cover account separation, bounded/account-bound keyset
pagination, status filtering and safe failure mapping. Actual Axum router tests
use production-mode sessions/hashed credentials and a temporary synthetic SQLite
database to check private-account isolation, no transport/error disclosure,
cache headers, list-scope/read-only credential rejection, admin access, credential
revocation and session logout. No external provider or personal database is used.

Observed verification: `cargo test --manifest-path app/Cargo.toml jobs --lib
-- --nocapture` passed five tests (the four new domain/router tests and the
existing durable lease test). `cargo clippy --manifest-path app/Cargo.toml
--all-targets -- -D warnings` passed. This verifies the local data/router contract;
physical-device delivery and production-provider health are not inferred from
this page or these tests.
