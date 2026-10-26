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
