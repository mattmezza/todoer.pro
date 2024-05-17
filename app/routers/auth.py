import logging
from datetime import timedelta
from typing import Annotated
from uuid import uuid4

import shortuuid
from fastapi import APIRouter, Depends, Form, HTTPException, Request, status
from fastapi.responses import RedirectResponse
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm

from app import tpl
from app.deps import RepoDB, repo_db
from app.security import (
    ACCESS_TOKEN_EXPIRE_MINUTES,
    authenticate_user,
    create_access_token,
    pwd_context,
    set_auth_cookie,
    unset_auth_cookie,
)

logger = logging.getLogger(__name__)
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
router = APIRouter()
URL_AFTER_SIGNIN = "/dashboard"


# @router.post("/token", response_model=Token)
async def login_api(
    form_data: Annotated[OAuth2PasswordRequestForm, Depends()],
    db: RepoDB = Depends(repo_db),
):
    user = await authenticate_user(db, form_data.username, form_data.password)
    if not user:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Incorrect username or password",
            headers={"WWW-Authenticate": "Bearer"},
        )
    access_token_expires = timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    access_token = create_access_token(
        data={"sub": user["uuid"]},
        expires_delta=access_token_expires,
    )
    return {"access_token": access_token, "token_type": "bearer"}


@router.get("/signin")
@tpl.htmx("Page.Signin")
async def login(
    req: Request,
    goto: str | None = None,
    err: str | None = None,
):
    return {"goto": goto, "error": err}


@router.post("/signin")
async def signin(
    email: Annotated[str, Form()],
    password: Annotated[str, Form()],
    goto: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
):
    urepo = db.for_anon().users()
    user = await authenticate_user(urepo, email, password)
    if not user:
        return RedirectResponse("/signin?err=auth-credentials", 303)
    await urepo.update_last_login_by_uuid(user["uuid"])
    return set_auth_cookie(
        RedirectResponse(goto or URL_AFTER_SIGNIN, 303), user
    )


@router.get("/signout")
async def logout():
    return unset_auth_cookie(RedirectResponse("/signin", 303))


@router.get("/signup")
@tpl.htmx("Page.Signup")
async def signup_view(req: Request):
    return {}


@router.post("/signup")
async def signup(
    name: Annotated[str, Form()],
    email: Annotated[str, Form()],
    password: Annotated[str, Form()],
    consent: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
):
    pwd = pwd_context.hash(password)
    confirm_code = shortuuid.encode(uuid4())[:7]
    user = await db.for_anon().users().create(name, email, pwd, confirm_code)
    # TODO send confirmation email
    logger.info("sending confirmation email should happen here")
    return set_auth_cookie(RedirectResponse(URL_AFTER_SIGNIN, 303), user)


@router.get("/{sid}/confirmation/{code}")
async def confirm(
    sid: str,
    code: str,
    db: RepoDB = Depends(repo_db),
):
    await db.for_anon().users().update_confirmed_by_sid(sid, code)
    return RedirectResponse("/signin", 303)


@router.get("/forgot-password")
@tpl.htmx("Page.ForgotPwd")
async def forgot_pwd_view(req: Request):
    return {}
