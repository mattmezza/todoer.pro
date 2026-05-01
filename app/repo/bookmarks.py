from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class BookmarkForUser(LoggedInUserRepo):
    async def all(self) -> list[Mapping]:
        bookmarks = await self.db.fetch_all(
            """
            SELECT
                b.*,
                CASE b.entity_type
                    WHEN 'space' THEN s.title
                    WHEN 'list' THEN l.title
                END AS entity_title,
                CASE b.entity_type
                    WHEN 'space' THEN s.sid
                    WHEN 'list' THEN l.sid
                END AS entity_sid,
                CASE b.entity_type
                    WHEN 'list' THEN l.color
                    ELSE NULL
                END AS entity_color
            FROM bookmarks b
            LEFT JOIN spaces s
                ON b.entity_type = 'space'
                AND b.entity_uuid = s.uuid
            LEFT JOIN lists l
                ON b.entity_type = 'list'
                AND b.entity_uuid = l.uuid
            WHERE b.user = :user
            ORDER BY b."order" ASC, b.created ASC
            """,
            {"user": self.user["uuid"]},
        )
        return [bm._mapping for bm in bookmarks]

    async def create(
        self, entity_type: str, entity_uuid: str
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        bm = await self.db.fetch_one(
            """
            INSERT INTO bookmarks (
                uuid, sid, user, entity_type, entity_uuid
            )
            VALUES (
                :uuid, :sid, :user, :entity_type, :entity_uuid
            )
            ON CONFLICT(user, entity_type, entity_uuid) DO NOTHING
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "user": self.user["uuid"],
                "entity_type": entity_type,
                "entity_uuid": entity_uuid,
            },
        )
        return bm._mapping if bm else None

    async def del_by_entity(
        self, entity_type: str, entity_uuid: str
    ) -> None:
        await self.db.execute(
            """
            DELETE FROM bookmarks
            WHERE user = :user
                AND entity_type = :entity_type
                AND entity_uuid = :entity_uuid
            """,
            {
                "user": self.user["uuid"],
                "entity_type": entity_type,
                "entity_uuid": entity_uuid,
            },
        )

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM bookmarks
            WHERE sid = :sid AND user = :user
            """,
            {"sid": sid, "user": self.user["uuid"]},
        )

    async def is_bookmarked(
        self, entity_type: str, entity_uuid: str
    ) -> bool:
        row = await self.db.fetch_one(
            """
            SELECT 1 FROM bookmarks
            WHERE user = :user
                AND entity_type = :entity_type
                AND entity_uuid = :entity_uuid
            """,
            {
                "user": self.user["uuid"],
                "entity_type": entity_type,
                "entity_uuid": entity_uuid,
            },
        )
        return row is not None
