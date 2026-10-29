import re
from typing import Annotated, Mapping

from fastapi import APIRouter, Depends, Query, Request

from app import tpl
from app.deps import RepoDB, get_current_active_user, repo_db

router = APIRouter()


def parse_scoped_query(q: str):
    space = None
    list_ = None
    section = None
    query = q.strip()
    if not query:
        return space, list_, section, query

    if query.startswith("@"):
        query = query[1:]
        m = re.match(r"([^\s#$%]+)(.*)", query)
        if m:
            space = m.group(1)
            query = m.group(2).strip()
        else:
            space = query
            query = ""

    if query.startswith("#"):
        query = query[1:]
        m = re.match(r"([^\s$%]+)(.*)", query)
        if m:
            list_ = m.group(1)
            query = m.group(2).strip()
        else:
            list_ = query
            query = ""

    if query.startswith("$"):
        query = query[1:]
        m = re.match(r"([^\s%]+)(.*)", query)
        if m:
            section = m.group(1)
            query = m.group(2).strip()
        else:
            section = query
            query = ""

    if query.startswith("%"):
        query = query[1:].strip()

    return space, list_, section, query


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
    space_name, list_name, section_name, query = parse_scoped_query(q)

    if space_name:
        space_row = await urepo.spaces().find_by_title(space_name)
        if not space_row:
            spaces = await urepo.spaces().search(space_name)
            return {"results": [dict(s) for s in spaces], "query": q}
        space_sid = space_row["sid"]

        if list_name:
            list_row = await urepo.lists().find_by_title_in_space(
                list_name, space_sid
            )
            if list_row and query:
                tasks = await urepo.tasks().search(
                    query, list_sid=list_row["sid"]
                )
                return {
                    "results": [dict(t) for t in tasks],
                    "query": q,
                }
            elif list_row:
                return {"results": [dict(list_row)], "query": q}
            else:
                lists = await urepo.lists().search(
                    list_name, space_sid=space_sid
                )
                return {
                    "results": [dict(l) for l in lists],
                    "query": q,
                }

        if section_name:
            sections = await urepo.sections().search(
                section_name, space_sid=space_sid
            )
            return {
                "results": [dict(s) for s in sections],
                "query": q,
            }

        if query:
            tasks = await urepo.tasks().search(
                query, space_sid=space_sid
            )
            lists = await urepo.lists().search(
                query, space_sid=space_sid
            )
            return {
                "results": [dict(l) for l in lists]
                + [dict(t) for t in tasks],
                "query": q,
            }
        else:
            lists = await urepo.lists().search("", space_sid=space_sid)
            return {
                "results": [dict(l) for l in lists],
                "query": q,
            }

    if list_name:
        lists = await urepo.lists().search(list_name)
        return {"results": [dict(l) for l in lists], "query": q}

    if section_name:
        sections = await urepo.sections().search(section_name)
        return {
            "results": [dict(s) for s in sections],
            "query": q,
        }

    tasks = await urepo.tasks().search(query or q)
    lists = await urepo.lists().search(query or q)
    spaces = await urepo.spaces().search(query or q)
    results = (
        [dict(s) for s in spaces]
        + [dict(l) for l in lists]
        + [dict(t) for t in tasks]
    )
    return {"results": results, "query": q}
