from typing import Annotated, Mapping
from uuid import UUID, uuid4

from databases import Database
from fastapi import APIRouter, Depends, Form, Request, Response
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl, get_current_active_user

router = APIRouter()
# TODO add rbac support everywhere


@router.delete("/{uuid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    res: Response,
    uuid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
):
    await db.execute(
        "DELETE FROM lists WHERE uuid = :uuid",
        {"uuid": uuid},
    )
    res.headers["HX-Trigger"] = "lists-delete"
    return ""


@router.get("/{uuid}", response_class=HTMLResponse)
async def view(
    req: Request,
    uuid: str,
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo_list = await db.fetch_one(
        "SELECT * FROM lists WHERE uuid = :uuid",
        {"uuid": uuid},
    )
    todos = await db.fetch_all(
        """
        SELECT * FROM todos WHERE list_id = :uuid
        ORDER BY created_at ASC
    """,
        {"uuid": uuid},
    )
    todo_tpl = tpl.get_template("todo/view.htm")
    html = {"todos": [todo_tpl.render(**todo._mapping) for todo in todos]}
    return tpl.TemplateResponse(
        "lists/view.htm",
        {
            "request": req,
            **todo_list,
            "todos": {t._mapping for t in todos},
            "html": html,
            "user": user,
        },
    )


@router.post("/{uuid}/todo", response_class=HTMLResponse)
async def add_todo(
    req: Request,
    uuid: UUID,
    title: Annotated[str, Form()],
    descr: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        """
        INSERT INTO todos (uuid, done, title, descr, list_id)
        VALUES (:uuid, 0, :title, :descr, :list_id)
        RETURNING *
        """,
        {
            "uuid": str(uuid4()),
            "title": title,
            "descr": descr,
            "list_id": str(uuid),
        },
    )
    return tpl.TemplateResponse(
        "todo/view.htm",
        {"request": req, **todo._mapping, "user": user},
    )


@router.post("/", response_class=HTMLResponse)
async def add(
    req: Request,
    title: Annotated[str, Form()],
    color: Annotated[str | None, Form()] = None,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    li = await db.fetch_one(
        """
        INSERT INTO lists (uuid, title, color)
        VALUES (:uuid, :title, :color)
        RETURNING *, 0 as todos_undone
        """,
        {"uuid": str(uuid4()), "title": title, "color": color},
    )
    return tpl.TemplateResponse(
        "nav/add.htm",
        {"request": req, **li._mapping, "user": user},
    )
