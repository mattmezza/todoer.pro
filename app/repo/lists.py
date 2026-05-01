from collections import defaultdict
from typing import Mapping
from uuid import UUID

from .utils import LoggedInUserRepo, new_ids


class ListForUser(LoggedInUserRepo):
    async def search(self, query: str, limit: int = 10) -> list[Mapping]:
        lists = await self.db.fetch_all(
            """
            SELECT l.sid, l.title, l.color, 'list' as type,
                   s.sid as space_sid, s.title as space_title
            FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
              AND l.archived = 0
              AND (l.title LIKE :query OR l.descr LIKE :query)
            ORDER BY l.updated DESC, l.created DESC
            LIMIT :limit
            """,
            {
                "creator": self.user["uuid"],
                "query": f"%{query}%",
                "limit": limit,
            },
        )
        return [ll._mapping for ll in lists]

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

    async def default_for_space(self, sid: str) -> Mapping | None:
        list_ = await self.db.fetch_one(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE l.`default` = 1
                AND s.sid = :sid
                AND s.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return list_._mapping if list_ else None

    async def all(self, limit: int = 100) -> list[Mapping]:
        lists = await self.db.fetch_all(
            """
            SELECT l.* FROM lists l
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND l.archived = 0
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
                AND l.archived = 0
            ORDER BY l.created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"], "sid": sid},
        )
        return [li._mapping for li in lists]

    async def num_sections_by_list_for_space(self, sid: str) -> Mapping | None:
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

    async def num_tasks_by_list_for_space(self, sid: str) -> Mapping | None:
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

    async def archive(self, sid: str) -> Mapping | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists SET archived = 1, updated = CURRENT_TIMESTAMP
            WHERE sid = :sid AND creator = :creator
            RETURNING *
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return li._mapping if li else None

    async def del_by_sid(self, sid: str) -> None:
        await self.db.execute(
            "DELETE FROM lists WHERE sid = :sid AND creator = :creator",
            {"sid": sid, "creator": self.user["uuid"]},
        )

    async def update_title_by_sid(self, title: str, sid: str) -> str | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET title = :title, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                AND creator = :creator
            RETURNING title
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return li.title if li is not None else None

    async def update_descr_by_sid(
        self, descr: str, sid: str
    ) -> Mapping | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET descr = :descr, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                AND creator = :creator
            RETURNING *
            """,
            {"sid": sid, "descr": descr, "creator": self.user["uuid"]},
        )
        return li._mapping if li is not None else None

    async def update_color_by_sid(self, color: str, sid: str) -> str | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET color = :color, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                AND creator = :creator
            RETURNING color
            """,
            {"sid": sid, "color": color, "creator": self.user["uuid"]},
        )
        return li.color if li is not None else None

    async def update_space_by_sid(
        self, space: UUID, sid: str
    ) -> Mapping | None:
        li = await self.db.fetch_one(
            """
            UPDATE lists
                SET space = :space, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                AND creator = :creator
            RETURNING *
            """,
            {"sid": sid, "space": str(space), "creator": self.user["uuid"]},
        )
        return li._mapping if li is not None else None

    async def insert(
        self,
        color: str | None,
        title: str,
        descr: str | None,
        space: str,
        default: bool = False,
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        uuid_new_section, sid_new_section = new_ids()
        async with self.db.transaction():
            li = await self.db.fetch_one(
                """
                INSERT INTO lists (
                    uuid, sid, title, descr, color, creator, space, `default`
                )
                VALUES (:uuid, :sid, :title, :descr, :color, :creator, :space, :default)
                RETURNING *, 0 as incomplete
                """,
                {
                    "uuid": uuid_new,
                    "sid": sid_new,
                    "space": space,
                    "title": title.strip(),
                    "descr": descr.strip() if descr else None,
                    "default": 1 if default else 0,
                    "color": color,
                    "creator": self.user["uuid"],
                },
            )
            return None if li is None else li._mapping
