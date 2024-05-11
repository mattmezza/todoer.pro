from typing import Mapping

from .utils import LoggedInUserRepo


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
