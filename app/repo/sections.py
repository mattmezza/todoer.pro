from typing import Mapping
from uuid import UUID

from .utils import LoggedInUserRepo, new_ids


class SectionForUser(LoggedInUserRepo):
    async def by_sid(self, sid: str) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            SELECT se.* FROM sections se
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.sid = :sid
                AND sp.creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        return section._mapping if section else None

    async def by_uuid(self, uuid: str) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            SELECT se.* FROM sections se
            JOIN lists li ON li.uuid = se.list
            JOIN spaces sp ON sp.uuid = li.space
            WHERE se.uuid = :uuid
                AND sp.creator = :creator
            """,
            {"uuid": uuid, "creator": self.user["uuid"]},
        )
        return section._mapping if section else None

    async def by_list(self, list_uuid: str, limit: int = 100) -> list[Mapping]:
        sections = await self.db.fetch_all(
            """
            SELECT se.* FROM sections se
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND se.list = :list_uuid
            ORDER BY se.`order` ASC, se.created ASC
            LIMIT :limit
            """,
            {
                "limit": limit,
                "list_uuid": list_uuid,
                "creator": self.user["uuid"],
            },
        )
        res = [dict(ss._mapping) for ss in sections]
        for i, ss in enumerate(res):
            ss["is_first"] = i == 1 or ss["default"] is True
            ss["is_last"] = i == len(res) - 1 or ss["default"] is True
        return res

    async def update_title_by_sid(
        self, title: str, sid: str
    ) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            UPDATE sections
                SET title = :title, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT s.sid FROM sections s
                        JOIN lists l ON l.uuid = s.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
            RETURNING *
            """,
            {"sid": sid, "title": title, "creator": self.user["uuid"]},
        )
        return None if section is None else section._mapping

    async def del_by_sid(self, sid: str) -> None:
        async with self.db.transaction():
            section = await self.db.fetch_one(
                """
                DELETE FROM sections
                WHERE sid = :sid
                    AND EXISTS (
                        SELECT s.sid FROM sections s
                        JOIN lists l ON l.uuid = s.list
                        JOIN spaces sp ON sp.uuid = l.space
                        WHERE sp.creator = :creator
                    )
                RETURNING *
                """,
                {"sid": sid, "creator": self.user["uuid"]},
            )
            if section is None:
                return
            await self.db.execute(
                """
                UPDATE sections
                SET `order` = `order` - 1
                WHERE `order` > :order
                AND list = :list
                AND creator = :creator
                """,
                {
                    "order": section["order"],
                    "list": section["list"],
                    "creator": self.user["uuid"],
                },
            )

    async def all_by_list(
        self, list_uuid: str, limit: int = 100
    ) -> Mapping | None:
        sections = await self.db.fetch_all(
            """
            SELECT se.* FROM sections se
            JOIN lists l ON l.uuid = se.list
            JOIN spaces s ON s.uuid = l.space
            WHERE s.creator = :creator
                AND l.uuid = :list
            ORDER BY se.`order` ASC, se.created ASC
            LIMIT :limit
            """,
            {"limit": limit, "creator": self.user["uuid"], "list": list_uuid},
        )
        res = [dict(ss._mapping) for ss in sections]
        for i, ss in enumerate(res):
            ss["is_first"] = i == 1 or ss["default"] is True
            ss["is_last"] = i == len(res) - 1 and ss["default"] is True
        return res

    async def insert(
        self, title: str, list_id: str, default: bool = False
    ) -> Mapping | None:
        uuid_new, sid_new = new_ids()
        section = await self.db.fetch_one(
            """INSERT INTO sections (
                uuid,
                sid,
                `order`,
                `default`,
                title,
                creator,
                list
            )
            VALUES (
                :uuid,
                :sid,
                COALESCE((SELECT MAX(`order`) + 1 FROM sections WHERE list = :list), 1),
                :default,
                :title,
                :creator,
                :list
            )
            RETURNING *""",
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title.strip(),
                "default": 1 if default else 0,
                "creator": self.user["uuid"],
                "list": list_id,
            },
        )
        return section._mapping if section else None

    async def update_list_by_sid(self, list: UUID, sid: str) -> Mapping | None:
        section = await self.db.fetch_one(
            """
            UPDATE sections
                SET list = :list, updated = CURRENT_TIMESTAMP
                WHERE sid = :sid
                AND creator = :creator
            RETURNING *
            """,
            {"sid": sid, "list": str(list), "creator": self.user["uuid"]},
        )
        return section._mapping if section is not None else None

    async def move_up(self, sid: str) -> None:
        section = await self.db.fetch_one(
            """
            SELECT * FROM sections
            WHERE sid = :sid
            AND creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        section_before = await self.db.fetch_one(
            """
            SELECT * FROM sections
            WHERE `order` < :order
            AND `default` = 0
            AND list = :list
            AND creator = :creator
            ORDER BY `order` DESC
            LIMIT 1
            """,
            {
                "order": section["order"],
                "list": section["list"],
                "creator": self.user["uuid"],
            },
        )
        if section_before is None:
            return section._mapping if section is not None else None
        order_before = section_before["order"]
        section_before = await self.db.fetch_one(
            """
            UPDATE sections
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
            RETURNING *
            """,
            {
                "sid": section_before["sid"],
                "order": section["order"],
            },
        )
        section = await self.db.fetch_one(
            """
            UPDATE sections
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
            RETURNING *
            """,
            {
                "sid": sid,
                "order": order_before,
            },
        )
        return section._mapping if section is not None else None

    async def move_down(self, sid: str) -> None:
        section = await self.db.fetch_one(
            """
            SELECT * FROM sections
            WHERE sid = :sid
            AND creator = :creator
            """,
            {"sid": sid, "creator": self.user["uuid"]},
        )
        section_after = await self.db.fetch_one(
            """
            SELECT * FROM sections
            WHERE `order` > :order
            AND list = :list
            AND creator = :creator
            ORDER BY `order` ASC
            LIMIT 1
            """,
            {
                "order": section["order"],
                "list": section["list"],
                "creator": self.user["uuid"],
            },
        )
        if section_after is None:
            return section._mapping if section is not None else None
        order_after = section_after["order"]
        section_after = await self.db.fetch_one(
            """
            UPDATE sections
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
            RETURNING *
            """,
            {
                "sid": section_after["sid"],
                "order": section["order"],
            },
        )
        section = await self.db.fetch_one(
            """
            UPDATE sections
                SET `order` = :order,
                updated = CURRENT_TIMESTAMP
            WHERE sid = :sid
            RETURNING *
            """,
            {
                "sid": sid,
                "order": order_after,
            },
        )
        return section._mapping if section is not None else None

    async def move_top(self, sid: str) -> None:
        async with self.db.transaction():
            section = await self.db.fetch_one(
                """
                SELECT * FROM sections
                WHERE sid = :sid
                AND creator = :creator
                """,
                {"sid": sid, "creator": self.user["uuid"]},
            )
            section = await self.db.fetch_one(
                """
                UPDATE sections
                    SET `order` = 1
                    WHERE sid = :sid
                    AND list = :list
                    AND creator = :creator
                RETURNING *
                """,
                {
                    "sid": sid,
                    "list": section["list"],
                    "creator": self.user["uuid"],
                },
            )
            await self.db.execute(
                """
                UPDATE sections
                    SET `order` = `order` + 1
                    WHERE sid != :sid
                    AND `order` > 0
                    AND `default` = 0
                    AND list = :list
                    AND creator = :creator
                """,
                {
                    "sid": sid,
                    "list": section["list"],
                    "creator": self.user["uuid"],
                },
            )
            return section._mapping if section is not None else None

    async def move_bottom(self, sid: str) -> None:
        async with self.db.transaction():
            section = await self.db.fetch_one(
                """
                SELECT * FROM sections
                WHERE sid = :sid
                AND creator = :creator
                """,
                {"sid": sid, "creator": self.user["uuid"]},
            )
            order_old = section["order"]
            section = await self.db.fetch_one(
                """
                UPDATE sections
                    SET `order` = (
                        SELECT MAX(`order`) FROM sections
                            WHERE list = :list
                            AND creator = :creator
                        )
                    WHERE sid = :sid
                    AND list = :list
                    AND creator = :creator
                    RETURNING *
                """,
                {
                    "sid": sid,
                    "list": section["list"],
                    "creator": self.user["uuid"],
                },
            )
            await self.db.execute(
                """
                UPDATE sections
                    SET `order` = `order` - 1
                    WHERE sid != :sid
                    AND `order` > :order
                    AND `default` = 0
                    AND list = :list
                    AND creator = :creator
                """,
                {
                    "sid": sid,
                    "list": section["list"],
                    "creator": self.user["uuid"],
                    "order": order_old,
                },
            )
            return section._mapping if section is not None else None
