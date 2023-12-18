import logging
from typing import Annotated, Mapping

from databases import Database
from fastapi import APIRouter, Depends, Form, Request, UploadFile
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
                select uuid from tasks where section in (
                    select uuid from sections where list in (
                        select uuid from lists where creator = :creator
                    )
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
    file: UploadFile,
    user: Mapping = Depends(get_current_active_user),
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    uuid_new, sid_new = new_ids()
    content_type = file.content_type
    fname = file.filename

    size = len(file.file.read())
    file.file.seek(0)

    blob.put(uuid_new, file.file, content_type)
    attachment = await db.fetch_one(
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
            "mime": content_type,
            "fname": fname,
            "size": size,
            "task": task,
        },
    )
    res = tpl.TemplateResponse(
        "attachments/view.htm",
        {
            "request": req,
            "attachment": dict(attachment._mapping)
            | {
                "url": blob.url_signed_for(
                    attachment.uuid,
                    blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
                )
            },
            "user": user,
        },
    )
    res.headers["HX-Trigger"] = "attachment-create"
    return res
