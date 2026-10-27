# Short URLs and first-capture defaults

Migration 24 is additive. It keeps every existing primary key and reference,
and assigns persistent public aliases to existing spaces, lists, sections and
tasks. New entity IDs use a type prefix and 12 URL-safe characters (`s_`, `l_`,
`c_`, `t_`); IDs are random, independent of names, and checked for collisions.
New users/comments/files also receive short opaque IDs. Authentication tokens
and storage object keys retain their separate security-oriented formats.

Canonical browser routes are `/space/<short-id>`, `/lists/<short-id>` and
`/tasks/<short-id>`. Old workspace/list/task UUID links redirect after a live
permission check. An alias is an identifier, never an authorization grant.
Existing API primary IDs remain valid; new records return short primary IDs.

A newly verified user is provisioned in the identity transaction with a private
Personal space, an Inbox list, a hidden Main section and an explicit Inbox
capture preference. A new space gets an Inbox and hidden Main; every new list
gets hidden Main. A failure rolls back the entire provisioning transaction.
Space navigation renders its default list directly, with the space URL kept in
the address bar. Named sections remain optional; the default section stays out
of section headings, selectors and manual section ordering. Root captures use
its stored location; nested tasks inherit their effective named section.

Existing data gets hidden Main sections and uses an existing unrestricted,
unarchived Inbox if available. Otherwise migration adds an empty Inbox. It
never moves existing tasks between lists. Each existing account selects an
owned Personal/Inbox space when available, otherwise an owned active space,
otherwise a new Personal space. Existing explicit capture preferences remain.
Defaults and public aliases are included in authorized exports/imports.

Space links open the default list. Space settings are available in the list's
context menu as a tabbed panel (Details, Sharing, Lists), using the same native
form authorization and revision checks as the standalone management endpoint.
The panel becomes a stacked screen on mobile. Direct list guests do not gain
access to their hidden parent space.

Verification: 205 library tests passed, including transactional provisioning
failure, verified identity rollback, first capture, retry idempotency, persistent
aliases and permission isolation. Browser/local migration checks are recorded
in the implementation ledger after exercising the delivered build.

Task URLs now use a persisted, seven-character, prefix-free public ID. The ID
shown after `%` is identical to the URL segment. Migration 026 stores these in
`task_public_ids`; previous public aliases and UUID URLs continue to resolve and
redirect to the compact URL. Reference lookup and search accept the compact ID.
Exports include the mapping, so restored tasks retain their URLs. Database IDs
and existing saved references remain unchanged.
