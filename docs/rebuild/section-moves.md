# Reviewed section moves

`Store::section_move_preview(actor, section_id, destination_list_id)` returns a
bounded `SectionMovePreview`. `Store::move_section(actor, section_id,
SectionMoveInput)` applies that exact reviewed plan. The input supplies destination
list ID, expected section/source-list/destination-list revisions, `policy_hash`,
and explicit `acknowledge_access_change` (default false).

The caller needs current write permission on both lists. Previews contain only
section/list IDs, revisions, affected/assigned task counts, the number of other
section assignments that will be cleared, audience member counts, a broadening
flag and opaque policy hash. They do not enumerate member identities or disclose
inaccessible workspace metadata. The hash binds the actor, section/list metadata,
all affected task identities/revisions/parents/trash/section state, current user
audiences, relevant source/destination tag identities/revisions, and both lists'
history watermarks. Descendant edits, new descendants, assignment changes,
authorization changes and relevant tag changes require a fresh preview. Permission
loss fails immediately. New destination readers require explicit acknowledgement,
even for an empty section whose title/description will become visible there.

Moving transfers the section and the complete subtrees of its assigned tasks.
Overlapping assigned subtrees are processed once. The operation uses one outer
transaction and the ordinary authorized subtree mover for each root; failure on
a later root rolls back earlier roots, history, tag changes and section/slugs.
Root ordering follows source position/ID and appends to the destination. The
section appends to destination sections, preserves its ID/title/description and
bumps its revision; source/destination list revisions also advance. Its persisted
slug is reallocated in destination-list scope, with normal collision suffixes.

Task IDs/references, nested parent links, task content, completion/trash state,
comments, attachment metadata/object keys and history remain intact. Root tasks
whose parent is outside the moved group detach, as with an ordinary subtree
move. Tag scopes use the ordinary subtree move's mapping rules. Only assignments
to this moved section are restored. If a descendant had an assignment to a
different section, that source section stays behind and the descendant becomes
unsectioned; the preview's `other_section_assignments_cleared` makes this explicit.
Ordinary section assignment already applies to a whole subtree, so differing
nested assignments chiefly concern retained imported/legacy structures.

Live parent subtrees carry hidden descendants safely, and hidden descendants
retain membership in the moved section for later restoration. A standalone
trashed root cannot use the live subtree mover; the operation explicitly rejects
it with instructions to restore before moving, rather than leaving cross-list
section references. Same-list destinations are rejected. A section move is
limited to 500 affected tasks including hidden descendants; larger groups must
be split first. Cyclic or cross-list task ancestry fails explicitly.

This operation currently requires an outer Store connection, matching reviewed
bulk-operation transaction ownership. It is not a nested operation inside another
reviewed bulk plan. It performs no object-storage requests and neither copies nor
deletes file bytes: attachments continue referencing the same immutable objects.
Tests use synthetic metadata to verify identity/key preservation, not an external
object-storage transfer claim.

## Move a section's tasks without moving the section

The section menu's **Move all tasks to…** opens a destination-list picker with an
optional section. The destination may be another section in the same list, another
list's section, or the destination list without a section. Completed tasks and all
nested descendants move together. The source section keeps its identity, title,
description and position, and the browser stays on the current list after success.

POST `/api/v1/sections/{id}/tasks/move/preview` accepts
`{"destination_list_id":"…","destination_section_id":null}` (or a section ID).
It returns the existing section-move snapshot fields plus `root_task_count` and
`destination_section_id`. POST `/api/v1/sections/{id}/tasks/move` accepts the same
destination fields, the returned `policy_hash`, and
`acknowledge_access_change` (default false). Cookie requests require CSRF, and
unknown JSON fields are rejected. The browser supplies acknowledgment when the
user selects Move, consistently with the other move dialogs.

The contents-only plan also binds the destination section and its revision and
uses a distinct operation hash. It checks current write access on both active
lists, rejects sections outside the destination list and the source section as its
own destination, and recomputes the plan under one immediate transaction. A
concurrent subtree, permission, section or list change rejects the stale plan.
A later-root failure rolls back all earlier moves and history. It uses the same
500-affected-task bound and trashed-standalone-root checks described above, without
the generic bulk API's 50-operation limit. Hidden descendants retain their trash
state and receive the destination section too. Task IDs, nested parent links,
comments, files and tag remapping follow the existing subtree mover.

Domain and HTTP tests cover cross-list and same-list destinations, optional section,
completed and hidden descendants, more than 50 roots, stale plans, permissions,
access acknowledgment, strict fields, CSRF and rollback. Browser checks cover the
phone dialog, each destination type, preservation of the source section, completed
tasks and subtasks, and retrying a rejected move.
