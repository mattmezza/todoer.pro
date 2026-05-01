from typing import Annotated

from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import RedirectResponse

from app import svc, tpl
from app.deps import RepoDB, repo_db
from app.security import (
    authenticate_user,
    pwd_context,
    set_auth_cookie,
    unset_auth_cookie,
)

router = APIRouter()
URL_AFTER_SIGNIN = "/inbox"


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
    if goto and goto.startswith("/") and not goto.startswith("//"):
        redirect_url = goto
    else:
        redirect_url = URL_AFTER_SIGNIN
    return set_auth_cookie(RedirectResponse(redirect_url, 303), user)


@router.get("/signout")
async def logout():
    return unset_auth_cookie(RedirectResponse("/signin", 303))


@router.get("/signup")
@tpl.htmx("Page.Signup")
async def signup_view(
    req: Request,
    err: str | None = None,
):
    return {"error": err}


@router.post("/signup")
async def signup(
    name: Annotated[str, Form()],
    email: Annotated[str, Form()],
    password: Annotated[str, Form()],
    consent: Annotated[str | None, Form()] = None,
    db: RepoDB = Depends(repo_db),
):
    if not consent:
        return RedirectResponse("/signup?err=consent-required", 303)
    if len(password) < 8:
        return RedirectResponse("/signup?err=password-too-short", 303)
    urepo = db.for_anon().users()
    existing = await urepo.by_email(email)
    if existing:
        return RedirectResponse("/signup?err=email-already-exists", 303)
    pwd = pwd_context.hash(password)
    user, *_ = await svc.users.signup(db, email, name, pwd)
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


@router.post("/forgot-password")
@tpl.htmx("Page.ForgotPwd")
async def forgot_pwd(
    req: Request,
    email: Annotated[str, Form()],
):
    return {"sent": True}
