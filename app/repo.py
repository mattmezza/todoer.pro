import uuid
from collections import defaultdict
from typing import Mapping

import shortuuid
from databases import Database


def new_ids(max_len: int = 7) -> tuple[str, str]:
    u = uuid.uuid4()
    s = shortuuid.encode(u)[:max_len]
    return str(u), s


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

    async def by_sid_with_tot_lists(self, sid: str) -> Mapping | None:
        space = await self.db.fetch_one(
            """
            SELECT
               s.*,
                COUNT(l.uuid) AS lists
            FROM spaces s
            LEFT JOIN lists l ON l.space = s.uuid
            WHERE s.creator = :creator
                AND s.sid = :sid
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return space._mapping if space else None

    async def num_tasks(self, sid: str) -> Mapping | None:
        space = await self.db.fetch_one(
            """
            SELECT
                sp.uuid as space,
                count(t.uuid) as tasks_tot
            FROM tasks t
            JOIN sections se on se.uuid = t.section
            JOIN lists l on l.uuid = se.list
            JOIN spaces sp on sp.uuid = l.space
            WHERE sp.creator = :creator
                AND sp.sid = :sid
            GROUP BY sp.uuid
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

    async def all_with_tot_lists(self, limit: int = 100) -> list[Mapping]:
        spaces = await self.db.fetch_all(
            """
            SELECT
               s.*,
                COUNT(l.uuid) AS lists
            FROM spaces s
            LEFT JOIN lists l ON l.space = s.uuid
            WHERE s.creator = :creator
            GROUP BY s.uuid
            ORDER BY s.created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"]},
        )
        return [s._mapping for s in spaces]

    async def tasks_by_space(self, limit: int = 100) -> list[Mapping]:
        num_tasks = await self.db.fetch_all(
            """
            SELECT
                sp.uuid as space,
                count(t.uuid) as tasks_tot
            FROM tasks t
            JOIN sections se on se.uuid = t.section
            JOIN lists l on l.uuid = se.list
            JOIN spaces sp on sp.uuid = l.space
            WHERE sp.creator = :creator
            GROUP BY sp.uuid
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"]},
        )
        tasks_by_space = defaultdict(lambda: 0)
        for num_task in num_tasks:
            tasks_by_space[num_task.space] = num_task.tasks_tot
        return tasks_by_space

    async def insert(self, title: str, descr: str | None) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        sp = await self.db.fetch_one(
            """INSERT INTO spaces (
                uuid,
                sid,
                title,
                descr,
                creator
            )
            VALUES (
                :uuid,
                :sid,
                :title,
                :descr,
                :creator
            )
            RETURNING *""",
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "creator": self.user["uuid"],
            },
        )
        return sp._mapping

    async def delete(self, sid: str) -> None:
        await self.db.execute(
            """
            DELETE FROM spaces
                WHERE sid = :sid AND creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def update_title(self, sid: str, title: str) -> str | None:
        sp = await self.db.fetch_one(
            """
            UPDATE spaces
                SET title = :title
                WHERE sid = :sid
                AND creator = :creator
            RETURNING title
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return None if sp is None else sp.title

    async def update_descr(self, sid: str, descr: str | None) -> str | None:
        se = await self.db.fetch_one(
            """
            UPDATE spaces
                SET descr = :descr
                WHERE sid = :sid
                AND creator = :creator
            RETURNING descr
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return None if se is None else se.descr


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

    async def all_by_space(self, sid: str, limit: int = 100) -> Mapping | None:
        lists = await self.db.fetch_all(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND s.sid = :sid
            ORDER BY l.created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"], "sid": sid},
        )
        return [li._mapping for li in lists]

    async def num_sections_by_list(self, sid: str) -> Mapping | None:
        num_sections_in_list = await self.db.fetch_all(
            """
            SELECT
                l.uuid as list,
                count(se.uuid) as sections_tot
            FROM sections se
            JOIN lists l on l.uuid = se.list
            JOIN spaces sp on sp.uuid = l.space
            WHERE sp.creator = :creator
                AND sp.sid = :sid
            GROUP BY l.uuid
            """,
            {"creator": self.user["uuid"], "sid": sid},
        )
        sections_by_list = defaultdict(lambda: 0)
        for num_section in num_sections_in_list:
            sections_by_list[num_section.list] = num_section.sections_tot
        return sections_by_list

    async def num_tasks_by_list(self, sid: str) -> Mapping | None:
        num_tasks_in_list = await self.db.fetch_all(
            """
            SELECT
                l.uuid as list,
                count(t.uuid) as tasks_tot
            FROM tasks t
            JOIN sections se on se.uuid = t.section
            JOIN lists l on l.uuid = se.list
            JOIN spaces sp on sp.uuid = l.space
            WHERE sp.creator = :creator
                AND sp.sid = :sid
            GROUP BY l.uuid
            """,
            {"creator": self.user["uuid"], "sid": sid},
        )
        tasks_by_list = defaultdict(lambda: 0)
        for num_task in num_tasks_in_list:
            tasks_by_list[num_task.list] = num_task.tasks_tot
        return tasks_by_list

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            "DELETE FROM lists WHERE sid = :sid AND creator = :creator",
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def update_title_by_sid(self, title: str, sid: str) -> str | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET title = :title
                WHERE sid = :sid
                AND creator = :creator
            RETURNING title
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return li.title if li is not None else None

    async def update_descr_by_sid(self, descr: str, sid: str) -> str | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET descr = :descr
                WHERE sid = :sid
                AND creator = :creator
            RETURNING descr
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return li.descr if li is not None else None

    async def update_color_by_sid(self, color: str, sid: str) -> str | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET color = :color
                WHERE sid = :sid
                AND creator = :creator
            RETURNING color
            """,
            {"sid": sid, "color": color, "creator": self.user["uuid"]},
        )
        return li.color if li is not None else None


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
