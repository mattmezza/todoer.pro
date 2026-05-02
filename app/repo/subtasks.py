from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class SubtaskForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        subtask = await self.db.fetch_one(
            """
            SELECT st.* FROM subtasks st
            JOIN tasks ta ON ta.uuid = st.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE st.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return dict(subtask._mapping) if subtask else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        subtask = await self.db.fetch_one(
            """
            SELECT st.* FROM subtasks st
            JOIN tasks ta ON ta.uuid = st.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE st.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return dict(subtask._mapping) if subtask else None

    async def by_sid_full(self, sid: str) -> Mapping | None:
        row = await self.db.fetch_one(
            """
            SELECT
                st.*,
                ta.uuid AS task_uuid,
                ta.sid AS task_sid,
                ta.title AS task_title,
                se.uuid AS section_uuid,
                se.sid AS section_sid,
                se.title AS section_title,
                li.uuid AS list_uuid,
                li.sid AS list_sid,
                li.title AS list_title,
                li.color AS list_color,
                li."default" AS list_default,
                sp.uuid AS space_uuid,
                sp.sid AS space_sid,
                sp.title AS space_title,
                sp."default" AS space_default
            FROM subtasks st
            JOIN tasks ta ON ta.uuid = st.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE st.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return dict(row._mapping) if row else None

    async def by_task(
        self,
        task_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        subtasks = await self.db.fetch_all(
            """
            SELECT st.* FROM subtasks st
            JOIN tasks ta ON ta.uuid = st.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND st.task = :task_uuid
                AND st.parent IS NULL
            ORDER BY st.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "task_uuid": task_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [dict(st._mapping) for st in subtasks]

    async def by_parent(
        self,
        parent_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        subtasks = await self.db.fetch_all(
            """
            SELECT st.* FROM subtasks st
            JOIN tasks ta ON ta.uuid = st.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND st.parent = :parent_uuid
            ORDER BY st.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "parent_uuid": parent_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [dict(st._mapping) for st in subtasks]

    async def ancestors(self, subtask_uuid: str) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            WITH RECURSIVE anc(uuid, sid, title, parent, depth) AS (
                SELECT st.uuid, st.sid, st.title, st.parent, 0
                FROM subtasks st
                WHERE st.uuid = :uuid
                UNION ALL
                SELECT p.uuid, p.sid, p.title, p.parent, anc.depth + 1
                FROM subtasks p
                JOIN anc ON anc.parent = p.uuid
            )
            SELECT uuid, sid, title, parent, depth FROM anc
            WHERE uuid != :uuid
            ORDER BY depth DESC
            """,
            {"uuid": subtask_uuid},
        )
        return [dict(r._mapping) for r in rows]

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM subtasks
            WHERE sid = :sid
                AND EXISTS (
                    SELECT 1 FROM subtasks st
                    JOIN tasks ta ON ta.uuid = st.task
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = se.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE st.uuid = subtasks.uuid
                        AND sp.creator = :creator
                )
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def create(
        self,
        title: str,
        descr: str | None,
        task_uuid: str,
        parent_uuid: str | None = None,
    ) -> Mapping:
        uuid_new, sid_new = new_ids()
        subtask = await self.db.fetch_one(
            """
            INSERT INTO subtasks (uuid, sid, `order`, title, descr, task, parent)
            SELECT
                :uuid,
                :sid,
                COALESCE(
                    (SELECT MAX(`order`) FROM subtasks WHERE task = :task),
                    0
                ),
                :title,
                :descr,
                :task,
                :parent
            WHERE EXISTS (
                SELECT 1 FROM tasks ta
                JOIN sections se ON se.uuid = ta.section
                JOIN lists l ON l.uuid = se.list
                JOIN spaces sp ON sp.uuid = l.space
                WHERE ta.uuid = :task
                    AND sp.creator = :creator
            )
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "task": task_uuid,
                "parent": parent_uuid,
                "creator": self.user["uuid"],
            },
        )
        return dict(subtask._mapping) if subtask else None

    async def update(
        self, sid: str, title: str, descr: str | None, complete: bool
    ) -> Mapping:
        subtask = await self.db.fetch_one(
            """
            UPDATE subtasks
            SET complete = :complete, title = :title, descr = :descr, updated = CURRENT_TIMESTAMP
            WHERE sid = :sid AND task IN (
                SELECT uuid FROM tasks WHERE section IN (
                    SELECT uuid FROM sections WHERE creator = :user
                )
            )
            RETURNING *
            """,
            {
                "sid": sid,
                "complete": 1 if complete else 0,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "user": self.user['uuid'],
            },
        )
        return dict(subtask._mapping) if subtask else None

    async def update_title(self, sid: str, title: str) -> Mapping | None:
        subtask = await self.db.fetch_one(
            """
            UPDATE subtasks
            SET title = :title, updated = CURRENT_TIMESTAMP
            WHERE sid = :sid AND task IN (
                SELECT uuid FROM tasks WHERE section IN (
                    SELECT uuid FROM sections WHERE creator = :user
                )
            )
            RETURNING *
            """,
            {
                "sid": sid,
                "title": title.strip(),
                "user": self.user['uuid'],
            },
        )
        return dict(subtask._mapping) if subtask else None

    async def update_descr(
        self, sid: str, descr: str | None
    ) -> Mapping | None:
        subtask = await self.db.fetch_one(
            """
            UPDATE subtasks
            SET descr = :descr, updated = CURRENT_TIMESTAMP
            WHERE sid = :sid AND task IN (
                SELECT uuid FROM tasks WHERE section IN (
                    SELECT uuid FROM sections WHERE creator = :user
                )
            )
            RETURNING *
            """,
            {
                "sid": sid,
                "descr": descr.strip() if descr else None,
                "user": self.user['uuid'],
            },
        )
        return dict(subtask._mapping) if subtask else None

    async def update_complete(
        self, sid: str, complete: bool
    ) -> Mapping | None:
        subtask = await self.db.fetch_one(
            """
            UPDATE subtasks
            SET complete = :complete,
                completed = CASE WHEN :complete = 1 THEN CURRENT_TIMESTAMP ELSE NULL END,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid AND task IN (
                SELECT uuid FROM tasks WHERE section IN (
                    SELECT uuid FROM sections WHERE creator = :user
                )
            )
            RETURNING *
            """,
            {
                "sid": sid,
                "complete": 1 if complete else 0,
                "user": self.user['uuid'],
            },
        )
        return dict(subtask._mapping) if subtask else None
