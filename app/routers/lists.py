from collections import defaultdict
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from app import http, tpl
from app.dependencies import (
    Renderer,
    get_current_active_user,
    renderer,
    repo_db,
)
from app.repo import RepoDB

router = APIRouter()


@router.delete("/list/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    await db.for_user(user).lists().del_by_sid(sid)
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = f"list-delete, list-{sid}-delete"
    return res


@router.get("/list/{sid}")
@tpl.htmx("Page/List")
async def view(
    req: Request,
    sid: str,
    view_complete: bool = False,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
    render: Renderer = Depends(renderer),
):
    user_repo = db.for_user(user)
    list_r = user_repo.lists()
    li = http.Opt(await list_r.by_sid(sid)).or_404(f"List #{sid} not found.")

    space_r = user_repo.spaces()
    section_r = user_repo.sections()
    space = await space_r.by_uuid(li["space"])
    sections = await section_r.by_list(li["uuid"])
    tarepo = user_repo.tasks()
    tasks_by_section = defaultdict(list)
    tasks = await tarepo.by_list(li["uuid"])
    for t in tasks:
        tasks_by_section[t["section"]].append(t)
    return {
        "request": req,
        "spaces": await space_r.all(),
        "space": space,
        "lists": await list_r.all_by_space(space["sid"]),
        "list": li,
        "sections": sections,
        "view_complete": view_complete,
        "user": user,
        "tasks_by_section": tasks_by_section,
    }


@router.put("/list/{sid}/color")
@tpl.htmx("Lists/Color")
async def update_color(
    res: HTMLResponse,
    sid: str,
    color: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    render: Renderer = Depends(renderer),
    db: RepoDB = Depends(repo_db),
):
    col = http.Opt(
        await db.for_user(user).lists().update_color_by_sid(color, sid)
    ).or_404(f"List #{sid} not found.")
    return (
        dict(color=col),
        200,
        {"HX-Trigger": f"list-update, list-{sid}-update"},
    )


@router.put("/list/{sid}/title")
async def update_title(
    res: HTMLResponse,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    t = http.Opt(
        await db.for_user(user).lists().update_title_by_sid(title, sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse(t)
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return res


@router.put("/list/{sid}/descr")
async def update_descr(
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    d = http.Opt(
        await db.for_user(user).lists().update_descr_by_sid(descr, sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse(d)
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return res


@router.post("/lists")
@tpl.htmx("Nav/Lists/Item.jinja")
async def add(
    req: Request,
    space: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    color: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li = http.Opt(
        await db.for_user(user).lists().create(color, title, descr, space)
    ).or_400()
    return {"list": li, "user": user}, 201, {"HX-Trigger": "list-create"}
