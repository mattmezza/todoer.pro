from collections import defaultdict
from typing import Mapping

from .utils import LoggedInUserRepo


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
