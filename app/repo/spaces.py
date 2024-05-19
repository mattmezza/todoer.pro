from collections import defaultdict
from typing import Mapping

from .utils import LoggedInUserRepo, new_ids


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
            JOIN lists l ON l.space = s.uuid
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

    async def insert(
        self, title: str, descr: str | None, default: bool = False
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        sp = await self.db.fetch_one(
            """INSERT INTO spaces (
                uuid,
                sid,
                title,
                descr,
                `default`,
                creator
            )
            VALUES (
                :uuid,
                :sid,
                :title,
                :descr,
                :default,
                :creator
            )
            RETURNING *""",
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "default": 1 if default else 0,
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

    async def default(self) -> Mapping | None:
        space = await self.db.fetch_one(
            """
            SELECT * FROM spaces
            WHERE `default` = 1
                AND creator = :creator
            """,
            {"creator": self.user["uuid"]},
        )
        return space._mapping if space else None
