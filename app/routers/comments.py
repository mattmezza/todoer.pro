from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form
from fastapi.responses import HTMLResponse

from app import http, templating
from app.dependencies import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.delete("/comment/{sid}", response_class=HTMLResponse)
async def delete(
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).comments().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"comment-delete, comment-{sid}-delete"
    return res


@router.post("/comments")
@templating.htmx("Comment")
async def add(
    task: Annotated[str, Form()],
    content: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    comment = http.Opt(
        await db.for_user(user).comments().create(content, task)
    ).or_400()
    return (
        {
            "comment": dict(comment)
            | {"author_email_hash": user["email_hash"]},
            "user": user,
        },
        201,
        {"HX-Trigger": "comment-create"},
    )
