from typing import Annotated, Mapping
from uuid import UUID

from databases import Database
from fastapi import APIRouter, Depends, Form, Request, Response
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl, get_current_active_user

router = APIRouter()


@router.get("/{uuid}", response_class=HTMLResponse)
async def view(
    req: Request,
    uuid: UUID,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        "SELECT * FROM todos WHERE uuid = :uuid", {"uuid": str(uuid)}
    )
    li = await db.fetch_one(
        "SELECT * FROM lists WHERE uuid = :uuid",
        {"uuid": todo.list_id},
    )
    return tpl.TemplateResponse(
        "todo/detail.htm",
        {"request": req, **todo._mapping, "list": li._mapping, "user": user},
    )


@router.put("/{uuid}", response_class=HTMLResponse)
async def update(
    req: Request,
    uuid: UUID,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    done: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        """
        UPDATE todos
        SET done = :done, title = :title, descr = :descr
        WHERE uuid = :uuid
        RETURNING *
        """,
        {
            "uuid": str(uuid),
            "done": 1 if done else 0,
            "title": title,
            "descr": descr,
        },
    )
    res = tpl.TemplateResponse(
        "todo/view.htm",
        {"request": req, **todo},
    )
    res.headers["HX-Trigger"] = "task-update"
    return res


@router.put("/{uuid}/descr", response_class=HTMLResponse)
async def update_descr(
    req: Request,
    uuid: UUID,
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    row = await db.fetch_one(
        "UPDATE todos SET descr = :descr WHERE uuid = :uuid RETURNING descr",
        {"uuid": str(uuid), "descr": descr},
    )
    return row.descr


@router.put("/{uuid}/title", response_class=HTMLResponse)
async def update_title(
    req: Request,
    uuid: UUID,
    title: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    row = await db.fetch_one(
        "UPDATE todos SET title = :title WHERE uuid = :uuid RETURNING title",
        {"uuid": str(uuid), "title": title},
    )
    return row.title


@router.delete("/{uuid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: Response,
    uuid: UUID,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    await db.execute(
        "DELETE FROM todos WHERE uuid = :uuid", {"uuid": str(uuid)}
    )
    res.headers["HX-Trigger"] = "task-delete"
    return ""
