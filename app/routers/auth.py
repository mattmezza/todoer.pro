from typing import Annotated

from fastapi import (
    APIRouter,
    Cookie,
    Depends,
    Form,
    Request,
)
from fastapi.responses import RedirectResponse

from app import svc, tpl
from app.deps import RepoDB, repo_db
from app.security import (
    authenticate_user,
    get_password_hash,
    set_auth_cookie,
    set_pending_2fa_cookie,
    unset_auth_cookie,
    unset_pending_2fa_cookie,
    verify_pending_2fa_token,
)
from app.svc import totp as totp_svc

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
    if user.get("totp_enabled"):
        goto_param = f"&goto={goto}" if goto else ""
        res = RedirectResponse(
            f"/auth/2fa/verify?{goto_param}", 303
        )
        return set_pending_2fa_cookie(res, email)
    await urepo.update_last_login_by_uuid(user["uuid"])
    if goto and goto.startswith("/") and not goto.startswith("//"):
        redirect_url = goto
    else:
        redirect_url = URL_AFTER_SIGNIN
    return set_auth_cookie(RedirectResponse(redirect_url, 303), user)


@router.get("/auth/2fa/verify")
@tpl.htmx("Page.TwoFactorVerify")
async def two_factor_verify_view(
    req: Request,
    goto: str | None = None,
    err: str | None = None,
    pending_2fa: str | None = Cookie(None),
):
    if not pending_2fa or not verify_pending_2fa_token(
        pending_2fa
    ):
        return RedirectResponse(
            "/signin?err=auth-credentials", 303
        )
    return {"goto": goto, "error": err}


@router.post("/auth/2fa/verify")
async def two_factor_verify(
    code: Annotated[str, Form()],
    goto: Annotated[str | None, Form()] = None,
    pending_2fa: str | None = Cookie(None),
    db: RepoDB = Depends(repo_db),
):
    if not pending_2fa:
        return RedirectResponse("/signin?err=auth-credentials", 303)
    email = verify_pending_2fa_token(pending_2fa)
    if not email:
        return RedirectResponse(
            "/signin?err=auth-2fa-expired", 303
        )
    urepo = db.for_anon().users()
    user = await urepo.by_email(email)
    if not user:
        return RedirectResponse("/signin?err=auth-credentials", 303)
    secret = totp_svc.decrypt_secret(user["totp_secret"])
    code_clean = code.strip().replace(" ", "")
    valid = totp_svc.verify_code(secret, code_clean)
    if not valid and user.get("totp_backup_codes"):
        remaining = totp_svc.verify_backup_code(
            code_clean, user["totp_backup_codes"]
        )
        if remaining is not None:
            valid = True
            user_dict = {k: v for k, v in user.items()}
            await (
                db.for_user(user_dict)
                .users()
                .update_backup_codes(remaining)
            )
    if not valid:
        goto_param = f"&goto={goto}" if goto else ""
        return RedirectResponse(
            f"/auth/2fa/verify?err=invalid-code{goto_param}",
            303,
        )
    await urepo.update_last_login_by_uuid(user["uuid"])
    res = RedirectResponse(goto or URL_AFTER_SIGNIN, 303)
    res = set_auth_cookie(res, user)
    res = unset_pending_2fa_cookie(res)
    return res


@router.get("/signout")
async def logout():
    res = unset_auth_cookie(RedirectResponse("/signin", 303))
    return unset_pending_2fa_cookie(res)


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
        return RedirectResponse(
            "/signup?err=password-too-short", 303
        )
    urepo = db.for_anon().users()
    existing = await urepo.by_email(email)
    if existing:
        return RedirectResponse(
            "/signup?err=email-already-exists", 303
        )
    pwd = get_password_hash(password)
    user, *_ = await svc.users.signup(db, email, name, pwd)
    return set_auth_cookie(
        RedirectResponse(URL_AFTER_SIGNIN, 303), user
    )


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
