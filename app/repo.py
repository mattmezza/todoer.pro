from databases import Database

from typing import Mapping


class LoggedInUserRepo:
    def __init__(self, user: Mapping, db: Database):
        self.user = user
        self.db = db


class SpaceForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        space = await self.db.fetch_one(
            """
            SELECT * FROM spaces
            WHERE sid = :sid
                AND creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return space._mapping if space else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        space = await self.db.fetch_one(
            """
            SELECT * FROM spaces
            WHERE uuid = :uuid
                AND creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return space._mapping if space else None

    async def all(self, limit: int = 100) -> list[Mapping]:
        spaces = await self.db.fetch_all(
            """
            SELECT * FROM spaces
            WHERE creator = :creator
            ORDER BY created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"]},
        )
        return [s._mapping for s in spaces]


class ListForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        list_ = await self.db.fetch_one(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE l.sid = :sid
                AND s.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return list_._mapping if list_ else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        list_ = await self.db.fetch_one(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE l.uuid = :uuid
                AND s.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return list_._mapping if list_ else None

    async def all(self, limit: int = 100) -> list[Mapping]:
        lists = await self.db.fetch_all(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
            ORDER BY l.created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"]},
        )
        return [ll._mapping for ll in lists]

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            "DELETE FROM lists WHERE sid = :sid AND creator = :creator",
            {"sid": sid, "creator": self.user["uuid"]},
        )


class SectionForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None: ...

    async def by_uuid(self, uuid: str) -> Mapping | None: ...

    async def by_list(self, list_uuid: str, limit: int = 100) -> list[Mapping]:
        sections = await self.db.fetch_all(
            """
            SELECT se.* FROM sections se
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND se.list = :list_uuid
            ORDER BY se.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "list_uuid": list_uuid,
                "creator": self.user["uuid"],
            },
        )
        return [ss._mapping for ss in sections]


class ForUser:
    def __init__(self, db: Database, user: Mapping):
        self.db = db
        self.user = user

    def spaces(self) -> SpaceForUser:
        return SpaceForUser(self.user, self.db)

    def lists(self) -> ListForUser:
        return ListForUser(self.user, self.db)

    def sections(self) -> SectionForUser:
        return SectionForUser(self.user, self.db)


class RepoDB:
    def __init__(self, db: Database):
        self.db = db

    def for_user(self, user: Mapping) -> ForUser:
        return ForUser(self.db, user)
