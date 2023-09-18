from typing import Annotated
from uuid import UUID

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl

router = APIRouter()


@router.get("/new")
async def new():
    return ""


@router.get("/{uuid}", response_class=HTMLResponse)
async def todo_view(
    req: Request,
    uuid: str,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    return ""


@router.get("/{uuid}/detail", response_class=HTMLResponse)
async def todo_detail_view(
    req: Request,
    uuid: UUID,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        "SELECT * FROM todo WHERE uuid = :uuid", {"uuid": str(uuid)}
    )
    todos = await db.fetch_all(
        """
        SELECT * FROM todo WHERE todo_list = :uuid
        ORDER BY created_at DESC
    """,
        {"uuid": todo.todo_list},
    )
    todo_tpl = tpl.get_template("todo/view.htm")
    html = {"todos": [todo_tpl.render(**todo._mapping) for todo in todos]}
    return tpl.TemplateResponse(
        "todo/detail.htm",
        {"request": req, **todo, "html": html},
    )


@router.get("/{uuid}/edit", response_class=HTMLResponse)
async def todo_edit(
    req: Request,
    uuid: UUID,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        "SELECT * FROM todo WHERE uuid = :uuid", {"uuid": str(uuid)}
    )
    return tpl.TemplateResponse(
        "todo/edit.htm",
        {"request": req, **todo},
    )


@router.put("/{uuid}", response_class=HTMLResponse)
async def todo_put(
    req: Request,
    uuid: UUID,
    content: Annotated[str, Form()],
    done: Annotated[str | None, Form()] = None,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    todo = await db.fetch_one(
        "UPDATE todo SET done = :done, content = :content WHERE uuid = :uuid RETURNING *",
        {"uuid": str(uuid), "done": 1 if done else 0, "content": content},
    )
    return tpl.TemplateResponse(
        "todo/view.htm",
        {"request": req, **todo},
    )


@router.delete("/{uuid}", response_class=HTMLResponse)
async def todo_delete(
    req: Request,
    uuid: UUID,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    await db.execute(
        "DELETE FROM todo WHERE uuid = :uuid", {"uuid": str(uuid)}
    )
    return ""
