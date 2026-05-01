from typing import Mapping

from .utils import AnonymousRepo, LoggedInUserRepo, new_ids


class UserForAnon(AnonymousRepo):
    async def by_email(self, email: str) -> Mapping | None:
        user = await self.db.fetch_one(
            """
            SELECT * FROM users
            WHERE email = :email
            """,
            {"email": email},
        )
        return user._mapping if user else None

    async def update_last_login_by_uuid(self, uuid: str) -> Mapping | None:
        user = await self.db.fetch_one(
            """
            UPDATE users
            SET last_login = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            """,
            {"uuid": uuid},
        )
        return user._mapping if user else None

    async def update_confirmed_by_sid(
        self, sid: str, code: str
    ) -> Mapping | None:
        user = await self.db.fetch_one(
            """
            UPDATE users
            SET confirmed = CURRENT_TIMESTAMP
            WHERE sid = :sid AND confirm_code = :code
            """,
            {"sid": sid, "code": code},
        )
        return user._mapping if user else None

    async def insert(
        self, name: str, email: str, pwd: str, confirm_code: str
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        async with self.db.transaction():
            user = await self.db.fetch_one(
                """
                INSERT INTO users
                    (uuid, sid, name, email, password, confirm_code)
                VALUES (:uuid, :sid, :name, :email, :password, :confirm_code)
                RETURNING uuid, email
                """,
                {
                    "uuid": uuid_new,
                    "sid": sid_new,
                    "name": name,
                    "email": email,
                    "password": pwd,
                    "confirm_code": confirm_code,
                },
            )
            return user._mapping if user else None


class UserForUser(LoggedInUserRepo):
    async def update_fname_by_uuid(self, fname: str) -> Mapping | None:
        user = await self.db.fetch_one(
            "UPDATE users SET name = :fname WHERE uuid = :uuid RETURNING *",
            {"fname": fname, "uuid": self.user["uuid"]},
        )
        return user
