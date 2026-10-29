-- Human-facing names remain untouched; IDs/references remain canonical URL keys.
CREATE TABLE entity_slugs (
 kind TEXT NOT NULL CHECK(kind IN ('workspace','list','section')),
 entity_id TEXT NOT NULL,
 scope TEXT NOT NULL,
 slug TEXT NOT NULL CHECK(length(slug)>0 AND length(slug)<=80),
 source_title TEXT NOT NULL,
 PRIMARY KEY(kind,entity_id),
 UNIQUE(kind,scope,slug)
);
CREATE TRIGGER workspace_slug_delete AFTER DELETE ON workspaces BEGIN
 DELETE FROM entity_slugs WHERE kind='workspace' AND entity_id=old.id;
END;
CREATE TRIGGER list_slug_delete AFTER DELETE ON lists BEGIN
 DELETE FROM entity_slugs WHERE kind='list' AND entity_id=old.id;
END;
CREATE TRIGGER section_slug_delete AFTER DELETE ON sections BEGIN
 DELETE FROM entity_slugs WHERE kind='section' AND entity_id=old.id;
END;
