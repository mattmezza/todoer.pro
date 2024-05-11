from typing import Mapping

from .utils import LoggedInUserRepo


class TaskForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        task = await self.db.fetch_one(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return task._mapping if task else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        task = await self.db.fetch_one(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return task._mapping if task else None

    async def by_list(
        self,
        list_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        tasks = await self.db.fetch_all(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND se.list = :list_uuid
            ORDER BY ta.`order` ASC, ta.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "list_uuid": list_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [tt._mapping for tt in tasks]

    async def by_section(
        self,
        section_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        tasks = await self.db.fetch_all(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND ta.section = :section_uuid
            ORDER BY ta.`order` ASC, ta.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "section_uuid": section_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [tt._mapping for tt in tasks]

    async def update_descr_by_sid(self, descr: str, sid: str) -> str | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET descr = :descr
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = s.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING descr
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return ta.descr if ta is not None else None

    async def update_title_by_sid(self, title: str, sid: str) -> str | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET title = :title
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = s.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING title
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return ta.descr if ta is not None else None

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM tasks
            WHERE sid = :sid
                AND EXISTS (
                    SELECT ta.uuid FROM tasks ta
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = s.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE sp.creator = :creator
                )
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
