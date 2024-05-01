import asyncio
import sys
import uuid

from databases import Database
import shortuuid


def new_ids(max_len: int = 7) -> tuple[str, str]:
    u = uuid.uuid4()
    s = shortuuid.encode(u)[:max_len]
    return str(u), s


db = Database(sys.argv[1])


@db.transaction()
async def main():
    # adding space table
    await db.execute("""
CREATE TABLE IF NOT EXISTS spaces(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    title VARCHAR NOT NULL,
    descr TEXT,
    creator UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE CASCADE,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP
);
""")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_spaces_sid ON spaces(sid)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_spaces_title ON spaces(title)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_spaces_creator ON spaces(creator)")

    # adding column to lists table
    await db.execute("""
ALTER TABLE lists ADD COLUMN
    space UUID REFERENCES spaces(uuid)
    ON UPDATE CASCADE
    ON DELETE CASCADE;
    """)
    await db.execute("CREATE INDEX IF NOT EXISTS ix_lists_space ON lists(space)")

    # adding default space for each user
    users = await db.fetch_all("SELECT * FROM users")
    for user in users:
        uuid, sid = new_ids()
        space = await db.fetch_one(
            f"""
            INSERT INTO spaces
            (uuid, sid, title, descr, creator)
            VALUES (
                :uuid,
                :sid,
                'My space',
                "A space for {user.name}.",
                :creator
            )
            RETURNING uuid
            """,
            {
                "uuid": uuid,
                "sid": sid,
                "creator": user.uuid,
            },
        )
        await db.execute(
            """
            UPDATE lists SET space = :uuid
            WHERE creator = :creator
            """,
            {"uuid": space.uuid, "creator": user.uuid},
        )
        print(f"user {user.uuid} patched with space {space.uuid}")

    # adding NOT NULL constraint
    await db.execute("""
CREATE TABLE IF NOT EXISTS lists_tmp(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    title VARCHAR NOT NULL,
    descr TEXT,
    color VARCHAR NOT NULL DEFAULT 'FF0000',
    space UUID NOT NULL REFERENCES spaces(uuid),
    creator UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE CASCADE,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP
);
    """)
    await db.execute("""
INSERT INTO lists_tmp (
    uuid,
    sid,
    title,
    descr,
    color,
    space,
    creator,
    created,
    updated
) SELECT
    uuid,
    sid,
    title,
    descr,
    color,
    space,
    creator,
    created,
    updated
FROM lists
    """)
    await db.execute("""
CREATE TABLE IF NOT EXISTS sections_tmp(
    uuid UUID PRIMARY KEY,
    sid VARCHAR NOT NULL UNIQUE,
    `order` INTEGER NOT NULL,
    `default` INTEGER NOT NULL DEFAULT 0,
    title VARCHAR NOT NULL,
    creator UUID NOT NULL REFERENCES users(uuid) ON UPDATE CASCADE ON DELETE CASCADE,
    created TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMP,
    list UUID NOT NULL REFERENCES lists_tmp(uuid) ON UPDATE CASCADE ON DELETE CASCADE
)
    """)
    await db.execute("""
INSERT INTO sections_tmp (
    uuid,
    sid,
    `order`,
    `default`,
    title,
    creator,
    created,
    updated,
    list
) SELECT
    uuid,
    sid,
    `order`,
    `default`,
    title,
    creator,
    created,
    updated,
    list
FROM sections
    """)
    await db.execute("ALTER TABLE lists RENAME TO _lists_20240428")
    await db.execute("ALTER TABLE lists_tmp RENAME TO lists")
    await db.execute("ALTER TABLE sections RENAME TO _sections_20240428")
    await db.execute("ALTER TABLE sections_tmp RENAME TO sections")

    await db.execute("DROP INDEX ix_lists_space")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_lists_space ON lists(space)")

    await db.execute("DROP INDEX ix_lists_sid")
    await db.execute("DROP INDEX ix_lists_title")
    await db.execute("DROP INDEX ix_lists_creator")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_lists_sid ON lists(sid)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_lists_title ON lists(title)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_lists_creator ON lists(creator)")

    await db.execute("DROP INDEX ix_sections_sid")
    await db.execute("DROP INDEX ix_sections_order")
    await db.execute("DROP INDEX ix_sections_title")
    await db.execute("DROP INDEX ix_sections_creator")
    await db.execute("DROP INDEX ix_sections_list")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_sections_sid ON sections(sid)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_sections_order ON sections(`order`)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_sections_title ON sections(title)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_sections_creator ON sections(creator)")
    await db.execute("CREATE INDEX IF NOT EXISTS ix_sections_list ON sections(list)")

asyncio.get_event_loop().run_until_complete(asyncio.gather(main()))
