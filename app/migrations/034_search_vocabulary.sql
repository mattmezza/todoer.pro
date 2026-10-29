-- A live view of the existing FTS index, with no duplicate index or backfill.
CREATE VIRTUAL TABLE search_vocabulary USING fts5vocab(search_fts, row);
