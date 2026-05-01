from typing import Annotated, Mapping
from uuid import UUID

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from app import http, svc, tpl
from app.deps import Renderer, get_current_active_user, renderer, repo_db
from app.repo import RepoDB

router = APIRouter()


@router.patch("/list/{sid}/archive", response_class=HTMLResponse)
async def archive(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_user(user)
    lrepo = urepo.lists()
    list_ = http.Opt(await lrepo.by_sid(sid)).or_404(
        f"List #{sid} not found."
    )
    space = await urepo.spaces().by_uuid(list_["space"])
    await lrepo.archive(sid)
    return HTMLResponse(
        "",
        headers={"HX-Redirect": f"/space/{space['sid']}"},
    )


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
    tasks_by_section = await tarepo.by_section_for_list(li["uuid"])
    for tt in tasks_by_section.values():
        incomplete = [i for i, t in enumerate(tt) if not t["complete"]]
        for idx, ta in enumerate(tt):
            tt[idx] = dict(ta)
            if not tt[idx]["complete"] and incomplete:
                tt[idx]["is_first"] = idx == incomplete[0]
                tt[idx]["is_last"] = idx == incomplete[-1]
            else:
                tt[idx]["is_first"] = True
                tt[idx]["is_last"] = True
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
        await db.for_user(user).lists().update_color_by_sid(color.strip(), sid)
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
        await db.for_user(user).lists().update_title_by_sid(title.strip(), sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse("#" + t)
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return res


@router.put("/list/{sid}/descr", response_class=HTMLResponse)
@tpl.htmx("Lists.Descr")
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    li = http.Opt(
        await db.for_user(user)
        .lists()
        .update_descr_by_sid(descr.strip() if descr else None, sid)
    ).or_404(f"List #{sid} not found.")
    res.headers["HX-Trigger"] = f"list-update, list-{sid}-update"
    return {"list": li}


@router.put("/list/{sid}/space")
async def update_space(
    req: Request,
    res: HTMLResponse,
    sid: str,
    space: Annotated[UUID, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    _ = http.Opt(
        await db.for_user(user).lists().update_space_by_sid(space, sid)
    ).or_404(f"List #{sid} not found.")
    res = PlainTextResponse()
    res.headers["HX-Redirect"] = f"/list/{sid}"
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
        user,
        db,
        color if color else svc.lists.DEFAULT_COLOR,
        title.strip(),
        descr.strip() if descr else None,
        space,
        default=False,
    )
    return (
        {"list": li, "user": user},
        201,
        {"HX-Trigger": "list-create", "HX-Redirect": f"/list/{li.sid}"},
    )
