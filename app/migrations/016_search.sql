-- One independently indexed document per business entity, including each comment.
CREATE TABLE search_documents (
 rowid INTEGER PRIMARY KEY, kind TEXT NOT NULL, id TEXT NOT NULL,
 title TEXT NOT NULL, body TEXT NOT NULL,
 created_at INTEGER, updated_at INTEGER,
 UNIQUE(kind,id)
);
CREATE VIRTUAL TABLE search_fts USING fts5(title,body,content='search_documents',content_rowid='rowid',tokenize='unicode61 remove_diacritics 2',prefix='2 3 4');
CREATE TABLE search_generation(singleton INTEGER PRIMARY KEY CHECK(singleton=1),value TEXT NOT NULL);
INSERT INTO search_generation VALUES(1,lower(hex(randomblob(16))));
CREATE TRIGGER search_doc_insert AFTER INSERT ON search_documents BEGIN
 INSERT INTO search_fts(rowid,title,body) VALUES(new.rowid,new.title,new.body);
 UPDATE search_generation SET value=lower(hex(randomblob(16)));
END;
CREATE TRIGGER search_doc_delete AFTER DELETE ON search_documents BEGIN
 INSERT INTO search_fts(search_fts,rowid,title,body) VALUES('delete',old.rowid,old.title,old.body);
 UPDATE search_generation SET value=lower(hex(randomblob(16)));
END;
CREATE TRIGGER search_doc_update AFTER UPDATE ON search_documents BEGIN
 INSERT INTO search_fts(search_fts,rowid,title,body) VALUES('delete',old.rowid,old.title,old.body);
 INSERT INTO search_fts(rowid,title,body) VALUES(new.rowid,new.title,new.body);
 UPDATE search_generation SET value=lower(hex(randomblob(16)));
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'workspace',id,title,description FROM workspaces;
CREATE TRIGGER search_workspace_insert AFTER INSERT ON workspaces BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'workspace',id,title,description,unixepoch(),unixepoch() FROM workspaces WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_workspace_update AFTER UPDATE ON workspaces BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'workspace',id,title,description,unixepoch(),unixepoch() FROM workspaces WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_workspace_delete AFTER DELETE ON workspaces BEGIN
 DELETE FROM search_documents WHERE kind='workspace' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'list',id,title,description FROM lists;
CREATE TRIGGER search_list_insert AFTER INSERT ON lists BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'list',id,title,description,unixepoch(),unixepoch() FROM lists WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_list_update AFTER UPDATE ON lists BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'list',id,title,description,unixepoch(),unixepoch() FROM lists WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_list_delete AFTER DELETE ON lists BEGIN
 DELETE FROM search_documents WHERE kind='list' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'section',id,title,description FROM sections;
CREATE TRIGGER search_section_insert AFTER INSERT ON sections BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'section',id,title,description,unixepoch(),unixepoch() FROM sections WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_section_update AFTER UPDATE ON sections BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'section',id,title,description,unixepoch(),unixepoch() FROM sections WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_section_delete AFTER DELETE ON sections BEGIN
 DELETE FROM search_documents WHERE kind='section' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'task',id,title,description||' '||reference||' '||id FROM tasks;
CREATE TRIGGER search_task_insert AFTER INSERT ON tasks BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'task',id,title,description||' '||reference||' '||id,unixepoch(),unixepoch() FROM tasks WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_task_update AFTER UPDATE ON tasks BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'task',id,title,description||' '||reference||' '||id,unixepoch(),unixepoch() FROM tasks WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_task_delete AFTER DELETE ON tasks BEGIN
 DELETE FROM search_documents WHERE kind='task' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'comment',id,'Comment',body,created_at,COALESCE(updated_at,created_at) FROM comments;
CREATE TRIGGER search_comment_insert AFTER INSERT ON comments BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'comment',id,'Comment',body,unixepoch(),unixepoch() FROM comments WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_comment_update AFTER UPDATE ON comments BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'comment',id,'Comment',body,unixepoch(),unixepoch() FROM comments WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_comment_delete AFTER DELETE ON comments BEGIN
 DELETE FROM search_documents WHERE kind='comment' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'tag',id,name,normalized_name FROM tags;
