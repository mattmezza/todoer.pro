from collections import defaultdict
from datetime import datetime, timezone
from typing import Mapping
from uuid import UUID

from .utils import LoggedInUserRepo, new_ids


class TaskForUser(LoggedInUserRepo):
    async def stats(self) -> Mapping:
        row = await self.db.fetch_one(
            """
            SELECT
                COUNT(*) as total,
                SUM(CASE WHEN ta.complete = 1
                    THEN 1 ELSE 0 END) as completed,
                SUM(CASE WHEN ta.complete = 0
                    AND ta.archived = 0
                    THEN 1 ELSE 0 END) as active,
                SUM(CASE WHEN ta.due_date IS NOT NULL
                    AND ta.due_date < date('now')
                    AND ta.complete = 0
                    AND ta.archived = 0
                    THEN 1 ELSE 0 END) as overdue
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator AND ta.archived = 0
            """,
            {"creator": self.user["uuid"]},
        )
        if not row:
            return {"total": 0, "completed": 0, "active": 0, "overdue": 0}
        return dict(row._mapping)

    async def completed_per_period(self, days: int = 30) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT
                date(ta.completed) as day,
                COUNT(*) as count
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
                AND ta.complete = 1
                AND ta.completed >= date('now', :days_ago)
            GROUP BY date(ta.completed)
            ORDER BY day ASC
            """,
            {
                "creator": self.user["uuid"],
                "days_ago": f"-{days} days",
            },
        )
        return [dict(r._mapping) for r in rows]

    async def overdue(self) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT ta.uuid, ta.sid, ta.title, ta.due_date,
                   se.sid as section_sid, l.sid as list_sid,
                   sp.sid as space_sid,
                   sp.title as space_title, l.title as list_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
              AND ta.archived = 0
              AND ta.complete = 0
              AND ta.due_date IS NOT NULL
              AND ta.due_date < date('now')
            ORDER BY ta.due_date ASC
            """,
            {"creator": self.user["uuid"]},
        )
        return [dict(r._mapping) for r in rows]

    async def due_today(self) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT ta.uuid, ta.sid, ta.title, ta.due_date,
                   se.sid as section_sid, l.sid as list_sid,
                   sp.sid as space_sid,
                   sp.title as space_title, l.title as list_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
              AND ta.archived = 0
              AND ta.complete = 0
              AND ta.due_date IS NOT NULL
              AND ta.due_date = date('now')
            ORDER BY ta.title ASC
            """,
            {"creator": self.user["uuid"]},
        )
        return [dict(r._mapping) for r in rows]

    async def upcoming(self, days: int = 7) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT ta.uuid, ta.sid, ta.title, ta.due_date,
                   se.sid as section_sid, l.sid as list_sid,
                   sp.sid as space_sid,
                   sp.title as space_title, l.title as list_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
              AND ta.archived = 0
              AND ta.complete = 0
              AND ta.due_date IS NOT NULL
              AND ta.due_date > date('now')
              AND ta.due_date <= date('now', :days_ahead)
            ORDER BY ta.due_date ASC
            """,
            {
                "creator": self.user["uuid"],
                "days_ahead": f"+{days} days",
            },
        )
        return [dict(r._mapping) for r in rows]

    async def search(
        self,
        query: str,
        limit: int = 20,
        space_sid: str | None = None,
        list_sid: str | None = None,
    ) -> list[Mapping]:
        conds = ["sp.creator = :creator", "ta.archived = 0"]
        params: dict = {
            "creator": self.user["uuid"],
            "limit": limit,
        }
        if query:
            conds.append(
                "(ta.title LIKE :query"
                " OR ta.descr LIKE :query"
                " OR ta.tags LIKE :query)"
            )
            params["query"] = f"%{query}%"
        if space_sid:
            conds.append("sp.sid = :space_sid")
            params["space_sid"] = space_sid
        if list_sid:
            conds.append("l.sid = :list_sid")
            params["list_sid"] = list_sid
        where = " AND ".join(conds)
        tasks = await self.db.fetch_all(
            f"""
            SELECT ta.sid, ta.title, ta.descr, 'task' as type,
                   se.sid as section_sid, l.sid as list_sid,
                   sp.sid as space_sid,
                   sp.title as space_title, l.title as list_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE {where}
            ORDER BY ta.updated DESC, ta.created DESC
            LIMIT :limit
            """,
            params,
        )
        return [dict(t._mapping) for t in tasks]

    async def all_tags(self) -> list[str]:
        rows = await self.db.fetch_all(
            """
            SELECT DISTINCT tags FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
              AND ta.tags != '' AND ta.tags IS NOT NULL
            """,
            {"creator": self.user["uuid"]},
        )
        all_tags: set[str] = set()
        for row in rows:
            for tag in dict(row._mapping)["tags"].split(","):
                t = tag.strip()
                if t:
                    all_tags.add(t)
        return sorted(all_tags)

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
                ) THEN 1 ELSE 0 END AS is_last,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid), 0) AS subtasks_total,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid AND complete = 1), 0) AS subtasks_done
            FROM tasks ta
            JOIN sections se on ta.section = se.uuid
            JOIN lists l on se.list = l.uuid
            JOIN spaces sp on l.space = sp.uuid
            WHERE ta.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return dict(task._mapping) if task else None

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
                ) THEN 1 ELSE 0 END AS is_last,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid), 0) AS subtasks_total,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid AND complete = 1), 0) AS subtasks_done
            FROM tasks ta
            JOIN sections se on ta.section = se.uuid
            JOIN lists l on se.list = l.uuid
            JOIN spaces sp on l.space = sp.uuid
            WHERE ta.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return dict(task._mapping) if task else None

    async def by_list(
        self,
        list_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        tasks = await self.db.fetch_all(
            """
            SELECT
                ta.*,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid), 0) AS subtasks_total,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid AND complete = 1), 0) AS subtasks_done
            FROM tasks ta
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
        return [dict(tt._mapping) for tt in tasks]

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
                ) THEN 1 ELSE 0 END AS is_last,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid), 0) AS subtasks_total,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid AND complete = 1), 0) AS subtasks_done
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
        return [dict(tt._mapping) for tt in tasks]

    async def update_priority_by_sid(
        self, priority: int | None, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET priority = :priority, updated = CURRENT_TIMESTAMP
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
                "priority": priority,
                "creator": self.user["uuid"],
            },
        )
        return dict(ta._mapping) if ta is not None else None

    async def update_due_date_by_sid(
        self, due_date: str | None, sid: str
    ) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET due_date = :due_date, updated = CURRENT_TIMESTAMP
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
                "due_date": due_date,
                "creator": self.user["uuid"],
            },
        )
        return dict(ta._mapping) if ta is not None else None

    async def update_tags_by_sid(self, tags: str, sid: str) -> Mapping | None:
        ta = await self.db.fetch_one(
            """
            UPDATE tasks
                SET tags = :tags, updated = CURRENT_TIMESTAMP
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
                "tags": tags,
                "creator": self.user["uuid"],
            },
        )
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta is not None else None

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
        return dict(ta._mapping) if ta else None

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
                {"section": task._mapping["section"], "order": task._mapping["order"]},
            )

    async def create(
        self,
        title: str,
        descr: str | None,
        section: str,
        due_date: str | None = None,
    ) -> Mapping:
        uuid_new, sid_new = new_ids()
        task = await self.db.fetch_one(
            """
            INSERT INTO tasks (uuid, sid, `order`, title, descr, section, due_date)
            SELECT
                :uuid,
                :sid,
                COALESCE(
                    (SELECT MAX(`order`) + 1 FROM tasks WHERE section = :section),
                    0
                ),
                :title,
                :descr,
                :section,
                :due_date
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
                "due_date": due_date,
                "creator": self.user["uuid"],
            },
        )
        return dict(task._mapping) if task else None

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
                "section": task._mapping["section"],
                "order": task._mapping["order"],
                "creator": self.user["uuid"],
            },
        )
        if task_before is None:
            return dict(task._mapping) if task is not None else None
        order_before = task_before._mapping["order"]
        task_before = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": task._mapping["order"], "uuid": task_before._mapping["uuid"]},
        )
        task = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": order_before, "uuid": task._mapping["uuid"]},
        )
        return dict(task._mapping) if task is not None else None

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
                "order": task._mapping["order"],
                "section": task._mapping["section"],
            },
        )
        if task_after is None:
            return dict(task._mapping) if task is not None else None
        order_after = task_after._mapping["order"]
        task_after = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": task._mapping["order"], "uuid": task_after._mapping["uuid"]},
        )
        task = await self.db.fetch_one(
            """
            UPDATE tasks
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            RETURNING *
            """,
            {"order": order_after, "uuid": task._mapping["uuid"]},
        )
        return dict(task._mapping) if task is not None else None

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
            if task._mapping["order"] == 0:
                return dict(task._mapping)
            order_old = task._mapping["order"]
            task = await self.db.fetch_one(
                """
                UPDATE tasks
                    SET `order` = 0,
                    updated = CURRENT_TIMESTAMP
                WHERE uuid = :uuid
                RETURNING *
                """,
                {"uuid": task._mapping["uuid"]},
            )
            await self.db.execute(
                """
                UPDATE tasks
                    SET `order` = `order` + 1
                WHERE sid != :sid
                    AND section = :section
                    AND `order` >= 0 AND `order` < :order_old
                """,
                {"section": task._mapping["section"], "sid": sid, "order_old": order_old},
            )
            return dict(task._mapping) if task is not None else None

    async def log_action(
        self,
        task_uuid: str,
        action: str,
        detail: str | None = None,
    ) -> None:
        await self.db.execute(
            """
            INSERT INTO task_audit_log (task, action, detail, actor)
            VALUES (:task, :action, :detail, :actor)
            """,
            {
                "task": task_uuid,
                "action": action,
                "detail": detail,
                "actor": self.user["uuid"],
            },
        )

    async def audit_log(
        self, task_uuid: str, limit: int = 50
    ) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT al.*, u.name as actor_name, u.email as actor_email
            FROM task_audit_log al
            JOIN users u ON u.uuid = al.actor
            WHERE al.task = :task
            ORDER BY al.created DESC
            LIMIT :limit
            """,
            {"task": task_uuid, "limit": limit},
        )
        return [dict(r._mapping) for r in rows]

    async def reorder(self, sids: list[str]) -> None:
        """Bulk-update task order based on the provided list of SIDs.

        Each task's `order` column is set to its index in the list.
        All tasks must belong to the same section and be owned by
        the current user.
        """
        async with self.db.transaction():
            for order, sid in enumerate(sids):
                await self.db.execute(
                    """
                    UPDATE tasks
                    SET `order` = :order,
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
                    """,
                    {
                        "order": order,
                        "sid": sid,
                        "creator": self.user["uuid"],
                    },
                )

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
            order_old = task._mapping["order"]
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
                {"sid": sid, "section": task._mapping["section"]},
            )
            await self.db.execute(
                """
                UPDATE tasks
                    SET `order` = `order` - 1
                WHERE sid != :sid
                    AND section = :section
                    AND `order` > :order
                """,
                {"section": task._mapping["section"], "sid": sid, "order": order_old},
            )
            return dict(task._mapping) if task is not None else None

    async def bulk_complete(self, sids: list[str]) -> int:
        """Mark multiple tasks as complete by their SIDs."""
        if not sids:
            return 0
        now = datetime.now(timezone.utc)
        count = 0
        async with self.db.transaction():
            for sid in sids:
                ta = await self.db.fetch_one(
                    """
                    UPDATE tasks
                        SET complete = 1,
                            completed = :completed,
                            updated = CURRENT_TIMESTAMP
                        WHERE sid = :sid
                            AND complete = 0
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
                        "completed": now,
                        "creator": self.user["uuid"],
                    },
                )
                if ta is not None:
                    count += 1
                    await self.log_action(
                        ta._mapping["uuid"], "completed"
                    )
        return count

    async def bulk_delete(self, sids: list[str]) -> int:
        """Delete multiple tasks by their SIDs."""
        if not sids:
            return 0
        count = 0
        async with self.db.transaction():
            for sid in sids:
                ta = await self.db.fetch_one(
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
                    {
                        "sid": sid,
                        "creator": self.user["uuid"],
                    },
                )
                if ta is not None:
                    count += 1
        return count

    async def bulk_move(
        self, sids: list[str], section_uuid: str
    ) -> int:
        """Move multiple tasks to a different section."""
        if not sids:
            return 0
        count = 0
        async with self.db.transaction():
            for sid in sids:
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
                        "section": section_uuid,
                        "creator": self.user["uuid"],
                    },
                )
                if ta is not None:
                    count += 1
        return count

    async def filtered_by_list(
        self,
        list_uuid: str,
        sort_by: str = "manual",
        priority: int | None = None,
        due: str | None = None,
        tags: list[str] | None = None,
        limit: int = 100,
    ) -> list[Mapping]:
        """Fetch tasks for a list with optional filters.

        due filters: 'overdue', 'today', 'this-week', 'next-week', 'none'
        """
        conds = [
            "s.creator = :creator",
            "ta.archived = 0",
            "se.list = :list_uuid",
        ]
        params: dict = {
            "creator": self.user["uuid"],
            "list_uuid": list_uuid,
            "limit": limit,
        }
        if priority is not None:
            conds.append("ta.priority = :priority")
            params["priority"] = priority
        if due == "overdue":
            conds.append(
                "ta.due_date IS NOT NULL AND ta.due_date < date('now')"
            )
        elif due == "today":
            conds.append("ta.due_date = date('now')")
        elif due == "this-week":
            conds.append(
                "ta.due_date > date('now') AND ta.due_date <= date('now', '+7 days')"
            )
        elif due == "next-week":
            conds.append(
                "ta.due_date > date('now', '+7 days') AND ta.due_date <= date('now', '+14 days')"
            )
        elif due == "none":
            conds.append("ta.due_date IS NULL")
        if tags:
            tag_conds = []
            for i, t in enumerate(tags):
                tag_conds.append(f"ta.tags LIKE :tags_{i}")
                params[f"tags_{i}"] = f"%{t.strip()}%"
            conds.append(f"({' OR '.join(tag_conds)})")
        where = " AND ".join(conds)
        order_clause = "ta.`order` ASC, ta.created ASC"
        if sort_by == "priority":
            order_clause = (
                "COALESCE(ta.priority, 99) ASC, ta.`order` ASC"
            )
        elif sort_by == "due-date":
            order_clause = (
                "ta.due_date IS NULL ASC, ta.due_date ASC, ta.`order` ASC"
            )
        elif sort_by == "created":
            order_clause = "ta.created ASC, ta.`order` ASC"
        elif sort_by == "alphabetical":
            order_clause = "LOWER(ta.title) ASC, ta.`order` ASC"
        tasks = await self.db.fetch_all(
            f"""
            SELECT
                ta.*,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid), 0) AS subtasks_total,
                COALESCE((SELECT COUNT(*) FROM subtasks WHERE task = ta.uuid AND complete = 1), 0) AS subtasks_done
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE {where}
            ORDER BY ta.complete ASC, {order_clause}
            LIMIT :limit
            """,
            params,
        )
        return [dict(tt._mapping) for tt in tasks]

    async def all_by_priority(
        self, priority: int, limit: int = 200
    ) -> list[Mapping]:
        """Fetch all tasks with given priority across all user's spaces."""
        rows = await self.db.fetch_all(
            """
            SELECT ta.uuid, ta.sid, ta.title, ta.descr, ta.priority,
                   ta.due_date, ta.tags, ta.complete, ta.archived,
                   ta.section, ta.created,
                   se.sid as section_sid, se.title as section_title,
                   l.sid as list_sid, l.title as list_title, l.color as list_color,
                   sp.sid as space_sid, sp.title as space_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
                AND ta.archived = 0
                AND ta.priority = :priority
            ORDER BY ta.due_date ASC NULLS LAST, ta.created ASC
            LIMIT :limit
            """,
            {
                "creator": self.user["uuid"],
                "priority": priority,
                "limit": limit,
            },
        )
        return [dict(r._mapping) for r in rows]

    async def by_date_range(
        self, start_date: str, end_date: str
    ) -> list[Mapping]:
        """Fetch all unarchived tasks with due_date in [start_date, end_date]."""
        rows = await self.db.fetch_all(
            """
            SELECT ta.uuid, ta.sid, ta.title, ta.descr, ta.priority,
                   ta.due_date, ta.tags, ta.complete, ta.completed,
                   ta.created,
                   se.sid as section_sid, se.title as section_title,
                   l.sid as list_sid, l.title as list_title, l.color as list_color,
                   sp.sid as space_sid, sp.title as space_title
            FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
                AND ta.archived = 0
                AND ta.due_date IS NOT NULL
                AND ta.due_date >= :start_date
                AND ta.due_date <= :end_date
            ORDER BY ta.due_date ASC, ta.priority ASC NULLS LAST, ta.complete ASC
            """,
            {
                "creator": self.user["uuid"],
                "start_date": start_date,
                "end_date": end_date,
            },
        )
        return [dict(r._mapping) for r in rows]

    async def all_tags_annotated(self) -> list[dict]:
        """Return all tags with counts."""
        rows = await self.db.fetch_all(
            """
            SELECT ta.tags FROM tasks ta
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces sp ON sp.uuid = l.space
            WHERE sp.creator = :creator
                AND ta.archived = 0
                AND ta.tags != '' AND ta.tags IS NOT NULL
            """,
            {"creator": self.user["uuid"]},
        )
        counts: dict[str, int] = {}
        for row in rows:
            for tag in row._mapping["tags"].split(","):
                t = tag.strip()
                if t:
                    counts[t] = counts.get(t, 0) + 1
        return sorted(
            [{"tag": k, "count": v} for k, v in counts.items()],
            key=lambda x: (-x["count"], x["tag"]),
        )
