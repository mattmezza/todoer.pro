from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import (
    Renderer, get_current_active_user, get_db, get_tpl, renderer, repo_db
)
from app.ids import new_ids
from app.http import Opt
from app.repo import RepoDB

router = APIRouter()


@router.delete("/list/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
):
    await db.execute(
        "DELETE FROM lists WHERE sid = :sid AND creator = :creator",
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "list-delete"
    return res


@router.get("/list/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
    view_complete: bool = False,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
    tpl: Jinja2Templates = Depends(get_tpl),
    render: Renderer = Depends(renderer),
):
    user_repo = db.for_user(user)
    list_r = user_repo.lists()
    li = Opt(await list_r.by_sid(sid)).or_404(f"List #{sid} not found.")

    space_r = user_repo.spaces()
    section_r = user_repo.sections()
    return HTMLResponse(
        render(
            "Page/List",
            request=req,
            spaces=await space_r.all(),
            space=await space_r.by_uuid(li["space"]),
            lists=await list_r.all(),
            list=li,
            sections=await section_r.by_list(li["uuid"]),
            view_complete=view_complete,
            user=user,
        )
    )


@router.put("/lists/{sid}/color", response_class=HTMLResponse)
async def update_color(
    req: Request,
    res: HTMLResponse,
    sid: str,
    color: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    li = await db.fetch_one(
        """
        UPDATE lists
            SET color = :color
            WHERE sid = :sid
            AND creator = :creator
        RETURNING color
        """,
        {"sid": sid, "color": color, "creator": user["uuid"]},
    )
    res.headers["HX-Trigger"] = "list-update"

    return f"""<div
    class="w-6 h-6 rounded-full cursor-pointer"
    style="background-color: {li.color }"
    _="on click hide me then show the next <form /> then click() the next <input />">
</div>"""


@router.put("/lists/{sid}/title", response_class=HTMLResponse)
async def update_title(
    req: Request,
    res: HTMLResponse,
    sid: str,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    li = await db.fetch_one(
        """
        UPDATE lists
            SET title = :title
            WHERE sid = :sid
            AND creator = :creator
        RETURNING title
        """,
        {"sid": sid, "title": title, "creator": user["uuid"]},
    )
    res.headers["HX-Trigger"] = "list-update"
    return li.title


@router.put("/lists/{sid}/descr", response_class=HTMLResponse)
async def update_descr(
    req: Request,
    res: HTMLResponse,
    sid: str,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    li = await db.fetch_one(
        """
        UPDATE lists
            SET descr = :descr
            WHERE sid = :sid
            AND creator = :creator
        RETURNING descr
        """,
        {"sid": sid, "descr": descr, "creator": user["uuid"]},
    )
    res.headers["HX-Trigger"] = "list-update"
    return li.descr


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
