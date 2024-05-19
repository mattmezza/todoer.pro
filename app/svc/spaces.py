from typing import Mapping


from app.repo import RepoDB
from . import lists


SPACE_INBOX_TITLE = "inbox"
SPACE_INBOX_DESCR = None


async def create(
    user: Mapping, db: RepoDB, title: str, descr: str, default: bool
) -> tuple[Mapping, Mapping, Mapping]:
    urepo = db.for_user(user)
    async with db.transaction():
        sp = await urepo.spaces().insert(title, descr, default)
        li, se = await lists.create(
            user,
            db,
            lists.LIST_INBOX_COLOR,
            lists.LIST_INBOX_TITLE,
            lists.LIST_INBOX_DESCR,
            sp["uuid"],
            default=True,
        )
        return sp, li, se
