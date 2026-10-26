#!/usr/bin/env python3
"""Exercise the Rust CLI against an isolated development server. Creates one task."""

import json
import os
import subprocess
import uuid


def cli(*args):
    result = subprocess.run(
        ["target/debug/todoer", "--json", *args],
        capture_output=True,
        text=True,
        timeout=30,
    )
    if result.returncode:
        # Do not print request metadata or credentials from dependency errors.
        raise RuntimeError(
            f"CLI verification failed (exit {result.returncode})"
        )
    return json.loads(result.stdout)


def main():
    if not os.environ.get("TODOER_TOKEN"):
        raise RuntimeError("TODOER_TOKEN is required")
    capabilities = cli("capabilities")
    assert capabilities["production_auth"] is False
    key = f"cli-check-{uuid.uuid4()}"
    first = cli(
        "capture",
        "--list",
        "inbox",
        "--key",
        key,
        "Synthetic CLI verification",
    )
    assert (
        cli(
            "capture",
            "--list",
            "inbox",
            "--key",
            key,
            "Synthetic CLI verification",
        )
        == first
    )
    task = first[0]
    updated = cli(
        "edit",
        task["id"],
        "--revision",
        str(task["revision"]),
        "--title",
        "Synthetic CLI updated",
    )
    cursor = 0
    event = None
    for _ in range(10):
        page = cli("changes", "--after", str(cursor))
        event = next(
            (
                e
                for e in page
                if e["kind"] == "update" and e["task"]["id"] == task["id"]
            ),
            None,
        )
        if event or not page:
            break
        cursor = page[-1]["seq"]
    assert (
        event is not None
    ), "Run the verification against an isolated bounded fixture"
    undone = cli(
        "undo", str(event["seq"]), "--revision", str(updated["revision"])
    )
    assert undone["title"] == task["title"]
    trashed = cli("trash", task["id"], "--revision", str(undone["revision"]))
    assert any(t["id"] == task["id"] for t in cli("trash-list"))
    restored = cli(
        "restore", task["id"], "--revision", str(trashed["revision"])
    )
    assert cli("subtree", task["id"])[0]["id"] == restored["id"]
    print(
        "CLI HTTP verification passed: capabilities, capture/retry, edit, undo, trash, restore, subtree"
    )


if __name__ == "__main__":
    main()
