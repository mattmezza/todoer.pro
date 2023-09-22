from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl

router = APIRouter()


@router.get("/lists", response_class=HTMLResponse)
async def nav_lists(
    req: Request,
    db: Database = Depends(get_db),
    tpl: Jinja2Templates = Depends(get_tpl),
):
    lists = await db.fetch_all("select uuid, title, color from lists limit 50")
    return tpl.TemplateResponse(
        "nav/lists.htm",
        {"request": req, "lists": [li._mapping for li in lists]},
    )
