from typing import Mapping

from app.repo import RepoDB


DEFAULT_COLOR = "#FF0000"
LIST_INBOX_COLOR = DEFAULT_COLOR
LIST_INBOX_TITLE = "inbox"
LIST_INBOX_DESCR = None
SECTION_INBOX_TITLE = "inbox"


def sort_tasks(tasks: list, sort_by: str) -> None:
    """Sort tasks in-place based on the sort_by setting."""
    if sort_by == "due-date":
        tasks.sort(
            key=lambda t: (
                t.get("complete", 0),
                t.get("due_date") or "9999-12-31",
                t.get("order", 0),
            )
        )
    elif sort_by == "created":
        tasks.sort(
            key=lambda t: (
                t.get("complete", 0),
                t.get("created") or "",
                t.get("order", 0),
            )
        )
    elif sort_by == "alphabetical":
        tasks.sort(
            key=lambda t: (
                t.get("complete", 0),
                (t.get("title") or "").lower(),
            )
        )
    elif sort_by == "priority":
        tasks.sort(
            key=lambda t: (
                t.get("complete", 0),
                t.get("priority") or 99,
                t.get("order", 0),
            )
        )
    # 'manual' is the default DB ordering, no re-sort needed


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
