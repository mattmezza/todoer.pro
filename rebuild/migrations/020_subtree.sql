-- Local watermark and bounded child/sibling seeks for projected subtree reads.
CREATE INDEX history_list_seq ON history(list_id,seq);
CREATE INDEX tasks_subtree_live_order ON tasks(parent_id,position,id) WHERE deleted_at IS NULL;
