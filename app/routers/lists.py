from typing import Annotated
from uuid import UUID, uuid4

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl

router = APIRouter()


@router.get("/new")
async def new(
    req: Request,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    return tpl.TemplateResponse(
        "lists/new.htm",
        {"request": req},
    )


@router.get("/{uuid}", response_class=HTMLResponse)
async def todo_list_view(
    req: Request,
    uuid: str,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo_list = await db.fetch_one(
        "SELECT * FROM todo_list WHERE uuid = :uuid",
        {"uuid": uuid},
    )
    todos = await db.fetch_all(
        """
        SELECT * FROM todo WHERE todo_list = :uuid
        ORDER BY created_at DESC
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
        },
    )


@router.post("/{uuid}/todo", response_class=HTMLResponse)
async def todo_list_add_todo(
    req: Request,
    uuid: UUID,
    content: Annotated[str, Form()],
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        "INSERT INTO todo (uuid, done, content, todo_list) VALUES (:uuid, 0, :content, :todo_list) RETURNING *",
        {"uuid": str(uuid4()), "content": content, "todo_list": str(uuid)},
    )
    return tpl.TemplateResponse(
        "todo/view.htm",
        {"request": req, **todo._mapping},
    )
