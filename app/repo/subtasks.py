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
        return subtask._mapping if subtask else None

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
        return subtask._mapping if subtask else None

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
            ORDER BY st.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "task_uuid": task_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [st._mapping for st in subtasks]

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
        self, title: str, descr: str | None, task_uuid: str
    ) -> Mapping:
        uuid_new, sid_new = new_ids()
        subtask = await self.db.fetch_one(
            """
            INSERT INTO subtasks (uuid, sid, `order`, title, descr, task)
            SELECT
                :uuid,
                :sid,
                COALESCE(
                    (SELECT MAX(`order`) FROM subtasks WHERE task = :task),
                    0
                ),
                :title,
                :descr,
                :task
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
                "creator": self.user["uuid"],
            },
        )
        return subtask._mapping if subtask else None

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
        return subtask._mapping if subtask else None
