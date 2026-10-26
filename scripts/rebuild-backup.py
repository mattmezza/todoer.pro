#!/usr/bin/env python3
"""Copy a current rebuild SQLite database to a new local recovery file."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import sys


def verify(connection):
    if connection.execute("PRAGMA integrity_check").fetchall() != [("ok",)]:
        raise ValueError("database integrity check failed")
    if connection.execute("PRAGMA foreign_key_check").fetchone() is not None:
        raise ValueError("database foreign-key check failed")
    tables = {
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )
    }
    if "spaces" in tables or any(
        row[1] == "uuid" for row in connection.execute("PRAGMA table_info(users)")
    ):
        raise ValueError("legacy database is not supported")
    if "schema_migrations" not in tables or not {
        "users", "lists", "memberships", "tasks"
    }.issubset(tables):
        raise ValueError("current rebuild database required")
    versions = [row[0] for row in connection.execute(
        "SELECT version FROM schema_migrations ORDER BY version"
    )]
    if not versions or versions != list(range(1, versions[-1] + 1)):
        raise ValueError("invalid rebuild migration history")
    return versions


def copy_database(source, target):
    if target.exists() or target.is_symlink():
        raise ValueError("target must not exist")
    source = source.resolve(strict=True)
    target = target.resolve(strict=False)
    if source == target:
        raise ValueError("source and target must differ")
    if not source.is_file():
        raise ValueError("source must be a database file")
    if not target.parent.is_dir():
        raise ValueError("target parent directory must exist")
    # Opening read-only sees committed WAL content without modifying DB rows.
    # No immutable flag: that would incorrectly ignore an active WAL.
    reader = sqlite3.connect(source.as_uri() + "?mode=ro", uri=True, timeout=5)
    created = False
    try:
        versions = verify(reader)
        # Exclusive creation prevents overwriting an existing file or symlink.
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.close(descriptor)
        created = True
        with sqlite3.connect(target, timeout=5) as writer:
            reader.backup(writer, pages=256, sleep=0.05)
            if verify(writer) != versions:
                raise ValueError("migration history changed during backup")
        return {"ok": True, "verified": True, "migration_versions": versions}
    except Exception:
        if created:
            # Only delete the destination this invocation exclusively created.
            target.unlink(missing_ok=True)
        raise
    finally:
        reader.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("backup", "restore"))
    parser.add_argument("source", type=Path)
    parser.add_argument("target", type=Path)
    args = parser.parse_args()
    try:
        result = copy_database(args.source, args.target)
        result["operation"] = args.operation
        print(json.dumps(result, sort_keys=True))
        return 0
    except (OSError, sqlite3.Error, ValueError):
        # Avoid filenames, SQL values, and other user content in output.
        print(json.dumps({"ok": False, "error": "copy refused or verification failed"}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
