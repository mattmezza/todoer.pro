from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Body, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import http, svc, tpl
from app.deps import RepoDB, get_current_active_user, repo_db
from app.utils import slugify_title

router = APIRouter()


@router.patch("/section/{sid}/archive", response_class=HTMLResponse)
async def archive(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).sections().archive(sid)
    return HTMLResponse(
        "",
        headers={"HX-Trigger": f"section-delete, section-{sid}-delete"},
    )


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
    section = dict(
        http.Opt(await serepo.by_sid(sid)).or_404(f"Section #{sid} not found.")
    )
    sections = await serepo.all_by_list(section["list"])
    if len(sections) <= 2:
        section["is_first"] = True
        section["is_last"] = True
    else:
        section["is_first"] = sections[1]["sid"] == sid
        section["is_last"] = sections[-1]["sid"] == sid
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    list_ = await repo.lists().by_uuid(section["list"])
    sort_by = list_.get("sort_by", "manual") if list_ else "manual"
    svc.lists.sort_tasks(tasks, sort_by)
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["sid"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return {
        "section": section,
        "sections": sections,
        "sections_by_list": sections_by_list,
        "tasks": tasks,
        "lists": lists,
        "list": list_,
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
    serepo = urepo.sections()
    section = dict(
        http.Opt(await serepo.by_sid(sid)).or_404(f"Section #{sid} not found.")
    )
    sections = await serepo.all_by_list(section["list"])
    if len(sections) <= 2:
        section["is_first"] = True
        section["is_last"] = True
    else:
        section["is_first"] = sections[1]["sid"] == sid
        section["is_last"] = sections[-1]["sid"] == sid
    tasks = await urepo.tasks().by_section(section["uuid"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["sid"])
    return {
        "section": section,
        "tasks": tasks,
        "list": list_,
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
    serepo = urepo.sections()
    section = dict(
        http.Opt(await serepo.update_title_by_sid(slugify_title(title), sid)).or_404(
            f"Section #{sid} not found."
        )
    )
    sections = await serepo.all_by_list(section["list"])
    if len(sections) <= 2:
        section["is_first"] = True
        section["is_last"] = True
    else:
        section["is_first"] = sections[1]["sid"] == sid
        section["is_last"] = sections[-1]["sid"] == sid
    tasks = await urepo.tasks().by_section(section["uuid"])
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["sid"])
    return (
        {
            "section": section,
            "tasks": tasks,
            "list": list_,
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
    serepo = urepo.sections()
    section = dict(http.Opt(await serepo.insert(slugify_title(title), list_id)).or_400())
    section["is_first"] = section["order"] == 1
    section["is_last"] = True
    list_ = await urepo.lists().by_uuid(section["list"])
    space = await urepo.spaces().by_uuid(list_["space"])
    lists = await urepo.lists().all_by_space(space["sid"])
    sections = await serepo.all_by_list(section["list"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return (
        {
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "user": user,
            "lists": lists,
            "list": list_,
            "tasks": [],
            "tasks_incomplete": [],
            "tasks_complete": [],
            "view_complete": False,
        },
        201,
        {"HX-Trigger": f"section-create, list-update, list-{list_id}-update"},
    )


@router.put("/section/{sid}/list")
async def update_list(
    req: Request,
    sid: str,
    list: Annotated[UUID, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    _ = http.Opt(
        await db.for_user(user).sections().update_list_by_sid(list, sid)
    ).or_404(f"Section #{sid} not found.")
    return HTMLResponse(
        content="",
        status_code=200,
        headers={"HX-Trigger": f"section-update, section-{sid}-update"},
    )


@router.put("/sections/reorder", response_class=HTMLResponse)
async def reorder_sections(
    req: Request,
    sids: Annotated[list[str], Body()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).sections().reorder(sids)
    return HTMLResponse(
        "",
        status_code=200,
        headers={"HX-Trigger": "list-update, sections-reorder"},
    )


@router.patch("/section/{sid}/move-up")
@tpl.htmx("Section")
async def move_up(
    req: Request,
    res: HTMLResponse,
    sid: str,
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    serepo = repo.sections()
    section = dict(
        http.Opt(await serepo.move_up(sid)).or_404(
            f"Section #{sid} not found."
        )
    )
    sections = await serepo.all_by_list(section["list"])
    if len(sections) <= 2:
        section["is_first"] = True
        section["is_last"] = True
    else:
        section["is_first"] = sections[1]["sid"] == sid
        section["is_last"] = sections[-1]["sid"] == sid
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    tasks = _add_first_last_tasks(tasks)
    list_ = await repo.lists().by_uuid(section["list"])
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["sid"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return (
        {
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "tasks": tasks,
            "lists": lists,
            "list": list_,
            "view_complete": view_complete,
            "user": user,
        },
        200,
        {"HX-Trigger": f"list-update, list-{list_['sid']}-update"},
    )


@router.patch("/section/{sid}/move-down")
@tpl.htmx("Section")
async def move_down(
    req: Request,
    res: HTMLResponse,
    sid: str,
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    serepo = repo.sections()
    section = dict(
        http.Opt(await serepo.move_down(sid)).or_404(
            f"Section #{sid} not found."
        )
    )
    sections = await serepo.all_by_list(section["list"])
    if len(sections) <= 2:
        section["is_first"] = True
        section["is_last"] = True
    else:
        section["is_first"] = sections[1]["sid"] == sid
        section["is_last"] = sections[-1]["sid"] == sid
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    tasks = _add_first_last_tasks(tasks)
    list_ = await repo.lists().by_uuid(section["list"])
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["sid"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return (
        {
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "tasks": tasks,
            "lists": lists,
            "list": list_,
            "view_complete": view_complete,
            "user": user,
        },
        200,
        {"HX-Trigger": f"list-update, list-{list_['sid']}-update"},
    )


@router.patch("/section/{sid}/move-top")
@tpl.htmx("Section")
async def move_top(
    req: Request,
    res: HTMLResponse,
    sid: str,
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    serepo = repo.sections()
    section = dict(
        http.Opt(await serepo.move_top(sid)).or_404(
            f"Section #{sid} not found."
        )
    )
    sections = await serepo.all_by_list(section["list"])
    section["is_first"] = True
    section["is_last"] = False
    if len(sections) <= 2:
        section["is_last"] = True
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    tasks = _add_first_last_tasks(tasks)
    list_ = await repo.lists().by_uuid(section["list"])
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["sid"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return (
        {
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "tasks": tasks,
            "lists": lists,
            "list": list_,
            "view_complete": view_complete,
            "user": user,
        },
        200,
        {"HX-Trigger": f"list-update, list-{list_['sid']}-update"},
    )


@router.patch("/section/{sid}/move-bottom")
@tpl.htmx("Section")
async def move_bottom(
    req: Request,
    res: HTMLResponse,
    sid: str,
    view_complete: bool = False,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    repo = db.for_user(user)
    serepo = repo.sections()
    section = dict(
        http.Opt(await serepo.move_bottom(sid)).or_404(
            f"Section #{sid} not found."
        )
    )
    sections = await serepo.all_by_list(section["list"])
    section["is_first"] = False
    section["is_last"] = True
    if len(sections) <= 2:
        section["is_first"] = True
    tarepo = repo.tasks()
    tasks = await tarepo.by_section(section["uuid"])
    tasks = _add_first_last_tasks(tasks)
    list_ = await repo.lists().by_uuid(section["list"])
    space = await repo.spaces().by_uuid(list_["space"])
    lists = await repo.lists().all_by_space(space["sid"])
    all_sections = await serepo.all_by_space(space["uuid"])
    sections_by_list = serepo.group_by_list(all_sections)
    return (
        {
            "section": section,
            "sections": sections,
            "sections_by_list": sections_by_list,
            "tasks": tasks,
            "lists": lists,
            "list": list_,
            "view_complete": view_complete,
            "user": user,
        },
        200,
        {"HX-Trigger": f"list-update, list-{list_['sid']}-update"},
    )


def _add_first_last_tasks(tasks: list[Mapping]) -> list[dict]:
    incomplete = [i for i, t in enumerate(tasks) if not t["complete"]]
    for idx, ta in enumerate(tasks):
        tasks[idx] = dict(ta)
        if not tasks[idx]["complete"] and incomplete:
            tasks[idx]["is_first"] = idx == incomplete[0]
            tasks[idx]["is_last"] = idx == incomplete[-1]
        else:
            tasks[idx]["is_first"] = True
            tasks[idx]["is_last"] = True
    return tasks
