-- Authorization is recalculated for every statement, including open MCP sessions.
CREATE VIEW actor_list_access AS
SELECT e.user_id AS actor_id,e.list_id,e.user_id,e.rank FROM effective_list_access e
UNION ALL
SELECT 'credential:'||a.id,e.list_id,e.user_id,
 MIN(e.rank,CASE WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='admin') THEN 3 WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='write') THEN 2 ELSE 1 END)
FROM auth_agents a JOIN effective_list_access e ON e.user_id=a.user_id
JOIN lists l ON l.id=e.list_id
WHERE a.revoked=0 AND a.expires_at>unixepoch()
 AND (a.list_ids IS NULL OR EXISTS(SELECT 1 FROM json_each(a.list_ids) WHERE value=e.list_id))
 AND (a.workspace IS NULL OR a.workspace=l.workspace_id)
 AND EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value IN ('read','write','admin'));
CREATE VIEW actor_workspace_access AS
SELECT user_id AS actor_id,workspace_id,user_id,CASE role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END AS rank FROM workspace_members
UNION ALL
SELECT 'credential:'||a.id,m.workspace_id,m.user_id,
 MIN(CASE m.role WHEN 'owner' THEN 3 WHEN 'editor' THEN 2 ELSE 1 END,CASE WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='admin') THEN 3 WHEN EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value='write') THEN 2 ELSE 1 END)
FROM auth_agents a JOIN workspace_members m ON m.user_id=a.user_id
WHERE a.revoked=0 AND a.expires_at>unixepoch() AND a.list_ids IS NULL
 AND (a.workspace IS NULL OR a.workspace=m.workspace_id)
 AND EXISTS(SELECT 1 FROM json_each(a.permissions) WHERE value IN ('read','write','admin'));
