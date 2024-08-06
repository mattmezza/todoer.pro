from typing import Mapping


from app.repo import RepoDB


DEFAULT_COLOR = "#FF0000"
LIST_INBOX_COLOR = DEFAULT_COLOR
LIST_INBOX_TITLE = "inbox"
LIST_INBOX_DESCR = None
SECTION_INBOX_TITLE = "inbox"


async def create(
    user: Mapping,
    db: RepoDB,
    color: str,
    title: str,
    descr: str | None,
    space_uuid: str,
    default: bool,
) -> tuple[Mapping, Mapping]:
    urepo = db.for_user(user)
    async with db.transaction():
        li = await urepo.lists().insert(
            color, title, descr, space_uuid, default
        )
        se = await urepo.sections().insert(
            SECTION_INBOX_TITLE, li["uuid"], default=True
        )
        return li, se
