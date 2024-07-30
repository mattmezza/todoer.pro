from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import http, tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.delete("/section/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).sections().del_by_sid(sid)
    res.headers["HX-Trigger"] = f"section-delete, section-{sid}-delete"
    return ""


@router.get("/section/{sid}")
@tpl.htmx("Section")
async def view(
    req: Request,
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
    list_ = await repo.lists().by_uuid(section["list"])
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["uuid"])
    return {
        "section": section,
        "tasks": tasks,
        "lists": lists,
        "view_complete": view_complete,
        "user": user,
    }


@router.get("/section/{sid}/title")
@tpl.htmx("Sections.Title")
async def read_title(
    req: Request,
    sid: str,
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    section = http.Opt(
        await urepo.sections().by_sid(sid)
    ).or_404(f"Section #{sid} not found.")
    tasks = await urepo.tasks().by_section(section["uuid"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["uuid"])
    return {
        "section": section,
        "tasks": tasks,
        "lists": lists,
        "view_complete": view_complete,
    }


@router.put("/section/{sid}/title")
@tpl.htmx("Sections.Title")
async def update_title(
    req: Request,
    sid: str,
    title: Annotated[str, Form()],
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    section = http.Opt(
        await urepo.sections().update_title_by_sid(title.strip(), sid)
    ).or_404(f"Section #{sid} not found.")
    tasks = await urepo.tasks().by_section(section["uuid"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["uuid"])
    return (
        {
            "section": section,
            "tasks": tasks,
            "lists": lists,
            "view_complete": view_complete,
        },
        200,
        {"HX-Trigger": f"section-update, section-{sid}-update"},
    )


@router.post("/sections")
@tpl.htmx("Section")
async def add(
    req: Request,
    title: Annotated[str, Form()],
    list_id: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    section = http.Opt(
        await urepo.sections().insert(title, list_id)
    ).or_400()
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["uuid"])
    return {
        "section": section,
        "user": user,
        "lists": lists,
        "tasks": [],
        "tasks_incomplete": [],
        "tasks_complete": [],
        "view_complete": False,
    }, 201, {"HX-Trigger":  "section-create"}


@router.put("/section/{sid}/list")
async def update_list(
    req: Request,
    res: HTMLResponse,
    sid: str,
    list: Annotated[UUID, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    _ = http.Opt(
        await db.for_user(user).sections().update_list_by_sid(list, sid)
    ).or_404(f"Section #{sid} not found.")
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = f"section-update, section-{sid}-update"
    return res
