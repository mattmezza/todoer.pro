from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Query

from app import templating
from app.dependencies import RepoDB, get_current_active_user, repo_db

router = APIRouter()


@router.get("/lists")
@templating.htmx("Nav/Lists")
async def nav_lists(
    space: Annotated[str, Query()],
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    repo = db.for_user(user)
    space = await repo.spaces().by_uuid(space)
    lists = await repo.lists().all_by_space(space["sid"])
    return {"space": space, "lists": lists}


@router.get("/spaces")
@templating.htmx("Nav/Spaces")
async def nav_spaces(
    db: RepoDB = Depends(repo_db),
    user: Mapping = Depends(get_current_active_user),
):
    return {"spaces": await db.for_user(user).spaces().all()},
