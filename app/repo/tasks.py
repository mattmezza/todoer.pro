from collections import defaultdict
from datetime import datetime
from typing import Mapping
from uuid import UUID

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

    async def by_section_for_list(
        self,
        list_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        tasks = await self.by_list(list_uuid, limit)
        tasks_by_section = defaultdict(list)
        for t in tasks:
            tasks_by_section[t["section"]].append(t)
        return tasks_by_section

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
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET descr = :descr, updated = CURRENT_TIMESTAMP
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
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return ta._mapping if ta is not None else None

    async def update_section(
        self, section: UUID, sid: str
    ) -> str | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET section = :section, updated = CURRENT_TIMESTAMP
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
            {"sid": sid, "section": str(section), "creator": self.user["uuid"]}
        )
        return ta.descr if ta is not None else None

    async def update_title_by_sid(
        self, title: str, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET title = :title, updated = CURRENT_TIMESTAMP
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
            RETURNING *
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
            RETURNING *
            """,
            {
                "sid": sid,
                "title": title,
                "descr": descr,
                "complete": 1 if complete else 0,
                "completed": datetime.utcnow() if complete else "NULL",
                "creator": self.user["uuid"],
            },
        )
        return ta._mapping if ta is not None else None

    async def del_by_sid(self, sid: str) -> None:
        async with self.db.transaction():
            task = await self.db.fetch_one(
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
                RETURNING *
                """,
                {"sid": sid, "creator": self.user["uuid"]},
            )
            if task is None:
                return
            await self.db.execute(
                """
                UPDATE tasks
                SET `order` = `order` - 1
                WHERE section = :section
                AND `order` > :order
                """,
                {"section": task.section, "order": task.order},
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

    async def move_up(self, sid: str) -> None | Mapping:
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
        task_before = await self.db.fetch_one(
            """
            SELECT ta.* FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE ta.section = :section
                AND ta.`order` < :order
                AND sp.creator = :creator
                AND ta.complete = 0
            ORDER BY ta.`order` DESC
            LIMIT 1
            """,
            {
                "section": task.section,
                "order": task.order,
                "creator": self.user["uuid"],
            },
        )
        if task_before is None:
            return task._mapping if task is not None else None
        order_before = task_before.order
        task_before = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": task.order, "uuid": task_before.uuid},
        )
        task = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": order_before, "uuid": task.uuid},
        )
        return task._mapping if task is not None else None

    async def move_down(self, sid: str) -> None | Mapping:
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
        task_after = await self.db.fetch_one(
            """
            SELECT * FROM tasks
            WHERE section = :section AND `order` > :order AND complete = 0
            ORDER BY `order` ASC
            LIMIT 1
            """,
            {
                "order": task.order,
                "section": task.section,
            },
        )
        if task_after is None:
            return task._mapping if task is not None else None
        order_after = task_after.order
        task_after = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": task.order, "uuid": task_after.uuid},
        )
        task = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": order_after, "uuid": task.uuid},
        )
        return task._mapping if task is not None else None

    async def move_top(self, sid: str) -> None | Mapping:
        async with self.db.transaction():
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
            if not task:
                return None
            if task.order == 0:
                return task._mapping
            task = await self.db.fetch_one(
                """
                UPDATE tasks
                    SET `order` = 0,
                    updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
                RETURNING *
                """,
                {"uuid": task.uuid},
            )
            await self.db.execute(
                """
                UPDATE tasks
                    SET `order` = `order` + 1
                WHERE sid != :sid
                    AND section = :section
                    AND `order` >= 0
                """,
                {"section": task.section, "sid": sid},
            )
            return task._mapping if task is not None else None

    async def move_bottom(self, sid: str) -> None | Mapping:
        async with self.db.transaction():
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
            if not task:
                return None
            order_old = task.order
            task = await self.db.fetch_one(
                """
                UPDATE tasks
                    SET `order` = (
                        SELECT MAX(`order`) FROM tasks
                            WHERE section = :section
                    )
                WHERE sid = :sid AND section = :section
                RETURNING *
                """,
                {"sid": sid, "section": task.section},
            )
            await self.db.execute(
                """
                UPDATE tasks
                    SET `order` = `order` - 1
                WHERE sid != :sid
                    AND section = :section
                    AND `order` > :order
                """,
                {"section": task.section, "sid": sid, "order": order_old},
            )
            return task._mapping if task is not None else None
