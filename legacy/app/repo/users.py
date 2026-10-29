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
        return dict(user._mapping) if user else None

    async def update_last_login_by_uuid(self, uuid: str) -> Mapping | None:
        user = await self.db.fetch_one(
            """
            UPDATE users
            SET last_login = CURRENT_TIMESTAMP
            WHERE uuid = :uuid
            """,
            {"uuid": uuid},
        )
        return dict(user._mapping) if user else None

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
        return dict(user._mapping) if user else None

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
            return dict(user._mapping) if user else None


class UserForUser(LoggedInUserRepo):
    async def update_fname_by_uuid(self, fname: str) -> Mapping | None:
        user = await self.db.fetch_one(
            "UPDATE users SET name = :fname WHERE uuid = :uuid RETURNING *",
            {"fname": fname, "uuid": self.user["uuid"]},
        )
        return user

    async def enable_totp(
        self,
        encrypted_secret: str,
        hashed_backup_codes: str,
    ) -> None:
        await self.db.execute(
            """
            UPDATE users
            SET totp_secret = :secret,
                totp_enabled = 1,
                totp_backup_codes = :backup_codes
            WHERE uuid = :uuid
            """,
            {
                "secret": encrypted_secret,
                "backup_codes": hashed_backup_codes,
                "uuid": self.user["uuid"],
            },
        )

    async def disable_totp(self) -> None:
        await self.db.execute(
            """
            UPDATE users
            SET totp_secret = NULL,
                totp_enabled = 0,
                totp_backup_codes = NULL
            WHERE uuid = :uuid
            """,
            {"uuid": self.user["uuid"]},
        )

    async def update_backup_codes(
        self, hashed_backup_codes: str
    ) -> None:
        await self.db.execute(
            """
            UPDATE users
            SET totp_backup_codes = :backup_codes
            WHERE uuid = :uuid
            """,
            {
                "backup_codes": hashed_backup_codes,
                "uuid": self.user["uuid"],
            },
        )
