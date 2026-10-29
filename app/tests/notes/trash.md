# Trash grouping and pagination

The Trash page pages deletion roots (25 operations plus one lookahead row), ordered
by deletion time and ID. Descendants sharing the root's delete_group render as an
indented tree in that card and have no separate Restore button. The original
visible workspace, list and ancestor titles appear above the tree. Workspace
context uses list_info's authorization-aware projection; ancestors must belong to
the authorized list. Readers see cards but cannot restore; writers can restore the
operation using the existing revision-checked, transactional endpoint.

Restoration retains the original list, section, parent and position, and restores
only members deleted by that operation. A descendant deleted independently before
its ancestor remains a separate operation and requires the parent to be restored
first. This preserves the existing undo semantics. Following a successful restore,
the page reloads so pagination and blocked child operations are refreshed.

No migration or dependency was added. The existing flat API/MCP trash response is
unchanged. Tests: `cargo test -p todoer --lib trash_` (three passed), including
27 deletion roots over two pages, a three-level tree, permission isolation, atomic
restore groups and export roundtrips containing trash.
