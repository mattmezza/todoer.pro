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
    res = HTMLResponse("", status_code=204)
    res.headers["HX-Trigger"] = "list-delete"
    return res


@router.get("/{sid}", response_class=HTMLResponse)
async def view(
    req: Request,
    sid: str,
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


@router.post("/", response_class=HTMLResponse)
async def add(
    req: Request,
    title: Annotated[str, Form()],
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
            INSERT INTO lists (uuid, sid, title, color, creator)
            VALUES (:uuid, :sid, :title, :color, :creator)
            RETURNING *, 0 as incomplete
            """,
            {
                "uuid": uuid_new,
                "sid": sid_new,
                "title": title,
                "color": color,
                "creator": user["uuid"],
            },
        )
        _ = await db.fetch_one(
            """INSERT INTO sections (
                uuid,
                sid,
                `order`,
                title,
                descr,
                creator,
                list
            )
            VALUES (:uuid, :sid, 0, :title, :descr, :creator, :list)
            RETURNING *""",
            {
                "uuid": uuid_new_section,
                "sid": sid_new_section,
                "title": "Main section",
                "descr": "This contains your tasks.",
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
