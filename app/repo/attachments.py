from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class AttachmentForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        attachment = await self.db.fetch_one(
            """
            SELECT at.* FROM attachments at
            JOIN tasks ta ON ta.uuid = at.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE at.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return attachment._mapping if attachment else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        attachment = await self.db.fetch_one(
            """
            SELECT at.* FROM attachments at
            JOIN tasks ta ON ta.uuid = at.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE at.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return attachment._mapping if attachment else None

    async def by_task(
        self,
        task_uuid: str,
        limit: int = 100,
    ) -> list[Mapping]:
        attachments = await self.db.fetch_all(
            """
            SELECT at.* FROM attachments at
            JOIN tasks ta ON ta.uuid = at.task
            JOIN sections se ON se.uuid = ta.section
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND at.task = :task_uuid
            ORDER BY at.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "task_uuid": task_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [st._mapping for st in attachments]

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM attachments
            WHERE sid = :sid
                AND EXISTS (
                    SELECT 1 FROM attachments at
                    JOIN tasks ta ON ta.uuid = at.task
                    JOIN sections se ON se.uuid = ta.section
                    JOIN lists l ON l.uuid = se.list
                    JOIN spaces sp ON sp.uuid = l.space
                    WHERE at.uuid = attachments.uuid
                        AND sp.creator = :creator
                )
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def create(
        self,
        uuid_new: str,
        content_type: str,
        fname: str,
        size: int,
        task_uuid: str,
    ) -> Mapping:
        _, sid_new = new_ids()
        attachment = await self.db.fetch_one(
            """
            INSERT INTO attachments (
                uuid,
                sid,
                mime,
                fname,
                size,
                task
            )
            VALUES (
                :uuid,
                :sid,
                :mime,
                :fname,
                :size,
                :task
            )
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "mime": content_type,
                "fname": fname,
                "size": size,
                "task": task_uuid,
            },
        )
        return attachment._mapping if attachment else None
