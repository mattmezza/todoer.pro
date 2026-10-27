# Projected subtree reads

`Store::read_subtree(actor, root_id_or_reference, SubtreeRead)` returns a flat preorder page. Every item includes `id`, `parent_id` and relative `depth`; the selected root has depth zero. Siblings sort by signed `position`, then binary task ID. Default projection is reference, title, complete, revision and position. Description is opt-in. An empty field array requests only structural fields. Fields are normalized to a sorted set. Limit defaults to 50 and accepts 1–100.

`SubtreePage` contains root/list IDs, a snapshot, normalized fields, ancestor context, items, `has_more` and `next_cursor`. Context contains stable task IDs/references, revisions, optional titles and parents; it contains no workspace metadata or descriptions. Ancestor context is limited to 64 ancestors and 32 KiB, retaining the nearest ancestors/root. `truncated` and `before_id` indicate older context; a title over 4096 bytes is omitted with `title_omitted=true`. This context limit never silently removes subtree items.

Pass the returned cursor with the same root and projection. Limit may change. Cursors are versioned opaque base64 JSON, not authentication credentials: every request checks current list permission before validating the cursor. Their snapshot binds root revision, list revision and the maximum history sequence **for this list**. Any normal task mutation in that list conservatively invalidates the cursor with a conflict requiring a restart, including edits outside this subtree. Activity in another list does not invalidate it. A root move to a readable list requires a restart; a move to an inaccessible list fails authorization. Direct database writes that bypass Store history are unsupported. Pagination is a consistent read transaction per page, not a transaction held between requests.

The complete serialized page is at most 256 KiB, including context and cursor. The reader may return fewer than the requested limit and `has_more=true` to respect this budget. Its cursor always points to the last returned item: no item is skipped to make a page fit. A single oversized projection fails explicitly; retry without large fields. Normal 100 KB descriptions fit individually. Text with heavy JSON escaping may require a smaller projection. Large unselected descriptions are not loaded. SQLite byte-length metadata checks reject obviously oversized selected text before JSON construction.

Relative traversal depth is limited to 256 and ordering paths to 64 KiB. Encountering deeper data fails explicitly with instructions to select a deeper root. This is a read limit, not a restriction or truncation of stored hierarchy. Invalid cross-list/cyclic hierarchy fails rather than returning another list's content. Cursor input is bounded to 128 KiB and ancestry is verified rather than trusting a supplied traversal path.

Traversal seeks only the first child and next sibling at each step using the live parent/position/ID index. Recursive queue ordering yields preorder without materializing all descendants or all children of a wide parent. Resume seeds are the last node's first child and the next sibling at each ancestor level, bounded by depth. Metadata ancestry/context queries have fixed bounds; there is no per-item database query from Rust. Selected task JSON is constructed only for the bounded page candidates. At most `limit+1` candidates are emitted by the walk; the extra row detects continuation. Queue size depends on hierarchy depth, not subtree width.

Implementation references: SQLite documents [recursive queue ordering and LIMIT](https://www.sqlite.org/lang_with.html) and [octet_length metadata behavior](https://www.sqlite.org/lang_corefunc.html). The ordinary complete-subtree mutation API remains unchanged; this reader is for bounded agent/API inspection.

## Move up one level

`POST /api/v1/tasks/{id}/promote` accepts `{ "snapshot": ... }` from a subtree
read. It promotes the selected task to its current parent's parent, or to list
top level, appending after existing siblings while retaining all descendants.
Write access is required. Confirmation checks the root revision, list revision
and list history sequence in the same transaction as the move; changes since
the preview return 409. Top-level tasks cannot be promoted.

The Structure tab appears on sub-tasks and shows a read-only, paginated tree.
Move to parent actions live in task menus and preview the complete moved subtree
before confirmation.
