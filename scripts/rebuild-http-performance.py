#!/usr/bin/env python3
"""Isolated synthetic HTTP diagnostic; requires an already built todoer-server.

Never opens an existing application database. No latency threshold assertions.
"""

import argparse
import concurrent.futures
import hashlib
import http.cookiejar
import json
import math
import os
import platform
from pathlib import Path
import re
import secrets
import signal
import shutil
import socket
import sqlite3
import statistics
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def request(
    base,
    path,
    *,
    token=None,
    opener=None,
    method="GET",
    body=None,
    headers=None,
):
    headers = dict(headers or {})
    if token:
        headers["Authorization"] = "Bearer " + token
    if body is not None and not isinstance(body, bytes):
        body = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(
        base + path, data=body, headers=headers, method=method
    )
    start = time.perf_counter()
    try:
        response = (
            opener.open(req, timeout=60)
            if opener
            else urllib.request.urlopen(req, timeout=60)
        )
    except urllib.error.HTTPError as error:
        response = error
    with response:
        data = response.read()
        return response.status, data, (time.perf_counter() - start) * 1000


def summary(samples, sizes):
    ordered = sorted(samples)
    return {
        "n": len(samples),
        "median_ms": round(statistics.median(samples), 3),
        "p95_ms": round(ordered[math.ceil(len(ordered) * 0.95) - 1], 3),
        "max_ms": round(max(samples), 3),
        "min_bytes": min(sizes),
        "max_bytes": max(sizes),
    }


def start_server(binary, env, base, log):
    process = subprocess.Popen(
        [str(binary)], cwd=ROOT, env=env, stdout=log, stderr=log
    )
    for _ in range(300):
        if process.poll() is not None:
            raise RuntimeError(
                "isolated server exited; inspect retained local log"
            )
        try:
            status, _, _ = request(base, "/health")
            if status == 200:
                return process
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.1)
    process.terminate()
    process.wait(timeout=30)
    raise RuntimeError("isolated server did not become ready")


def stop_server(process):
    if process and process.poll() is None:
        process.send_signal(signal.SIGTERM)
        process.wait(timeout=40)
        check(process.returncode == 0, "server did not shut down cleanly")


def seed(path):
    """Populate migrated schema directly while server is stopped; triggers maintained."""
    with sqlite3.connect(path) as db:
        db.execute("PRAGMA foreign_keys=ON")
        for number in range(100):
            lid = f"perf-list-{number:03}"
            db.execute(
                "INSERT INTO lists(id,title,position) VALUES(?,?,?)",
                (lid, f"Synthetic list {number:03}", number),
            )
            db.execute("INSERT INTO memberships VALUES(?,'dev')", (lid,))
        db.execute(
            "INSERT INTO lists(id,title) VALUES('perf-stress','Synthetic stress')"
        )
        db.execute("INSERT INTO memberships VALUES('perf-stress','dev')")
        description = (
            "Synthetic representative description café checklist. " * 10
        )
        for number in range(11000):
            tid = f"perf-task-{number:05}"
            lid = (
                f"perf-list-{number // 100:03}"
                if number < 10000
                else "perf-stress"
            )
            parent = (
                f"perf-task-{(number-1)//2:05}" if 0 < number < 50 else None
            )
            title = (
                f"Benchmark café task {number:05} "
                + "representative title " * (number % 5)
            )
            db.execute(
                "INSERT INTO tasks(id,reference,list_id,parent_id,title,description,position) VALUES(?,?,?,?,?,?,?)",
                (
                    tid,
                    f"p{number:07}",
                    lid,
                    parent,
                    title,
                    description,
                    number,
                ),
            )
            if number < 10000:
                db.execute(
                    "INSERT INTO comments(id,task_id,author_id,body) VALUES(?,?,'dev',?)",
                    (
                        f"perf-comment-{number:05}",
                        tid,
                        "Synthetic discussion benchmark comment. " * 8,
                    ),
                )
        db.execute(
            "INSERT INTO tasks(id,reference,list_id,title,position) VALUES('perf-private','private-perf','private','Benchmark private sentinel',0)"
        )
        check(
            db.execute("SELECT count(*) FROM tasks").fetchone()[0] == 11001,
            "fixture task count mismatch",
        )
        check(
            db.execute("SELECT count(*) FROM comments").fetchone()[0] == 10000,
            "fixture comment count mismatch",
        )


def human_login(base):
    opener = urllib.request.build_opener(
        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar())
    )
    status, body, _ = request(base, "/signin", opener=opener)
    check(status == 200, "development signin unavailable")
    csrf = re.search(rb'name="csrf" value="([^"]+)"', body).group(1).decode()
    status, _, _ = request(
        base,
        "/signin",
        opener=opener,
        method="POST",
        body=urllib.parse.urlencode({"csrf": csrf}).encode(),
        headers={
            "Origin": base,
            "Content-Type": "application/x-www-form-urlencoded",
        },
    )
    check(status == 200, "human cookie signin failed")
    return opener, csrf


