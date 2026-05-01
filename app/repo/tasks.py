from collections import defaultdict
from datetime import datetime, timezone
from typing import Mapping
from uuid import UUID

from .utils import LoggedInUserRepo, new_ids


class TaskForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        task = await self.db.fetch_one(
            """
            SELECT
                ta.*,
                ta.`order`,
                CASE WHEN ta.`order` = (
                    SELECT MIN(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_first,
                CASE WHEN ta.`order` = (
                    SELECT MAX(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_last
            FROM tasks ta
            JOIN sections se on ta.section = se.uuid
            JOIN lists l on se.list = l.uuid
            JOIN spaces sp on l.space = sp.uuid
            WHERE ta.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return task._mapping if task else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        task = await self.db.fetch_one(
            """
            SELECT
                ta.*,
                ta.`order`,
                CASE WHEN ta.`order` = (
                    SELECT MIN(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_first,
                CASE WHEN ta.`order` = (
                    SELECT MAX(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_last
            FROM tasks ta
            JOIN sections se on ta.section = se.uuid
            JOIN lists l on se.list = l.uuid
            JOIN spaces sp on l.space = sp.uuid
            WHERE ta.uuid = :uuid
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
                AND ta.archived = 0
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
            SELECT
                ta.*,
                ta.`order`,
                CASE WHEN ta.`order` = (
                    SELECT MIN(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_first,
                CASE WHEN ta.`order` = (
                    SELECT MAX(`order`) FROM tasks
                    WHERE section = ta.section AND complete = 0
                ) THEN 1 ELSE 0 END AS is_last
            FROM tasks ta
            JOIN sections se on ta.section = se.uuid
            JOIN lists l on se.list = l.uuid
            JOIN spaces sp on l.space = sp.uuid
            WHERE sp.creator = :creator
                AND ta.section = :section_uuid
                AND ta.archived = 0
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
                        SELECT 1 FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE ta.uuid = tasks.uuid
                            AND sp.creator = :creator
                    )
            RETURNING *
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return ta._mapping if ta is not None else None

    async def update_section(self, section: UUID, sid: str) -> str | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks SET
                section = :section,
                `order` = COALESCE(
                    (
                        SELECT MAX(`order`) + 1
                        FROM tasks
                        WHERE section = :section
                    ), 0),
                updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT 1 FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE ta.uuid = tasks.uuid
                            AND sp.creator = :creator
                    )
            RETURNING *
            """,
            {
                "sid": sid,
                "section": str(section),
                "creator": self.user["uuid"],
            },
        )
        return ta._mapping if ta is not None else None

    async def update_title_by_sid(
        self, title: str, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET title = :title, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT 1 FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE ta.uuid = tasks.uuid
                            AND sp.creator = :creator
                    )
            RETURNING *
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return ta._mapping if ta is not None else None

    async def _reorder_on_complete_toggle(
        self, task_before, task_after
    ) -> None:
        """Reorder tasks after a complete/incomplete toggle.

        When marking complete: move task to max order + 1 and
        decrement orders of tasks that were below it.
        When marking incomplete: move task to max order + 1
        among incomplete tasks (append at bottom of visible list).
        """
        old_complete = task_before["complete"]
        new_complete = task_after["complete"]
        if old_complete == new_complete:
            return
        section = task_after["section"]
        old_order = task_before["order"]
        if new_complete:
            # Marking complete: push to bottom, close gap
            max_order = await self.db.fetch_one(
                """
                SELECT COALESCE(MAX(`order`), -1) AS m
                FROM tasks
                WHERE section = :section
                """,
                {"section": section},
            )
            new_order = max_order["m"] + 1
            await self.db.execute(
                """
                UPDATE tasks
                SET `order` = :new_order,
                    updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
                """,
                {
                    "new_order": new_order,
                    "uuid": task_after["uuid"],
                },
            )
            await self.db.execute(
                """
                UPDATE tasks
                SET `order` = `order` - 1
                WHERE section = :section
                    AND `order` > :old_order
                    AND uuid != :uuid
                """,
                {
                    "section": section,
                    "old_order": old_order,
                    "uuid": task_after["uuid"],
                },
            )
        else:
            # Marking incomplete: append at bottom of
            # incomplete list
            max_incomplete = await self.db.fetch_one(
                """
                SELECT COALESCE(MAX(`order`), -1) AS m
                FROM tasks
                WHERE section = :section AND complete = 0
                    AND uuid != :uuid
                """,
                {
                    "section": section,
                    "uuid": task_after["uuid"],
                },
            )
            new_order = max_incomplete["m"] + 1
            await self.db.execute(
                """
                UPDATE tasks
                SET `order` = :new_order,
                    updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
                """,
                {
                    "new_order": new_order,
                    "uuid": task_after["uuid"],
                },
            )

    async def update_completed_by_sid(
        self, complete: str | None, sid: str
    ) -> Mapping | None:
        async with self.db.transaction():
            task_before = await self.db.fetch_one(
                """
                SELECT * FROM tasks WHERE sid = :sid
                """,
                {"sid": sid},
            )
            if task_before is None:
                return None
            ta = await self.db.fetch_one(
                """
                UPDATE tasks
                    SET
                        complete = :complete,
                        completed = :completed,
                        updated = CURRENT_TIMESTAMP
                    WHERE sid = :sid
                        AND EXISTS (
                            SELECT 1 FROM tasks ta
                            JOIN sections se ON se.uuid = ta.section
                            JOIN lists l ON l.uuid = se.list
                            JOIN spaces sp ON sp.uuid = l.space
                            WHERE ta.uuid = tasks.uuid
                                AND sp.creator = :creator
                        )
                RETURNING *
                """,
                {
                    "sid": sid,
                    "complete": 1 if complete else 0,
                    "completed": (
                        datetime.now(timezone.utc) if complete else None
                    ),
                    "creator": self.user["uuid"],
                },
            )
            if ta is None:
                return None
            await self._reorder_on_complete_toggle(
                task_before._mapping, ta._mapping
            )
            ta = await self.db.fetch_one(
                """
                SELECT * FROM tasks WHERE sid = :sid
                """,
                {"sid": sid},
            )
        return ta._mapping if ta is not None else None

    async def update_task_by_sid(
        self,
        complete: str | None,
        title: str,
        descr: str | None,
        sid: str,
    ) -> Mapping | None:
        async with self.db.transaction():
            task_before = await self.db.fetch_one(
                """
                SELECT * FROM tasks WHERE sid = :sid
                """,
                {"sid": sid},
            )
            if task_before is None:
                return None
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
                            SELECT 1 FROM tasks ta
                            JOIN sections se ON se.uuid = ta.section
                            JOIN lists l ON l.uuid = se.list
                            JOIN spaces sp ON sp.uuid = l.space
                            WHERE ta.uuid = tasks.uuid
                                AND sp.creator = :creator
                        )
                RETURNING *
                """,
                {
                    "sid": sid,
                    "title": title,
                    "descr": descr,
                    "complete": 1 if complete else 0,
                    "completed": (
                        datetime.now(timezone.utc) if complete else None
                    ),
                    "creator": self.user["uuid"],
                },
            )
            if ta is None:
                return None
            await self._reorder_on_complete_toggle(
                task_before._mapping, ta._mapping
            )
            ta = await self.db.fetch_one(
                """
                SELECT * FROM tasks WHERE sid = :sid
                """,
                {"sid": sid},
            )
        return ta._mapping if ta is not None else None

    async def archive(self, sid: str) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks SET archived = 1, updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
                AND EXISTS (
                    SELECT 1 FROM tasks ta
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = se.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE ta.uuid = tasks.uuid
                        AND sp.creator = :creator
                )
            RETURNING *
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return ta._mapping if ta else None

    async def del_by_sid(self, sid: str) -> None:
        async with self.db.transaction():
            task = await self.db.fetch_one(
                """
                DELETE FROM tasks
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT 1 FROM tasks ta
                        JOIN sections se ON se.uuid = ta.section
                        JOIN lists l ON l.uuid = se.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE ta.uuid = tasks.uuid
                            AND sp.creator = :creator
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
            SELECT
                :uuid,
                :sid,
                COALESCE(
                    (SELECT MAX(`order`) + 1 FROM tasks WHERE section = :section),
                    0
                ),
                :title,
                :descr,
                :section
            WHERE EXISTS (
                SELECT 1 FROM sections se
                JOIN lists l ON l.uuid = se.list
                JOIN spaces sp ON sp.uuid = l.space
                WHERE se.uuid = :section
                    AND sp.creator = :creator
            )
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "section": section,
                "creator": self.user["uuid"],
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
            order_old = task.order
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
                    AND `order` >= 0 AND `order` < :order_old
                """,
                {"section": task.section, "sid": sid, "order_old": order_old},
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
