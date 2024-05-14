import logging
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Form, UploadFile
from fastapi.responses import HTMLResponse

from app import blob, http, tpl
from app.ids import new_ids
from app.deps import RepoDB, get_current_active_user, repo_db

logger = logging.getLogger(__name__)
router = APIRouter()


@router.delete("/attachment/{sid}", response_class=HTMLResponse)
async def delete(
    res: HTMLResponse,
    sid: str,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    arepo = db.for_user(user).attachments()
    attachment = http.Opt(await arepo.by_sid(sid)).or_404(
        f"Attachment #{sid} not found."
    )
    await db.for_user(user).attachments().del_by_sid(sid)
    uuid = attachment["uuid"]
    try:
        blob.delete(uuid)
    except:  # TODO - catch specific exception
        logger.error("blob %s couldn't be deleted from remote storage", uuid)
    res.headers["HX-Trigger"] = f"attachment-delete, attachment-{sid}-delete"
    return ""


@router.post("/attachments")
@tpl.htmx("Attachment")
async def add(
    task: Annotated[str, Form()],
    file: UploadFile,
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    uuid_new, sid_new = new_ids()
    content_type = file.content_type
    fname = file.filename

    size = len(file.file.read())
    file.file.seek(0)

    blob.put(uuid_new, file.file, content_type)

    attachment = http.Opt(
        await db.for_user(user)
        .attachments()
        .create(uuid_new, sid_new, content_type, fname, size, task)
    ).or_400()
    return (
        {
            "attachment": dict(attachment)
            | {
                "url": blob.url_signed_for(
                    attachment["uuid"],
                    blob.URL_SIGNED_VALIDITY_DURATION_DEFAULT,
                )
            },
            "user": user,
        },
        201,
        {"HX-Trigger": "attachment-create"},
    )
