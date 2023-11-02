from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Request, Form
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
        """DELETE FROM comments
            WHERE sid = :sid AND task IN (
                select uuid from tasks where list in (
                    select uuid from lists where creator = :creator
                )
            )
        """,
        {"sid": sid, "creator": user["uuid"]},
    )
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "comment-delete"
    return res


@router.post("", response_class=HTMLResponse)
async def add(
    req: Request,
    task: Annotated[str, Form()],
    content: Annotated[str, Form()],
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    comm = await db.fetch_one(
        """INSERT INTO comments (
            uuid,
            sid,
            content,
            author,
            task
        )
        VALUES (
            :uuid,
            :sid,
            :content,
            :author,
            :task
        )
        RETURNING *""",
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "content": content,
            "author": user["uuid"],
            "task": task,
        },
    )
    res = tpl.TemplateResponse(
        "comments/view.htm",
        {
            "request": req,
            "comment": dict(comm._mapping)
            | {"author_email_hash": user["email_hash"]},
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "comment-create"
    return res
