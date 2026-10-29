CREATE TABLE workspace_lifecycle (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
 archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN(0,1)),
 deleted_at TEXT
);
ALTER TABLE lists ADD COLUMN color_hex TEXT CHECK(color_hex IS NULL OR (length(color_hex)=7 AND color_hex GLOB '#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]'));
-- Trash revokes every path, including existing direct list grants and tokens.
DROP VIEW effective_list_access;
CREATE VIEW effective_list_access AS
SELECT list_id,user_id,MAX(rank) AS rank FROM (
 SELECT m.list_id,m.user_id,CASE COALESCE(r.role,'editor') WHEN 'viewer' THEN 1 ELSE 2 END AS rank
 FROM memberships m LEFT JOIN membership_roles r ON r.list_id=m.list_id AND r.user_id=m.user_id
 UNION ALL
 SELECT l.id,w.user_id,CASE w.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END
 FROM lists l JOIN workspace_members w ON w.workspace_id=l.workspace_id WHERE l.restricted=0 OR w.role='owner'
) grants WHERE NOT EXISTS (SELECT 1 FROM lists l JOIN workspace_lifecycle x ON x.workspace_id=l.workspace_id WHERE l.id=grants.list_id AND x.deleted_at IS NOT NULL)
GROUP BY list_id,user_id;
CREATE TABLE workspace_lifecycle_events(id INTEGER PRIMARY KEY,workspace_id TEXT NOT NULL,actor_id TEXT NOT NULL,action TEXT NOT NULL,at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,revision INTEGER NOT NULL);
DROP VIEW actor_workspace_access;
CREATE VIEW actor_workspace_access AS
SELECT user_id AS actor_id,workspace_id,user_id,CASE role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END AS rank
FROM workspace_members m WHERE NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=m.workspace_id AND x.deleted_at IS NOT NULL)
UNION ALL
SELECT 'credential:'||a.id,m.workspace_id,m.user_id,
 MIN(CASE m.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END,CASE WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='admin') THEN 3 WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='write') THEN 2 ELSE 1 END)
FROM auth_agents a JOIN workspace_members m ON m.user_id=a.user_id
WHERE a.revoked=0 AND a.expires_at>unixepoch() AND a.list_ids IS NULL
 AND (a.workspace IS NULL OR a.workspace=m.workspace_id)
 AND EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value IN ('read','write','admin'))
 AND NOT EXISTS(SELECT 1 FROM workspace_lifecycle x WHERE x.workspace_id=m.workspace_id AND x.deleted_at IS NOT NULL);
CREATE TABLE list_references(list_id TEXT PRIMARY KEY REFERENCES lists(id) ON DELETE CASCADE,reference TEXT NOT NULL UNIQUE);
