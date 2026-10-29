CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE workspace_members (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL CHECK(role IN ('owner','editor','viewer')),
    PRIMARY KEY(workspace_id,user_id)
);
CREATE INDEX workspace_members_user ON workspace_members(user_id,workspace_id);
ALTER TABLE lists ADD COLUMN workspace_id TEXT REFERENCES workspaces(id);
ALTER TABLE lists ADD COLUMN restricted INTEGER NOT NULL DEFAULT 0 CHECK(restricted IN (0,1));
ALTER TABLE lists ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE lists ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1));
CREATE INDEX lists_workspace_position ON lists(workspace_id,position,id);
-- Keep legacy memberships untouched. Missing role rows mean legacy editor.
CREATE TABLE membership_roles (
    list_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('viewer','editor')),
    PRIMARY KEY(list_id,user_id),
    FOREIGN KEY(list_id,user_id) REFERENCES memberships(list_id,user_id) ON DELETE CASCADE
);
CREATE TABLE sections (
    id TEXT PRIMARY KEY,
    list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX sections_list_position ON sections(list_id,position,id);
CREATE VIEW effective_list_access AS
SELECT list_id,user_id,MAX(rank) AS rank FROM (
    SELECT m.list_id,m.user_id,
        CASE COALESCE(r.role,'editor') WHEN 'viewer' THEN 1 ELSE 2 END AS rank
    FROM memberships m LEFT JOIN membership_roles r
        ON r.list_id=m.list_id AND r.user_id=m.user_id
    UNION ALL
    SELECT l.id AS list_id,w.user_id,
        CASE w.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END AS rank
    FROM lists l JOIN workspace_members w ON w.workspace_id=l.workspace_id
    WHERE l.restricted=0 OR w.role='owner'
) GROUP BY list_id,user_id;
