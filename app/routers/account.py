import base64
import io
from typing import Annotated, Mapping

import qrcode
from fastapi import APIRouter, Depends, Form, Request
from fastapi.responses import HTMLResponse, RedirectResponse

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db
from app.svc import totp as totp_svc

router = APIRouter()


@router.get("/account")
@tpl.htmx("Page.Account")
async def read(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    return {
        "spaces": await db.for_user(user).spaces().all(),
        "user": user,
    }


@router.put("/account/fullname")
async def change_name(
    req: Request,
    name: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    await db.for_user(user).users().update_fname_by_uuid(
        name.strip()
    )
    return HTMLResponse(
        "",
        headers={
            "HX-Redirect": "/account",
            "HX-Trigger": "account-update",
        },
    )


@router.get("/account/2fa/setup")
@tpl.htmx("Page.TwoFactorSetup")
async def two_factor_setup_view(
    req: Request,
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    if user.get("totp_enabled"):
        return RedirectResponse("/account", 303)
    secret = totp_svc.generate_secret()
    uri = totp_svc.get_provisioning_uri(secret, user["email"])
    img = qrcode.make(uri)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    qr_b64 = base64.b64encode(buf.getvalue()).decode()
    return {
        "spaces": await db.for_user(user).spaces().all(),
        "user": user,
        "secret": secret,
        "qr_data": qr_b64,
    }


@router.post("/account/2fa/setup")
async def two_factor_setup_confirm(
    req: Request,
    secret: Annotated[str, Form()],
    code: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    code_clean = code.strip().replace(" ", "")
    if not totp_svc.verify_code(secret, code_clean):
        return RedirectResponse(
            "/account/2fa/setup?err=invalid-code", 303
        )
    backup_codes = totp_svc.generate_backup_codes()
    hashed_backups = totp_svc.hash_backup_codes(backup_codes)
    encrypted_secret = totp_svc.encrypt_secret(secret)
    await (
        db.for_user(user)
        .users()
        .enable_totp(encrypted_secret, hashed_backups)
    )
    codes_str = ",".join(backup_codes)
    return RedirectResponse(
        f"/account/2fa/backup-codes?codes={codes_str}", 303
    )


@router.get("/account/2fa/backup-codes")
@tpl.htmx("Page.TwoFactorBackupCodes")
async def two_factor_backup_codes_view(
    req: Request,
    codes: str = "",
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    backup_codes = codes.split(",") if codes else []
    return {
        "spaces": await db.for_user(user).spaces().all(),
        "user": user,
        "backup_codes": backup_codes,
    }


@router.post("/account/2fa/disable")
async def two_factor_disable(
    req: Request,
    code: Annotated[str, Form()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    if not user.get("totp_enabled"):
        return RedirectResponse("/account", 303)
    urepo = db.for_anon().users()
    full_user = await urepo.by_email(user["email"])
    secret = totp_svc.decrypt_secret(full_user["totp_secret"])
    code_clean = code.strip().replace(" ", "")
    if not totp_svc.verify_code(secret, code_clean):
        return RedirectResponse(
            "/account?err=invalid-2fa-code", 303
        )
    await db.for_user(user).users().disable_totp()
    return RedirectResponse("/account?msg=2fa-disabled", 303)
