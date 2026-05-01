from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Query, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/search")
@tpl.htmx("Search.Results")
async def search(
    req: Request,
    q: Annotated[str, Query()] = "",
    user: Mapping = Depends(get_current_active_user),
    db: RepoDB = Depends(repo_db),
):
    if not q or len(q) < 2:
        return {"results": [], "query": q}
    urepo = db.for_user(user)
    tasks = await urepo.tasks().search(q)
    lists = await urepo.lists().search(q)
    spaces = await urepo.spaces().search(q)
    results = (
        [dict(s) for s in spaces]
        + [dict(l) for l in lists]
        + [dict(t) for t in tasks]
    )
    return {"results": results, "query": q}