CREATE TRIGGER search_tag_insert AFTER INSERT ON tags BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'tag',id,name,normalized_name,unixepoch(),unixepoch() FROM tags WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_tag_update AFTER UPDATE ON tags BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'tag',id,name,normalized_name,unixepoch(),unixepoch() FROM tags WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_tag_delete AFTER DELETE ON tags BEGIN
 DELETE FROM search_documents WHERE kind='tag' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'attachment',id,filename,'',created_at,created_at FROM attachments;
CREATE TRIGGER search_attachment_insert AFTER INSERT ON attachments BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'attachment',id,filename,'',unixepoch(),unixepoch() FROM attachments WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_attachment_update AFTER UPDATE ON attachments BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'attachment',id,filename,'',unixepoch(),unixepoch() FROM attachments WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_attachment_delete AFTER DELETE ON attachments BEGIN
 DELETE FROM search_documents WHERE kind='attachment' AND id=old.id;
END;
INSERT INTO search_documents(kind,id,title,body) SELECT 'legacy_attachment',id,filename,'' FROM legacy_attachment_refs;
CREATE TRIGGER search_legacy_attachment_insert AFTER INSERT ON legacy_attachment_refs BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'legacy_attachment',id,filename,'',unixepoch(),unixepoch() FROM legacy_attachment_refs WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_legacy_attachment_update AFTER UPDATE ON legacy_attachment_refs BEGIN
 INSERT INTO search_documents(kind,id,title,body,created_at,updated_at) SELECT 'legacy_attachment',id,filename,'',unixepoch(),unixepoch() FROM legacy_attachment_refs WHERE id=new.id ON CONFLICT(kind,id) DO UPDATE SET title=excluded.title,body=excluded.body,updated_at=unixepoch();
END;
CREATE TRIGGER search_legacy_attachment_delete AFTER DELETE ON legacy_attachment_refs BEGIN
 DELETE FROM search_documents WHERE kind='legacy_attachment' AND id=old.id;
END;
CREATE TRIGGER search_generation_task_schedule_insert AFTER INSERT ON task_schedule BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_schedule_update AFTER UPDATE ON task_schedule BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_schedule_delete AFTER DELETE ON task_schedule BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_tags_insert AFTER INSERT ON task_tags BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_tags_update AFTER UPDATE ON task_tags BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_tags_delete AFTER DELETE ON task_tags BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_locations_insert AFTER INSERT ON task_locations BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_locations_update AFTER UPDATE ON task_locations BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_task_locations_delete AFTER DELETE ON task_locations BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_memberships_insert AFTER INSERT ON memberships BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_memberships_update AFTER UPDATE ON memberships BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_memberships_delete AFTER DELETE ON memberships BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_membership_roles_insert AFTER INSERT ON membership_roles BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_membership_roles_update AFTER UPDATE ON membership_roles BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_membership_roles_delete AFTER DELETE ON membership_roles BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_workspace_members_insert AFTER INSERT ON workspace_members BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_workspace_members_update AFTER UPDATE ON workspace_members BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_workspace_members_delete AFTER DELETE ON workspace_members BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_auth_agents_insert AFTER INSERT ON auth_agents BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_auth_agents_update AFTER UPDATE ON auth_agents BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_auth_agents_delete AFTER DELETE ON auth_agents BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_occurrences_insert AFTER INSERT ON recurrence_occurrences BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_occurrences_update AFTER UPDATE ON recurrence_occurrences BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_occurrences_delete AFTER DELETE ON recurrence_occurrences BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_series_insert AFTER INSERT ON recurrence_series BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_series_update AFTER UPDATE ON recurrence_series BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
CREATE TRIGGER search_generation_recurrence_series_delete AFTER DELETE ON recurrence_series BEGIN UPDATE search_generation SET value=lower(hex(randomblob(16))); END;
