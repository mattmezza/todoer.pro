from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class NotificationReadForUser(LoggedInUserRepo):
    async def is_read(self, task_uuid: str, kind: str) -> bool:
        row = await self.db.fetch_one(
            """
            SELECT 1 FROM notification_reads
            WHERE creator = :creator
                AND task_uuid = :task_uuid
                AND kind = :kind
            """,
            {
                "creator": self.user["uuid"],
                "task_uuid": task_uuid,
                "kind": kind,
            },
        )
        return row is not None

    async def mark_read(self, task_uuid: str, kind: str) -> None:
        uuid_new, _ = new_ids()
        await self.db.execute(
            """
            INSERT INTO notification_reads (
                uuid, creator, task_uuid, kind
            )
            VALUES (:uuid, :creator, :task_uuid, :kind)
            ON CONFLICT(creator, task_uuid, kind) DO NOTHING
            """,
            {
                "uuid": uuid_new,
                "creator": self.user["uuid"],
                "task_uuid": task_uuid,
                "kind": kind,
            },
        )

    async def mark_unread(self, task_uuid: str, kind: str) -> None:
        await self.db.execute(
            """
            DELETE FROM notification_reads
            WHERE creator = :creator
                AND task_uuid = :task_uuid
                AND kind = :kind
            """,
            {
                "creator": self.user["uuid"],
                "task_uuid": task_uuid,
                "kind": kind,
            },
        )

    async def mark_all_read(self, items: list[dict]) -> None:
        for item in items:
            uuid_new, _ = new_ids()
            await self.db.execute(
                """
                INSERT INTO notification_reads (
                    uuid, creator, task_uuid, kind
                )
                VALUES (:uuid, :creator, :task_uuid, :kind)
                ON CONFLICT(creator, task_uuid, kind) DO NOTHING
                """,
                {
                    "uuid": uuid_new,
                    "creator": self.user["uuid"],
                    "task_uuid": item["task_uuid"],
                    "kind": item["kind"],
                },
            )

    async def mark_all_unread(self) -> None:
        await self.db.execute(
            """
            DELETE FROM notification_reads
            WHERE creator = :creator
            """,
            {"creator": self.user["uuid"]},
        )

    async def get_read_keys(
        self, task_uuids: list[str], kinds: list[str]
    ) -> set[tuple[str, str]]:
        if not task_uuids or not kinds:
            return set()
        rows = await self.db.fetch_all(
            """
            SELECT task_uuid, kind FROM notification_reads
            WHERE creator = :creator
            """,
            {"creator": self.user["uuid"]},
        )
        return {
            (row._mapping["task_uuid"], row._mapping["kind"]) for row in rows
        }
