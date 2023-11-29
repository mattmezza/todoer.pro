import logging
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app import blob
from app.dependencies import get_current_active_user, get_db, get_tpl
from app.ids import new_ids

logger = logging.getLogger(__name__)
router = APIRouter()


@router.delete("/{sid}", response_class=HTMLResponse)
async def delete(
    req: Request,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
):
    row = await db.fetch_one(
        """DELETE FROM attachments
            WHERE sid = :sid AND task IN (
                select uuid from tasks where list in (
                    select uuid from lists where creator = :creator
                )
            )
            RETURNING uuid
        """,
        {"sid": sid, "creator": user["uuid"]},
    )
    uuid = row._mapping['uuid']
    try:
        blob.delete(uuid)
    except:  # TODO - catch specific exception
        logger.error("blob %s couldn't be deleted from remote storage", uuid)
    res = HTMLResponse("")
    res.headers["HX-Trigger"] = "attachment-delete"
    return res


@router.post("", response_class=HTMLResponse)
async def add(
    req: Request,
    task: Annotated[str, Form()],
    content: Annotated[str, Form()],  # TODO - swtch to file
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    mime = ""  # TODO - get mime type
    fname = ""  # TODO - get fname
    size = 0  # TODO - get size in bytes
    content = bytes("")  # TODO - get this from request
    file_key = blob.put(uuid_new, content)
    comm = await db.fetch_one(
        """INSERT INTO attachments (
            uuid,
            sid,
            mime,
            fname,
            size,
            task
        )
        VALUES (
            :uuid,
            :sid,
            :mime,
            :fname,
            :size,
            :task
        )
        RETURNING *""",
        {
            "uuid": uuid_new,
            "sid": sid_new,
            "mime": mime,
            "fname": fname,
            "size": size,
            "task": task,
        },
    )
    res = tpl.TemplateResponse(
        "attachments/view.htm",
        {
            "request": req,
            "attachment": dict(comm._mapping)
            | {
                "url": blob.url_signed_for(
                    file_key,
                    blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
                )
            },
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "attachment-create"
    return res
