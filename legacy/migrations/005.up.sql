ALTER TABLE spaces ADD COLUMN `default` INTEGER NOT NULL DEFAULT 0;
UPDATE spaces SET `default` = 1
    WHERE uuid IN (
        SELECT uuid FROM spaces
            GROUP BY creator
            ORDER BY created
    );
ALTER TABLE lists ADD COLUMN `default` INTEGER NOT NULL DEFAULT 0;
UPDATE lists SET `default` = 1
    WHERE uuid IN (
        SELECT uuid FROM lists
            GROUP BY space
            ORDER BY created
    );
