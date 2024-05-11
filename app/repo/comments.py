import hashlib
from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class CommentForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        comment = await self.db.fetch_one(
            """
            SELECT cc.* FROM comments cc
            JOIN tasks ta ON ta.uuid = cc.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE cc.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return comment._mapping if comment else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        comment = await self.db.fetch_one(
            """
            SELECT cc.* FROM comments cc
            JOIN tasks ta ON ta.uuid = cc.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE cc.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return comment._mapping if comment else None

    async def by_task(
        self,
        task_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        comments = await self.db.fetch_all(
            """
            SELECT cc.* FROM comments cc
            JOIN tasks ta ON ta.uuid = cc.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND cc.task = :task_uuid
            ORDER BY cc.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "task_uuid": task_uuid,
                "creator": self.user["uuid"],
            },
        )
        comments = [tt._mapping for tt in comments]
        for c in comments:
            c["author_email_hash"] = hashlib.md5(
                str(c["author_email"]).encode("utf-8")
            ).hexdigest()
        return comments

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM comments
            WHERE sid = :sid
                AND EXISTS (
                    SELECT cc.uuid FROM comments cc
                    JOIN tasks ta ON ta.uuid = cc.task
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = s.list
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
