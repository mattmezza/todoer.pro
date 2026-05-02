from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.delete("/comment/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).comments().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"task-update, comment-delete, comment-{sid}-delete"
    return ""


@router.put("/comment/{sid}")
@tpl.htmx("Comment")
async def update(
    req: Request,
    sid: str,
    content: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    comment = http.Opt(
        await db.for_user(user).comments().update_by_sid(content.strip(), sid)
    ).or_404(f"Comment #{sid} not found.")
    return (
        {
            "comment": dict(comment)
            | {
                "author_email_hash": user["email_hash"],
                "author_email": user["email"],
            },
        },
        200,
        {"HX-Trigger": f"task-update, comment-update, comment-{sid}-update"},
    )


@router.post("/comments")
@tpl.htmx("Comment")
async def add(
    req: Request,
    task: Annotated[str, Form()],
    content: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    comment = http.Opt(
        await db.for_user(user).comments().create(content.strip(), task)
    ).or_400()
    return (
        {
            "comment": dict(comment)
            | {
                "author_email_hash": user["email_hash"],
                "author_email": user["email"],
            },
        },
        201,
        {"HX-Trigger": "task-update, comment-create"},
    )
