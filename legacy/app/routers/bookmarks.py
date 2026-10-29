from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.post("/bookmarks")
@tpl.htmx("Bookmarks.Toggle")
async def toggle(
    req: Request,
    entity_type: Annotated[str, Form()],
    entity_uuid: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user).bookmarks()
    is_bookmarked = await repo.is_bookmarked(entity_type, entity_uuid)
    if is_bookmarked:
        await repo.del_by_entity(entity_type, entity_uuid)
        bookmarked = False
    else:
        await repo.create(entity_type, entity_uuid)
        bookmarked = True
    return (
        {
            "entity_type": entity_type,
            "entity_uuid": entity_uuid,
            "bookmarked": bookmarked,
        },
        200,
        {"HX-Trigger": "bookmark-update"},
    )


@router.delete("/bookmark/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    await db.for_user(user).bookmarks().del_by_sid(sid)
    res.headers["HX-Trigger"] = "bookmark-update"
    return ""
