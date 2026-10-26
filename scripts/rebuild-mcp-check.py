#!/usr/bin/env -S uv run --no-project --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp==2.3.0"]
# ///
"""Exercise the development MCP endpoint using the official Python SDK.

Run: uv run --no-project scripts/rebuild-mcp-check.py
Requires TODOER_TOKEN; TODOER_MCP_URL defaults to http://127.0.0.1:8092/mcp.
Creates one synthetic inbox task per run. Never prints credentials.
"""

import asyncio
import hashlib
import json
import os
import sys
import uuid
from urllib.parse import urlsplit

import httpx2
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client


async def tool(session, name, arguments):
    response = await session.call_tool(name, arguments)
    assert not response.is_error, f"{name} returned a tool error"
    text = next(item.text for item in response.content if item.type == "text")
    return json.loads(text)


async def main():
    token = os.environ.get("TODOER_TOKEN", "")
    if not token:
        print("TODOER_TOKEN is required", file=sys.stderr)
        return 3
    url = os.environ.get("TODOER_MCP_URL", "http://127.0.0.1:8092/mcp")
    async with httpx2.AsyncClient(
        headers={"Authorization": f"Bearer {token}"}, timeout=30
    ) as client:
        async with streamable_http_client(url, http_client=client) as streams:
            async with ClientSession(*streams) as session:
                await session.initialize()
                tools = await session.list_tools()
                names = {item.name for item in tools.tools}
                required = {
                    "resolve_destination",
                    "capture",
                    "read_subtree",
                    "update_task",
                    "move_preview",
                    "move_subtree",
                    "search",
                    "changes",
                    "trash_tasks",
                    "trash_task",
                    "restore_task",
                    "undo_update",
                    "preview_changes",
                    "apply_changes",
                    "configure_destination",
                }
                assert (
                    required <= names
                ), f"Missing tools: {sorted(required - names)}"
                destination = await tool(
                    session, "resolve_destination", {"destination": "inbox"}
                )
                assert destination["id"] == "inbox"
                payload = {
                    "list_id": destination["id"],
                    "idempotency_key": f"sdk-check-{uuid.uuid4()}",
                    "tasks": [
                        {
                            "title": "Synthetic MCP SDK verification",
                            "description": "",
                            "parent_id": None,
                        }
                    ],
                }
                created = await tool(session, "capture", payload)
                repeated = await tool(session, "capture", payload)
                assert len(created) == 1 and repeated == created
                subtree = await tool(
                    session, "read_subtree", {"id": created[0]["id"]}
                )
                assert (
                    len(subtree) == 1 and subtree[0]["id"] == created[0]["id"]
                )
                updated = await tool(
                    session,
                    "update_task",
                    {
                        "id": created[0]["id"],
                        "revision": created[0]["revision"],
                        "title": "Synthetic MCP updated",
                    },
                )
                cursor, event = 0, None
                for _ in range(10):
                    page = await tool(session, "changes", {"after": cursor})
                    event = next(
                        (
                            e
                            for e in page
                            if e["kind"] == "update"
                            and e["task"]["id"] == created[0]["id"]
                        ),
                        None,
                    )
                    if event or not page:
                        break
                    cursor = page[-1]["seq"]
                assert (
                    event is not None
                ), "Use an isolated bounded verification fixture"
                undone = await tool(
                    session,
                    "undo_update",
                    {"seq": event["seq"], "revision": updated["revision"]},
                )
                assert undone["title"] == created[0]["title"]
                trashed = await tool(
                    session,
                    "trash_task",
                    {"id": undone["id"], "revision": undone["revision"]},
                )
                assert any(
                    t["id"] == trashed["id"]
                    for t in await tool(session, "trash_tasks", {})
                )
                restored = await tool(
                    session,
                    "restore_task",
                    {"id": trashed["id"], "revision": trashed["revision"]},
                )
                assert restored["id"] == created[0]["id"]
                await tool(
                    session,
                    "configure_destination",
                    {"key": "mcp-sdk-capture", "list_id": "inbox"},
                )
                saved = await tool(
                    session,
                    "resolve_destination",
                    {"preference_key": "mcp-sdk-capture"},
                )
                assert saved["id"] == "inbox"
                assert any(
                    p["key"] == "mcp-sdk-capture"
                    for p in await tool(session, "configure_destination", {})
                )
                operations = [
                    {
                        "op": "update",
                        "id": restored["id"],
                        "revision": restored["revision"],
                        "title": "Reviewed MCP SDK change",
                    }
                ]
                plan = await tool(
                    session, "preview_changes", {"operations": operations}
                )
                assert (
                    await tool(session, "read_subtree", {"id": restored["id"]})
                )[0]["title"] == restored["title"]
                rejected = await session.call_tool(
                    "apply_changes",
                    {
                        "preview_id": plan["preview_id"],
                        "expected_hash": "mismatched",
                        "idempotency_key": "sdk-invalid",
                    },
                )
                assert rejected.is_error
                apply = {
                    "preview_id": plan["preview_id"],
                    "expected_hash": plan["expected_hash"],
                    "idempotency_key": f"sdk-apply-{uuid.uuid4()}",
                }
                applied = await tool(session, "apply_changes", apply)
                assert applied == await tool(session, "apply_changes", apply)
                final = (
                    await tool(session, "read_subtree", {"id": restored["id"]})
                )[0]
                assert final["title"] == "Reviewed MCP SDK change"
                failed = await session.call_tool(
                    "preview_changes",
                    {
                        "operations": [
                            {
                                "op": "update",
                                "id": final["id"],
                                "revision": final["revision"],
                                "title": "Must roll back",
                            },
                            {
                                "op": "update",
                                "id": final["id"],
                                "revision": 999999,
                                "title": "Stale",
                            },
                        ]
                    },
                )
                assert failed.is_error
                assert (
                    await tool(session, "read_subtree", {"id": final["id"]})
                )[0]["title"] == final["title"]
                current = (
                    await tool(session, "read_subtree", {"id": final["id"]})
                )[0]
                tag = await tool(
                    session,
                    "organize_tasks",
                    {
                        "action": {
                            "op": "tag_create",
                            "list_id": "inbox",
                            "name": f"SDK-{uuid.uuid4().hex[:8]}",
                        }
                    },
                )
                current = await tool(
                    session,
                    "organize_tasks",
                    {
                        "action": {
                            "op": "tags_set",
                            "id": current["id"],
                            "revision": current["revision"],
                            "tags": [{"id": tag["id"]}],
                        }
                    },
                )
                assigned = await tool(
                    session,
                    "organize_tasks",
                    {"action": {"op": "tags_get", "id": current["id"]}},
                )
                assert any(item["id"] == tag["id"] for item in assigned)
                page = await tool(
                    session,
                    "query",
                    {
                        "query": {
                            "text": "SDK change",
                            "list_id": "inbox",
                            "entities": ["task"],
                            "tag": tag["id"],
                            "fields": ["title"],
                        }
                    },
                )
                assert (
                    page["total"] == 1
                    and page["hits"][0]["id"] == current["id"]
                )
                assert "snippet" not in page["hits"][0]
                resolved_ref = await tool(
                    session,
                    "references",
                    {"action": {"op": "resolve", "reference": "#Inbox"}},
                )
                assert (
                    len(resolved_ref["choices"]) == 1
                    and resolved_ref["choices"][0]["id"] == "inbox"
                )
                current = await tool(
                    session,
                    "update_task",
                    {
                        "id": current["id"],
                        "revision": current["revision"],
                        "description": "#Inbox with `#Inbox` literal",
                    },
                )
                unresolved = await tool(
                    session,
                    "references",
                    {
                        "action": {
                            "op": "unresolved",
                            "source_kind": "task",
                            "source_id": current["id"],
                        }
                    },
                )
                assert unresolved == []
                binding = await tool(
                    session,
                    "references",
                    {
                        "action": {
                            "op": "bind",
                            "source_kind": "task",
                            "source_id": current["id"],
                            "revision": current["revision"],
                            "occurrence": 0,
                            "target_kind": "list",
                            "target_id": "inbox",
                        }
                    },
                )
                assert binding["source_revision"] == current["revision"] + 1
                backlinks = await tool(
                    session,
                    "references",
                    {
                        "action": {
                            "op": "backlinks",
                            "target_kind": "list",
                            "target_id": "inbox",
                        }
                    },
                )
                assert any(
                    item["source_id"] == current["id"] for item in backlinks
                )
                current = (
                    await tool(session, "read_subtree", {"id": current["id"]})
                )[0]
                access_info = await tool(
                    session,
                    "manage_access",
                    {"action": {"op": "list_info", "list_id": "inbox"}},
                )
                assert access_info["id"] == "inbox"
                state = await tool(
                    session,
                    "schedule_tasks",
                    {
                        "action": {
                            "op": "set",
                            "id": current["id"],
                            "revision": current["revision"],
                            "schedule": {
                                "planned": {
                                    "kind": "date",
                                    "date": "2026-10-26",
                                },
                                "due": {
                                    "kind": "timed",
                                    "local": "2026-10-27T16:00:00",
                                    "timezone": "Europe/Zurich",
                                },
                                "priority": 2,
                                "reminders": [],
                            },
                        }
                    },
                )
                assert state["schedule"]["priority"] == 2
                planning = await tool(
                    session,
                    "schedule_tasks",
                    {
                        "action": {
                            "op": "planning",
                            "from": "2026-10-26",
                            "to": "2026-10-28",
                        }
                    },
                )
                assert any(
                    item["task"]["id"] == current["id"]
                    for item in planning["tasks"]
                )
                statistics = await tool(
                    session,
                    "schedule_tasks",
                    {"action": {"op": "statistics", "from": "2026-10-26"}},
                )
                assert statistics["scheduled_open"] >= 1
                comment_action = {
                    "op": "comment_create",
                    "task_id": current["id"],
                    "idempotency_key": f"sdk-comment-{uuid.uuid4()}",
                    "body": "Synthetic SDK comment #Inbox",
                }
                comment = await tool(
                    session, "task_activity", {"action": comment_action}
                )
                assert comment == await tool(
                    session, "task_activity", {"action": comment_action}
                )
                comments = await tool(
                    session,
                    "task_activity",
                    {"action": {"op": "comments", "task_id": current["id"]}},
                )
                assert any(
                    item["id"] == comment["id"]
                    and "body_excerpt" in item
                    and "body" not in item
                    for item in comments["comments"]
                )
                edited = await tool(
                    session,
                    "task_activity",
                    {
                        "action": {
                            "op": "comment_edit",
                            "id": comment["id"],
                            "revision": comment["revision"],
                            "body": "Reviewed SDK comment",
                        }
                    },
                )
                assert edited["revision"] == comment["revision"] + 1
                deleted = await tool(
                    session,
                    "task_activity",
                    {
                        "action": {
                            "op": "comment_delete",
                            "id": edited["id"],
                            "revision": edited["revision"],
                        }
                    },
                )
                assert deleted["deleted"]
                if os.environ.get("TODOER_TEST_ATTACHMENTS") == "1":
                    data = b"Synthetic MCP attachment fixture"
                    upload = await tool(
                        session,
                        "task_activity",
                        {
                            "action": {
                                "op": "upload_create",
                                "task_id": current["id"],
                                "input": {
                                    "filename": "sdk-check.txt",
                                    "size": len(data),
                                    "sha256": hashlib.sha256(data).hexdigest(),
                                },
                            }
                        },
                    )
                    endpoint = urlsplit(upload["upload"]["url"])
                    assert endpoint.scheme == "http" and endpoint.hostname in {
                        "127.0.0.1",
                        "localhost",
                        "::1",
                    }
                    async with httpx2.AsyncClient(timeout=30) as binary_client:
                        uploaded = await binary_client.put(
                            upload["upload"]["url"],
                            headers=upload["upload"]["headers"],
                            content=data,
                        )
                        assert uploaded.is_success
                    finalize = {
                        "op": "upload_finalize",
                        "id": upload["intent"]["id"],
                        "revision": upload["intent"]["revision"],
                    }
                    attachment = await tool(
                        session, "task_activity", {"action": finalize}
                    )
                    assert attachment == await tool(
                        session, "task_activity", {"action": finalize}
                    )
                    link = await tool(
                        session,
                        "task_activity",
                        {
                            "action": {
                                "op": "file_link",
                                "id": attachment["id"],
                            }
                        },
                    )
                    assert (
                        link["mcp_oauth_credential_accepted"] is False
                        and "object_key" not in link["attachment"]
                    )
                    assert (
                        urlsplit(link["download_url"]).netloc
                        == urlsplit(url).netloc
                    )
                    files = await tool(
                        session,
                        "task_activity",
                        {"action": {"op": "files", "task_id": current["id"]}},
                    )
                    assert any(
                        item["id"] == attachment["id"]
                        for item in files["attachments"]
                    )
                    await tool(
                        session,
                        "task_activity",
                        {
                            "action": {
                                "op": "file_delete",
                                "id": attachment["id"],
                                "revision": attachment["revision"],
                            }
                        },
                    )
                    print(
                        "MCP SDK attachment check passed: signed local upload, SHA256 verification, finalize retry, checked link metadata, authorized list and soft deletion"
                    )
                print(
                    f"MCP SDK check passed: initialize, {len(names)} tools, resolve, capture/retry, subtree, edit, undo, trash, restore, saved destination, bulk preview/apply/retry/rollback, tags, structured query, reference bind/backlinks, scheduling/planning/statistics, comment retry/edit/delete"
                )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(asyncio.run(main()))
    except Exception:
        # SDK exceptions may contain request metadata; keep credentials out of output.
        print(
            "MCP SDK check failed; verify server availability and development token",
            file=sys.stderr,
        )
        sys.exit(1)
