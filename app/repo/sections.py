from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class SectionForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            SELECT se.* FROM sections se
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return section._mapping if section else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            SELECT se.* FROM sections se
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return section._mapping if section else None

    async def by_list(self, list_uuid: str, limit: int = 100) -> list[Mapping]:
        sections = await self.db.fetch_all(
            """
            SELECT se.* FROM sections se
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND se.list = :list_uuid
            ORDER BY se.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "list_uuid": list_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [ss._mapping for ss in sections]

    async def update_title_by_sid(self, title: str, sid: str) -> str | None:
        section = await self.db.fetch_one(
            """
            UPDATE sections
                SET title = :title
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT s.sid FROM sections s
                        JOIN lists l ON l.uuid = s.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING title
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return section.descr if section is not None else None

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM sections
            WHERE sid = :sid
                AND EXISTS (
                    SELECT s.sid FROM sections s
                    JOIN lists l ON l.uuid = s.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE sp.creator = :creator
                )
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def insert(
        self, title: str, list_id: str, default: bool = False
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        section = await self.db.fetch_one(
            """INSERT INTO sections (
                uuid,
                sid,
                `order`,
                `default`,
                title,
                creator,
                list
            )
            VALUES (
                :uuid,
                :sid,
                COALESCE((SELECT MAX(`order`) + 1 FROM sections WHERE list = :list), 1),
                :default,
                :title,
                :creator,
                :list
            )
            RETURNING *""",
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "default": 1 if default else 0,
                "creator": self.user["uuid"],
                "list": list_id,
            },
        )
        return section._mapping if section else None
