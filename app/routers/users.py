import logging
from typing import Annotated, Mapping
from uuid import UUID, uuid4

from databases import Database
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import RedirectResponse, PlainTextResponse
from fastapi.templating import Jinja2Templates
from pydantic import EmailStr, SecretStr

from app.security import (
    pwd_context,
    set_auth_cookie,
)
from app.dependencies import get_current_active_user, get_db, get_tpl


logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/me")
async def me(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    lists = await db.fetch_all("select title, uuid from lists limit 50")
    return tpl.TemplateResponse(
        "pages/user.htm",
        {"request": req, "lists": [li._mapping for li in lists], "user": user},
    )


@router.put("/me/name", response_class=PlainTextResponse)
async def change_name(
    req: Request,
    name: Annotated[str, Form()],
    db: Database = Depends(get_db),
    user: Mapping = Depends(get_current_active_user),
):
    row = await db.fetch_one(
        "UPDATE users SET name = :name WHERE uuid = :uuid RETURNING name",
        {"name": name, "uuid": user["uuid"]},
    )
    return row.name


@router.post("/")
async def signup(
    name: Annotated[str, Form()],
    email: Annotated[EmailStr, Form()],
    password: Annotated[SecretStr, Form()],
    db: Annotated[Database, Depends(get_db)],
):
    pwd = pwd_context.hash(password.get_secret_value())
    user = await db.fetch_one(
        """
        INSERT INTO users
            (uuid, name, email, password, confirm_code)
        VALUES (:uuid, :name, :email, :password, :confirm_code)
        RETURNING uuid, email
        """,
        {
            "uuid": str(uuid4()),
            "name": name,
            "email": email,
            "password": pwd,
            "confirm_code": str(uuid4()),
        },
    )
    # TODO send confirmation email
    logger.info("sending confirmation email should happen here")
    res = RedirectResponse("/", 303)
    set_auth_cookie(res, user)
    return res


@router.get("/{uuid}/confirmation/{code}")
async def confirm(
    uuid: UUID,
    code: str,
    db: Annotated[Database, Depends(get_db)],
):
    _ = await db.execute(
        """
        UPDATE
            users
        SET
            confirmed_at = CURRENT_TIMESTAMP
        WHERE
            uuid = :uuid AND confirm_code = :code
        RETURNING uuid
        """,
        {"uuid": str(uuid), "code": code},
    )
    return RedirectResponse("/login", 303)
