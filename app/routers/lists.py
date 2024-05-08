from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, PlainTextResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import (
    Renderer, get_current_active_user, get_db, get_tpl, renderer, repo_db
)
from app import http, templating
from app.ids import new_ids
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
    res.headers["HX-Trigger"] = "list-delete"
    return res


@router.get("/list/{sid}", response_class=HTMLResponse)
@templating.htmx("Page/List")
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
    return {
        "request": req,
        "spaces": await space_r.all(),
        "space": await space_r.by_uuid(li["space"]),
        "lists": await list_r.all(),
        "list": li,
        "sections": await section_r.by_list(li["uuid"]),
        "view_complete": view_complete,
        "user": user,
    }


@router.put("/list/{sid}/color")
@templating.htmx("Lists/Color")
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
    return dict(color=col), 200, {"HX-Trigger": "list-update"}


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
    res.headers["HX-Trigger"] = "list-update"
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
    res.headers["HX-Trigger"] = "list-update"
    return res


@router.post("/lists", response_class=HTMLResponse)
async def add(
    req: Request,
    space: Annotated[str, Form()],
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    color: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    uuid_new_section, sid_new_section = new_ids()
    async with db.transaction():
        li = await db.fetch_one(
            """
            INSERT INTO lists (uuid, sid, title, descr, color, creator, space)
            VALUES (:uuid, :sid, :title, :descr, :color, :creator, :space)
            RETURNING *, 0 as incomplete
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "space": space,
                "title": title.strip(),
                "descr": descr.strip() if descr else None,
                "color": color,
                "creator": user["uuid"],
            },
        )
        _ = await db.fetch_one(
            """INSERT INTO sections (
                uuid,
                sid,
                `order`,
                `default`,
                title,
                creator,
                list
            )
            VALUES (:uuid, :sid, 0, 1, :title, :creator, :list)
            RETURNING *""",
            {
                "uuid": uuid_new_section,
                "sid": sid_new_section,
                "title": "Main",
                "creator": user["uuid"],
                "list": uuid_new,
            },
        )
        res = tpl.TemplateResponse(
            "nav/add.htm",
            {"request": req, "list": li._mapping, "user": user},
        )
        res.headers["HX-Trigger"] = "list-create"
        return res
