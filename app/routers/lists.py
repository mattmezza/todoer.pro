from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_current_active_user, get_db, get_tpl
from app.ids import new_ids

router = APIRouter()


@router.delete("/{sid}", response_class=HTMLResponse)
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


@router.get("/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
    view_complete: bool = False,
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    li = await db.fetch_one(
        "SELECT * FROM lists WHERE sid = :sid AND creator = :creator",
        {"sid": sid, "creator": user["uuid"]},
    )
    ss = await db.fetch_all(
        "SELECT * FROM sections WHERE list = :list",
        {"list": li.uuid},
    )
    return tpl.TemplateResponse(
        "lists/view.htm",
        {
            "request": req,
            "list": li._mapping,
            "sections": [s._mapping for s in ss],
            "view_complete": view_complete,
            "user": user,
        },
    )


@router.put("/{sid}/title", response_class=HTMLResponse)
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


@router.put("/{sid}/descr", response_class=HTMLResponse)
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


@router.post("/", response_class=HTMLResponse)
async def add(
    req: Request,
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
            INSERT INTO lists (uuid, sid, title, descr, color, creator)
            VALUES (:uuid, :sid, :title, :descr, :color, :creator)
            RETURNING *, 0 as incomplete
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
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