def run(binary, directory, repetitions, browser=False, profile="unspecified"):
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    original_binary = binary
    binary = directory / "todoer-server.snapshot"
    shutil.copy2(original_binary, binary)
    database = directory / "synthetic.db"
    check(not database.exists(), "refusing to reuse an existing database")
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    base = f"http://127.0.0.1:{port}"
    token = secrets.token_urlsafe(32)
    # Inherited production credentials/services cannot participate in this probe.
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("TODOER_")
    }
    env.update(
        TODOER_DEV_AUTH="1",
        TODOER_DEV_TOKEN=token,
        TODOER_RUST_DB=str(database),
        TODOER_RUST_PORT=str(port),
        TODOER_RUST_BIND="127.0.0.1",
    )
    result = {
        "binary": str(original_binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "environment": {
            "architecture": platform.machine(),
            "platform": platform.system(),
            "fixture_python_sqlite": sqlite3.sqlite_version,
            "profile": profile,
        },
        "fixture": {
            "account_lists": 100,
            "tasks_per_typical_list": 100,
            "account_tasks": 10000,
            "account_comments": 10000,
            "stress_list_tasks": 1000,
            "private_sentinel_tasks": 1,
            "nested_subtree_nodes": 50,
            "description_bytes": len(
                (
                    "Synthetic representative description café checklist. "
                    * 10
                ).encode()
            ),
        },
        "measurements": {},
    }
    process = None
    with (directory / "server.log").open("wb") as log:
        try:
            process = start_server(binary, env, base, log)
            stop_server(process)
            seed(database)
            process = start_server(binary, env, base, log)
            opener, csrf = human_login(base)

            def measure(name, path, body=None, human=False):
                samples, sizes = [], []
                for _ in range(repetitions):
                    status, data, elapsed = request(
                        base,
                        path,
                        token=None if human else token,
                        opener=opener if human else None,
                        method="POST" if body is not None else "GET",
                        body=body,
                    )
                    check(status == 200, f"{name} returned HTTP {status}")
                    samples.append(elapsed)
                    sizes.append(len(data))
                result["measurements"][name] = summary(samples, sizes)
                return json.loads(data) if not human else data

            measure(
                "legacy_prefix_100", "/api/v1/search?query=Bench&limit=100"
            )
            query = {
                "text": "Bench",
                "entities": ["task", "comment"],
                "status": "all",
                "limit": 100,
                "sort": "id",
                "fields": ["title"],
            }
            page = measure("structured_prefix_100", "/api/v1/query", query)
            check(
                page["total"] == 21000,
                "authorized multi-entity total mismatch/private sentinel leaked",
            )
            check(
                len(page["hits"]) == 100 and page["next_cursor"],
                "query page not bounded",
            )
            next_query = dict(query, cursor=page["next_cursor"])
            second = measure(
                "structured_prefix_cursor_100", "/api/v1/query", next_query
            )
            check(
                not (
                    {hit["id"] for hit in page["hits"]}
                    & {hit["id"] for hit in second["hits"]}
                ),
                "query cursor repeated hits",
            )
            check(
                all(
                    hit["id"] != "perf-private"
                    for hit in page["hits"] + second["hits"]
                ),
                "private task exposed",
            )
            subtree_path = "/api/v1/tasks/perf-task-00000/subtree/query"
            subtree_query = {"limit": 17, "fields": ["title", "revision"]}
            measure("subtree_first_17", subtree_path, subtree_query)
            cursor, ids, sizes, samples = None, [], [], []
            while True:
                status, data, elapsed = request(
                    base,
                    subtree_path,
                    token=token,
                    method="POST",
                    body=dict(subtree_query, cursor=cursor),
                )
                check(status == 200, "subtree pagination failed")
                page = json.loads(data)
                check(
                    len(data) <= 262144 and len(page["items"]) <= 17,
                    "subtree response limit exceeded",
                )
                ids.extend(item["id"] for item in page["items"])
                samples.append(elapsed)
                sizes.append(len(data))
                if not page["has_more"]:
                    break
                cursor = page["next_cursor"]
            check(
                len(ids) == len(set(ids)) == 50,
                "subtree omitted/repeated nodes",
            )
            result["measurements"]["subtree_all_pages_50_nodes"] = summary(
                samples, sizes
            )
            for lid in ["perf-list-000", "perf-stress"]:
                html = measure("html_" + lid, "/lists/" + lid, human=True)
                check(
                    b"Synthetic" in html and b"task" in html,
                    "expected synthetic HTML missing",
                )

            if browser:
                completed = subprocess.run(
                    [
                        "node",
                        str(
                            ROOT
                            / "scripts/rebuild-http-performance-browser.cjs"
                        ),
                        base,
                        str(directory),
                    ],
                    cwd=ROOT,
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=180,
                )
                check(
                    completed.returncode == 0,
                    "phone probe failed: " + completed.stderr,
                )
                result["browser"] = json.loads(completed.stdout)

            # Same revision, one cookie-auth human and one bearer-auth agent race.
            barrier = __import__("threading").Barrier(2)

            def write(human):
                barrier.wait()
                return request(
                    base,
                    "/api/v1/tasks/perf-task-00099",
                    token=None if human else token,
                    opener=opener if human else None,
                    method="PATCH",
                    body={
                        "revision": 1,
                        "title": (
                            "Synthetic human winner"
                            if human
                            else "Synthetic agent winner"
                        ),
                    },
                    headers=(
                        {"X-CSRF-Token": csrf, "Origin": base} if human else {}
                    ),
                )

            with concurrent.futures.ThreadPoolExecutor(
                max_workers=2
            ) as executor:
                human, agent = list(executor.map(write, [True, False]))
            check(
                sorted([human[0], agent[0]]) == [200, 409],
                "CAS race did not preserve one writer",
            )
            winner = json.loads(human[1] if human[0] == 200 else agent[1])
            check(winner["revision"] == 2, "CAS revision incorrect")
            result["cas"] = {
                "human_status": human[0],
                "agent_status": agent[0],
                "human_ms": round(human[2], 3),
                "agent_ms": round(agent[2], 3),
                "winner_revision": winner["revision"],
            }
            with sqlite3.connect(f"file:{database}?mode=ro", uri=True) as db:
                check(
                    db.execute("PRAGMA journal_mode").fetchone()[0] == "wal",
                    "WAL not enabled",
                )
                check(
                    db.execute("PRAGMA integrity_check").fetchone()[0] == "ok",
                    "integrity check failed",
                )
            result["wal"] = {
                "journal_mode": "wal",
                "wal_present_while_running": Path(
                    str(database) + "-wal"
                ).exists(),
            }
            stop_server(process)
            process = start_server(binary, env, base, log)
            with sqlite3.connect(f"file:{database}?mode=ro", uri=True) as db:
                check(
                    db.execute(
                        "SELECT title,revision FROM tasks WHERE id='perf-task-00099'"
                    ).fetchone()
                    == (winner["title"], 2),
                    "winner not persisted after restart",
                )
                check(
                    db.execute("PRAGMA integrity_check").fetchone()[0] == "ok",
                    "restart integrity failed",
                )
                check(
                    not db.execute("PRAGMA foreign_key_check").fetchall(),
                    "foreign key check failed",
                )
                result["wal"][
                    "database_bytes_after_restart"
                ] = database.stat().st_size
            status, data, _ = request(
                base,
                "/api/v1/query",
                token=token,
                method="POST",
                body={
                    "text": winner["title"],
                    "entities": ["task"],
                    "limit": 100,
                    "status": "all",
                },
            )
            check(
                status == 200
                and any(
                    hit["id"] == winner["id"]
                    for hit in json.loads(data)["hits"]
                ),
                "persisted edit not queryable after restart",
            )
            result["restart"] = {
                "winner_persisted": True,
                "integrity": "ok",
                "foreign_keys": "ok",
                "edited_task_searchable": True,
            }
        finally:
            stop_server(process)
    (directory / "measurements.json").write_text(
        json.dumps(result, indent=2) + "\n"
    )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", type=Path, default=ROOT / "target/debug/todoer-server"
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="new empty private output directory; default temporary and deleted",
    )
    parser.add_argument("--repetitions", type=int, default=7)
    parser.add_argument(
        "--browser",
        action="store_true",
        help="also run installed Playwright Chromium phone probe",
    )
    parser.add_argument(
        "--profile",
        choices=["debug", "release", "unspecified"],
        default="unspecified",
    )
    args = parser.parse_args()
    check(args.binary.is_file(), "build todoer-server before running")
    check(1 <= args.repetitions <= 100, "repetitions must be 1..100")
    if args.output:
        check(
            not args.output.exists(), "output directory must not already exist"
        )
        result = run(
            args.binary.resolve(),
            args.output.resolve(),
            args.repetitions,
            args.browser,
            args.profile,
        )
    else:
        with tempfile.TemporaryDirectory(
            prefix="todoer-http-performance-"
        ) as temp:
            result = run(
                args.binary.resolve(),
                Path(temp),
                args.repetitions,
                args.browser,
                args.profile,
            )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
