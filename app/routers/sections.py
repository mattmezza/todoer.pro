from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form
from fastapi.responses import HTMLResponse

from app.dependencies import RepoDB, get_current_active_user, repo_db
from app import http, templating

router = APIRouter()


@router.delete("/section/{sid}", response_class=HTMLResponse)
async def delete(
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).tasks().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"section-delete, section-{sid}-delete"
    return ""


@router.get("/section/{sid}")
@templating.htmx("Section.jinja")
async def view(
    sid: str,
    view_complete: bool = False,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user)
    serepo = repo.sections()
    section = http.Opt(await serepo.by_sid(sid)).or_404(
        f"Section #{sid} not found."
    )
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    return {
        "section": section,
        "tasks": tasks,
        "view_complete": view_complete,
        "user": user,
    }


@router.post("/sections")
@templating.htmx("Section.jinja")
async def add(
    title: Annotated[str, Form()],
    list_id: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    section = http.Opt(
        await db.for_user(user).sections().create(title, list_id)
    ).or_400()
    return {
        "section": section,
        "user": user,
        "tasks": [],
        "tasks_incomplete": [],
        "tasks_complete": [],
        "view_complete": False,
    }, 201, {"HX-Trigger":  "section-create"}
