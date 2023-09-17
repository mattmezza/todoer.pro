from databases import Database
from fastapi import APIRouter, Depends, Request
from fastapi.templating import Jinja2Templates

from app.dependencies import get_db, get_tpl

router = APIRouter()


@router.get("/")
async def home(
    req: Request,
    tpl: Jinja2Templates = Depends(get_tpl),
    db: Database = Depends(get_db),
):
    lists = await db.fetch_all("select title, uuid from todo_list limit 50")
    return tpl.TemplateResponse(
        "pages/home.htm",
        {"request": req, "lists": [li._mapping for li in lists]},
    )
