from datetime import datetime
from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class TaskForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        task = await self.db.fetch_one(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE ta.sid = :sid
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

    async def update_descr_by_sid(
        self, descr: str | None, sid: str
    ) -> str | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET descr = :descr
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING descr
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return ta.descr if ta is not None else None

    async def update_title_by_sid(
        self, title: str, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET title = :title
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING *
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return ta._mapping if ta is not None else None

    async def update_completed_by_sid(
        self, complete: str | None, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET
                    complete = :complete,
                    completed = :completed,
                    updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING tasks.*
            """,
            {
                "sid": sid,
                "complete": 1 if complete else 0,
                "completed": datetime.utcnow() if complete else "NULL",
                "creator": self.user["uuid"],
            },
        )
        return ta._mapping if ta is not None else None

    async def update_task_by_sid(
        self, complete: str | None, title: str, descr: str | None, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET
                    title = :title,
                    descr = :descr,
                    complete = :complete,
                    completed = :completed,
                    updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT ta.uuid FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING tasks.*
            """,
            {
                "sid": sid,
                "title": title,
                "descr": descr if descr else "NULL",
                "complete": 1 if complete else 0,
                "completed": datetime.utcnow() if complete else "NULL",
                "creator": self.user["uuid"],
            },
        )
        return ta._mapping if ta is not None else None

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM tasks
            WHERE sid = :sid
                AND EXISTS (
                    SELECT ta.uuid FROM tasks ta
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = se.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE sp.creator = :creator
                )
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def create(
        self, title: str, descr: str | None, section: str
    ) -> Mapping:
        uuid_new, sid_new = new_ids()
        task = await self.db.fetch_one(
            """
            INSERT INTO tasks (uuid, sid, `order`, title, descr, section)
            VALUES (
                :uuid,
                :sid,
                COALESCE(
                    (select MAX(`order`) + 1 FROM tasks WHERE section = :section),
                    0
                ),
                :title,
                :descr,
                :section
            )
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "section": section,
            },
        )
        return task._mapping if task else None
