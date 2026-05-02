from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


class SettingsForUser(LoggedInUserRepo):
    async def all(self) -> list[Mapping]:
        rows = await self.db.fetch_all(
            """
            SELECT * FROM user_settings
            WHERE creator = :creator
            ORDER BY key
            """,
            {"creator": self.user["uuid"]},
        )
        return [dict(r._mapping) for r in rows]

    async def get(self, key: str) -> str | None:
        row = await self.db.fetch_one(
            """
            SELECT value FROM user_settings
            WHERE creator = :creator AND key = :key
            """,
            {"creator": self.user["uuid"], "key": key},
        )
        return row._mapping["value"] if row else None

    async def get_many(self, keys: list[str]) -> dict[str, str]:
        if not keys:
            return {}
        placeholders = ", ".join(f":k{i}" for i in range(len(keys)))
        params = {"creator": self.user["uuid"]}
        for i, k in enumerate(keys):
            params[f"k{i}"] = k
        rows = await self.db.fetch_all(
            f"""
            SELECT key, value FROM user_settings
            WHERE creator = :creator AND key IN ({placeholders})
            """,
            params,
        )
        return {r._mapping["key"]: r._mapping["value"] for r in rows}

    async def set(self, key: str, value: str) -> None:
        uuid, sid = new_ids()
        await self.db.execute(
            """
            INSERT INTO user_settings (uuid, sid, creator, key, value)
            VALUES (:uuid, :sid, :creator, :key, :value)
            ON CONFLICT(creator, key)
            DO UPDATE SET value = :value, updated = datetime('now')
            """,
            {
                "uuid": uuid,
                "sid": sid,
                "creator": self.user["uuid"],
                "key": key,
                "value": value,
            },
        )

    async def delete(self, key: str) -> None:
        await self.db.execute(
            """
            DELETE FROM user_settings
            WHERE creator = :creator AND key = :key
            """,
            {"creator": self.user["uuid"], "key": key},
        )
