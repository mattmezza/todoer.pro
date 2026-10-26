#!/usr/bin/env python3
"""Real S3 replication/isolated restore; synthetic data, loopback fixture only.
Run: python scripts/rebuild-litestream-check.py --litestream /path/to/litestream
Requires a local Moto S3 server (default http://127.0.0.1:8937).
"""
import argparse
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
import uuid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--litestream", required=True)
parser.add_argument("--endpoint", default="http://127.0.0.1:8937")
args = parser.parse_args()
endpoint = urllib.parse.urlsplit(args.endpoint)
assert endpoint.scheme == "http" and endpoint.hostname in ("127.0.0.1", "localhost"), "isolated loopback only"
executable = str(Path(args.litestream).resolve())
version = subprocess.check_output([executable, "version"], text=True).strip()
assert version == "0.5.17", f"expected pinned 0.5.17, got {version}"
bucket = "todoer-restore-" + uuid.uuid4().hex
urllib.request.urlopen(urllib.request.Request(args.endpoint + "/" + bucket, method="PUT")).close()
Path(".local/rebuild").mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="litestream-", dir=".local/rebuild") as directory:
    root = Path(directory).resolve()
    source = root / "source.db"
    connection = sqlite3.connect(source)
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("CREATE TABLE synthetic_tasks(id TEXT PRIMARY KEY,title TEXT)")
    connection.execute("INSERT INTO synthetic_tasks VALUES('a','Synthetic original')")
    connection.commit()
    env = os.environ.copy()
    env.update(TODOER_RUST_DB=str(source), TODOER_REPLICA_ENDPOINT=args.endpoint,
               TODOER_REPLICA_REGION="us-east-1", TODOER_REPLICA_BUCKET=bucket,
               TODOER_REPLICA_PREFIX="verification", TODOER_REPLICA_ACCESS_KEY_ID="development",
               TODOER_REPLICA_SECRET_ACCESS_KEY="development")
    config = str(Path("deploy/rebuild/litestream.yml").resolve())
    with (root / "replication.log").open("w") as log:
        process = subprocess.Popen([executable, "replicate", "-config", config], env=env, stdout=log, stderr=log)
        try:
            time.sleep(1)
            connection.execute("UPDATE synthetic_tasks SET title='Synthetic later transaction' WHERE id='a'")
            connection.execute("INSERT INTO synthetic_tasks VALUES('b','Synthetic second task')")
            connection.commit()
            deadline = time.monotonic() + 40
            attempt = 0
            while time.monotonic() < deadline:
                assert process.poll() is None, "replication exited; inspect isolated log"
                restored = root / f"restored-{attempt}.db"
                attempt += 1
                result = subprocess.run([executable, "restore", "-config", config, "-o", str(restored),
                                         "-integrity-check", "full", "-json", str(source)], env=env,
                                        capture_output=True, text=True, timeout=10)
                if result.returncode == 0:
                    with sqlite3.connect(restored) as recovered:
                        rows = recovered.execute("SELECT id,title FROM synthetic_tasks ORDER BY id").fetchall()
                        assert recovered.execute("PRAGMA integrity_check").fetchone() == ("ok",)
                        assert recovered.execute("PRAGMA foreign_key_check").fetchall() == []
                    if rows == [("a", "Synthetic later transaction"), ("b", "Synthetic second task")]:
                        summary = json.loads(result.stdout)
                        print(json.dumps({"check": "Litestream S3 replication and isolated restore", "version": version,
                                          "rows": len(rows), "integrity": summary.get("integrity_check"), "passed": True}))
                        break
                time.sleep(0.5)
            else:
                raise AssertionError("replicated latest committed transactions not restored within 40 seconds")
        finally:
            process.send_signal(signal.SIGTERM)
            process.wait(timeout=35)
            connection.close()
# Fixture bucket intentionally retained in the isolated test service for inspection;
# no deletion API or credentials for any real storage are used by this check.
