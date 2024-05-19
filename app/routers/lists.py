from collections import defaultdict
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from app import http, svc, tpl
from app.deps import (
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
    urepo = db.for_user(user)
    lrepo = urepo.lists()
    list_ = http.Opt(await lrepo.by_sid(sid)).or_404(f"List #{sid} not found.")
    space = await urepo.spaces().by_uuid(list_["space"])
    await lrepo.del_by_sid(sid)
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = f"list-delete, list-{sid}-delete"
    res.headers["HX-Redirect"] = f"/space/{space['sid']}"
    return res


@router.get("/list/{sid}")
@tpl.htmx("Page.List")
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
    tasks_by_section = await tarepo.by_section_for_list(li["uuid"])
    return {
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
@tpl.htmx("Lists.Color")
async def update_color(
    req: Request,
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
    req: Request,
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
    req: Request,
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
@tpl.htmx("Nav.Lists.Item")
async def add(
    req: Request,
    space: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    color: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li, _ = await svc.lists.create(
        user, db, color, title, descr, space, default=False
    )
    return {"list": li, "user": user}, 201, {"HX-Trigger": "list-create"}
