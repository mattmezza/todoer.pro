from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class AIUsageForUser(LoggedInUserRepo):
    async def count_today(self) -> int:
        row = await self.db.fetch_one(
            """
            SELECT COUNT(*) AS cnt FROM ai_usage
            WHERE user = :user
                AND created_at >= date('now')
            """,
            {"user": self.user["uuid"]},
        )
        return row._mapping["cnt"] if row else 0

    async def log(self, prompt_length: int, tasks_generated: int) -> Mapping:
        uuid_new, sid_new = new_ids()
        row = await self.db.fetch_one(
            """
            INSERT INTO ai_usage
                (uuid, sid, user, prompt_length, tasks_generated)
            VALUES
                (:uuid, :sid, :user, :prompt_length, :tasks_generated)
            RETURNING *
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "user": self.user["uuid"],
                "prompt_length": prompt_length,
                "tasks_generated": tasks_generated,
            },
        )
        return row._mapping if row else None
