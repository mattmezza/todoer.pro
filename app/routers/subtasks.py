from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form
from fastapi.responses import HTMLResponse

from app import http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.delete("/subtask/{sid}", response_class=HTMLResponse)
async def delete(
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).subtasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"subtask-delete, subtask-{sid}-delete"
    return ""


@router.post("/subtasks")
@tpl.htmx("Subtask")
async def add(
    task: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user).subtasks().create(title, descr, task)
    ).or_400()
    return (
        {
            "subtask": dict(subtask)
            | {"author_email_hash": user["email_hash"]},
        },
        201,
        {"HX-Trigger": "subtask-create"},
    )


@router.put("/subtask/{sid}")
@tpl.htmx("Subtask")
async def update(
    sid: str,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    complete: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    subtask = http.Opt(
        await db.for_user(user).subtasks().update(sid, title, descr, complete)
    ).or_400()
    hx_trigger = f"subtask-update, subtask-{sid}-update"
    if subtask.complete == 1:
        hx_trigger += f", subtask-complete, subtask-{sid}-complete"
    else:
        hx_trigger += f", subtask-incomplete, subtask-{sid}-incomplete"
    return {"subtask": subtask}, 200, {"HX-Trigger": hx_trigger}
