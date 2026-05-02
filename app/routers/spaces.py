from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import http, svc, tpl
from app.deps import RepoDB, get_current_active_user, repo_db
from app.utils import slugify_title

router = APIRouter()


@router.get("/spaces")
@tpl.htmx("Page.Spaces")
async def index(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user).spaces()
    return dict(
        spaces=await repo.all_with_tot_lists(),
        tasks_by_space=await repo.tasks_by_space(),
        user=user,
    )


@router.get("/space/{sid}")
@tpl.htmx("Page.Space")
async def read(
    req: Request,
    sid: str,
    view_complete: bool = False,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user)
    repo_spaces = repo.spaces()
    repo_sections = repo.sections()
    space = http.Opt(await repo_spaces.by_sid(sid)).or_404(
        f"Space '{sid}' not found."
    )
    num_tasks = await repo_spaces.num_tasks(sid)
    repo_lists = repo.lists()
    lists = await repo_lists.all_by_space(sid)
    list_inbox = await repo_lists.default_for_space(sid)
    sections_inbox = await repo_sections.by_list(list_inbox["uuid"])
    repo_tasks = repo.tasks()
    tasks_by_section_inbox = await repo_tasks.by_section_for_list(
        list_inbox["uuid"]
    )
    for tt in tasks_by_section_inbox.values():
        for idx, ta in enumerate(tt):
            tt[idx] = dict(ta)
            tt[idx]["is_first"] = idx == 0
            tt[idx]["is_last"] = idx == len(tt) - 1
    sections_by_list = await repo_lists.num_sections_by_list_for_space(sid)
    tasks_by_list = await repo_lists.num_tasks_by_list_for_space(sid)
    is_bookmarked = await repo.bookmarks().is_bookmarked(
        "space", space["uuid"]
    )
    return (
        dict(
            spaces=await repo_spaces.all(),
            space=space,
            lists=lists,
            list_inbox=list_inbox,
            sections_inbox=sections_inbox,
            tasks_by_section_inbox=tasks_by_section_inbox,
            view_complete=view_complete,
            tasks_by_space=num_tasks["tasks_tot"] if num_tasks else 0,
            tasks_by_list=tasks_by_list,
            sections_by_list=sections_by_list,
            is_bookmarked=is_bookmarked,
            user=user,
        ),
        200,
        {"HX-Trigger": "space-read"},
    )


@router.post("/spaces", response_class=HTMLResponse)
@tpl.htmx("Spaces.ListItem")
async def add(
    req: Request,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    sp, *_ = await svc.spaces.create(
        user,
        db,
        slugify_title(title),
        descr.strip() if descr else None,
        default=False,
    )
    return (
        dict(
            space=dict(sp) | {"lists": 0},
            tasks_by_space={sp["uuid"]: 0},
            user=user,
        ),
        200,
        {"HX-Trigger": "space-create", "HX-Redirect": "/spaces"},
    )


@router.patch("/space/{sid}/archive", response_class=HTMLResponse)
async def archive(
    req: Request,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    await db.for_user(user).spaces().archive(sid)
    return HTMLResponse("", headers={"HX-Redirect": "/spaces"})


@router.delete("/space/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: HTMLResponse,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    await db.for_user(user).spaces().delete(sid)
    res.headers["HX-Trigger"] = "space-delete"
    res.headers["HX-Redirect"] = "/spaces"
    return ""


@router.put("/space/{sid}/title", response_class=HTMLResponse)
async def update_title(
    req: Request,
    res: HTMLResponse,
    sid: str,
    title: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    res.headers["HX-Trigger"] = "space-update"
    return "@" + await db.for_user(user).spaces().update_title(
        sid, slugify_title(title)
    )


@router.put("/space/{sid}/descr", response_class=HTMLResponse)
@tpl.htmx("Spaces.Descr")
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    space = http.Opt(
        await db.for_user(user)
        .spaces()
        .update_descr(sid, descr.strip() if descr else None)
    ).or_404(f"Space '{sid}' not found.")
    res.headers["HX-Trigger"] = f"space-update, space-{sid}-update"
    return {"space": space}


@router.get("/inbox")
@tpl.htmx("Page.Space")
async def inbox(
    req: Request,
    view_complete: bool = False,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user)
    repo_spaces = repo.spaces()
    space = await repo_spaces.default()
    num_tasks = await repo_spaces.num_tasks(space["sid"])
    repo_lists = repo.lists()
    repo_sections = repo.sections()
    lists = await repo_lists.all_by_space(space["sid"])
    list_inbox = await repo_lists.default_for_space(space["sid"])
    sections_inbox = await repo_sections.by_list(list_inbox["uuid"])
    sections_by_list = await repo_lists.num_sections_by_list_for_space(
        space["sid"]
    )
    tasks_by_list = await repo_lists.num_tasks_by_list_for_space(
        space["sid"]
    )
    repo_tasks = repo.tasks()
    tasks_by_section_inbox = await repo_tasks.by_section_for_list(
        list_inbox["uuid"]
    )
    for tt in tasks_by_section_inbox.values():
        for idx, ta in enumerate(tt):
            tt[idx] = dict(ta)
            tt[idx]["is_first"] = idx == 0
            tt[idx]["is_last"] = idx == len(tt) - 1
    is_bookmarked = await repo.bookmarks().is_bookmarked(
        "space", space["uuid"]
    )
    return (
        dict(
            spaces=await repo_spaces.all(),
            space=space,
            lists=lists,
            list_inbox=list_inbox,
            sections_inbox=sections_inbox,
            tasks_by_section_inbox=tasks_by_section_inbox,
            view_complete=view_complete,
            tasks_by_space=num_tasks["tasks_tot"] if num_tasks else 0,
            tasks_by_list=tasks_by_list,
            sections_by_list=sections_by_list,
            is_bookmarked=is_bookmarked,
            user=user,
        ),
        200,
        {"HX-Trigger": "space-read"},
    )
