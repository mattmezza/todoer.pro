ALTER TABLE favorites ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
WITH ranked AS (
  SELECT user_id, target_kind, target_id,
    row_number() OVER (PARTITION BY user_id ORDER BY created_at, target_kind, target_id) AS position
  FROM favorites
)
UPDATE favorites SET position = (SELECT position FROM ranked r WHERE r.user_id=favorites.user_id AND r.target_kind=favorites.target_kind AND r.target_id=favorites.target_id);
