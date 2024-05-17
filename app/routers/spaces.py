from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/spaces")
@tpl.htmx("Page/Spaces")
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
@tpl.htmx("Page/Space")
async def read(
    req: Request,
    sid: str,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user)
    repo_spaces = repo.spaces()
    space = await repo_spaces.by_sid_with_tot_lists(sid)
    num_tasks = await repo_spaces.num_tasks(sid)
    repo_lists = repo.lists()
    lists = await repo_lists.all_by_space(sid)
    sections_by_list = await repo_lists.num_sections_by_list(sid)
    tasks_by_list = await repo_lists.num_tasks_by_list(sid)
    return (
        dict(
            spaces=await repo_spaces.all(),
            space=space,
            lists=lists,
            tasks_by_space=num_tasks["tasks_tot"] if num_tasks else 0,
            tasks_by_list=tasks_by_list,
            sections_by_list=sections_by_list,
            user=user,
        ),
        200,
        {"HX-Trigger": "space-read"},
    )


@router.post("/spaces", response_class=HTMLResponse)
@tpl.htmx("Spaces/ListItem")
async def add(
    req: Request,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    sp = await db.for_user(user).spaces().insert(title, descr)
    return (
        dict(
            space=dict(sp) | {"lists": 0},
            tasks_by_space={sp.uuid: 0},
            user=user,
        ),
        200,
        {"HX-Trigger": "space-create", "HX-Redirect": "/spaces"},
    )


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
    return await db.for_user(user).spaces().update_title(sid, title)


@router.put("/space/{sid}/descr", response_class=HTMLResponse)
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    res.headers["HX-Trigger"] = "space-update"
    return await db.for_user(user).spaces().update_descr(sid, descr)
