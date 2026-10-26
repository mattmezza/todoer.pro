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
import json
import os
import sys
import uuid

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
                print(
                    f"MCP SDK check passed: initialize, {len(names)} tools, resolve, capture/retry, subtree, edit, undo, trash, restore"
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
