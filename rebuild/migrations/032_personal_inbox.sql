CREATE TABLE user_inboxes(user_id TEXT PRIMARY KEY REFERENCES users(id),list_id TEXT NOT NULL UNIQUE REFERENCES lists(id));
INSERT INTO lists(id,title,description,workspace_id) SELECT 'intake:'||id,'Inbox','Capture first. Plan or file when ready.',NULL FROM users;
INSERT INTO memberships(list_id,user_id) SELECT 'intake:'||id,id FROM users;
INSERT INTO user_inboxes SELECT id,'intake:'||id FROM users;
CREATE TRIGGER create_personal_inbox AFTER INSERT ON users BEGIN
 INSERT INTO lists(id,title,description,workspace_id) VALUES('intake:'||NEW.id,'Inbox','Capture first. Plan or file when ready.',NULL);
 INSERT INTO memberships(list_id,user_id) VALUES('intake:'||NEW.id,NEW.id);
 INSERT INTO user_inboxes VALUES(NEW.id,'intake:'||NEW.id);
END;
CREATE TABLE plan_slots(task_id TEXT PRIMARY KEY REFERENCES tasks(id),local_time TEXT NOT NULL,timezone TEXT NOT NULL);
CREATE TRIGGER clear_unplanned_slot AFTER UPDATE OF document ON task_schedule WHEN json_extract(NEW.document,'$.planned') IS NULL BEGIN DELETE FROM plan_slots WHERE task_id=NEW.task_id; END;
