#!/usr/bin/env python3
"""Offline release policy and split-secret validation. Never prints secret values."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import unittest

REPOSITORY = "mattmezza/todoer.pro"
IMAGE = "ghcr.io/mattmezza/todoer.pro"
VERSION = re.compile(
    r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z.-]+))?\Z"
)


def version(tag):
    match = VERSION.fullmatch(tag)
    if not match or len(tag) > 128:
        raise ValueError("expected vMAJOR.MINOR.PATCH[-prerelease]")
    pre = match[4]
    if pre and any(
        not part or (part.isdigit() and len(part) > 1 and part[0] == "0")
        for part in pre.split(".")
    ):
        raise ValueError("invalid prerelease components")
    return tuple(map(int, match.group(1, 2, 3))), pre


def policy(repository, tag, known_tags):
    if repository != REPOSITORY:
        raise ValueError(
            "release publishing is restricted to the upstream repository"
        )
    core, pre = version(tag)
    stable = []
    for known in known_tags:
        try:
            value, suffix = version(known.strip())
        except ValueError:
            continue
        if suffix is None:
            stable.append(value)
    latest = pre is None and core >= max(stable, default=core)
    return {
        "image": IMAGE,
        "version": tag.removeprefix("v"),
        "latest": latest,
    }


def env_file(path):
    values = {}
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, sep, value = line.partition("=")
        if (
            not sep
            or not re.fullmatch(r"[A-Z][A-Z0-9_]*", key)
            or key in values
        ):
            raise ValueError(
                f"invalid or duplicate environment key on line {number}"
            )
        values[key] = value
    return values


def check_config(app_path, replica_path, origin, image):
    if Path(app_path).resolve() == Path(replica_path).resolve():
        raise ValueError(
            "app and replication credentials require separate files"
        )
    if not re.fullmatch(
        r"ghcr\.io/mattmezza/todoer\.pro@sha256:[a-f0-9]{64}", image
    ):
        raise ValueError(
            "deployment requires the application package and an immutable image digest"
        )
    app, replica = env_file(app_path), env_file(replica_path)
    from urllib.parse import urlsplit

    url = urlsplit(origin)
    if (
        url.scheme != "https"
        or not url.hostname
        or url.username
        or url.password
        or url.query
        or url.fragment
        or url.path not in ("", "/")
    ):
        raise ValueError("public URL must be a canonical HTTPS origin")
    required = {
        "TODOER_GOOGLE_CLIENT_ID",
        "TODOER_GOOGLE_CLIENT_SECRET",
        "TODOER_SERVER_KEY",
    }
    if any(not app.get(key) for key in required):
        raise ValueError(
            "app file requires Google client ID/secret and server key"
        )
    if len(app["TODOER_SERVER_KEY"].encode()) < 32:
        raise ValueError("server key must contain at least 32 bytes")
    if any(not key.startswith("TODOER_") for key in app):
        raise ValueError("app configuration accepts only TODOER_* keys")
    if any(
        key.startswith("TODOER_REPLICA_") or key.startswith("LITESTREAM_")
        for key in app
    ):
        raise ValueError(
            "replication credentials must not enter the app environment"
        )
    if app.get("TODOER_DEV_AUTH", "0") != "0" or "TODOER_DEV_TOKEN" in app:
        raise ValueError("development authentication must not be configured")
    replica_keys = {
        "TODOER_REPLICA_" + name
        for name in (
            "ENDPOINT",
            "REGION",
            "BUCKET",
            "PREFIX",
            "ACCESS_KEY_ID",
            "SECRET_ACCESS_KEY",
        )
    }
    if set(replica) != replica_keys or any(
        not value for value in replica.values()
    ):
        raise ValueError(
            "replication file must contain only the six nonempty TODOER_REPLICA_* keys"
        )
    endpoint = urlsplit(replica["TODOER_REPLICA_ENDPOINT"])
    if (
        endpoint.scheme != "https"
        or not endpoint.hostname
        or endpoint.username
        or endpoint.password
        or endpoint.query
        or endpoint.fragment
    ):
        raise ValueError(
            "production replica endpoint requires HTTPS without credentials or query"
        )
    attachments = [
        "TODOER_ATTACHMENTS_" + name
        for name in (
            "ENDPOINT",
            "BUCKET",
            "REGION",
            "PREFIX",
            "ACCESS_KEY_ID",
            "SECRET_ACCESS_KEY",
        )
    ]
    if any(key in app for key in attachments) and any(
        not app.get(key) for key in attachments
    ):
        raise ValueError("attachment configuration must be complete or absent")
    if (
        app.get("TODOER_ATTACHMENTS_ACCESS_KEY_ID")
        == replica["TODOER_REPLICA_ACCESS_KEY_ID"]
        or app.get("TODOER_ATTACHMENTS_SECRET_ACCESS_KEY")
        == replica["TODOER_REPLICA_SECRET_ACCESS_KEY"]
    ):
        raise ValueError(
            "attachment and replication credentials must be distinct"
        )
    if (
        app.get("TODOER_ATTACHMENTS_BUCKET")
        == replica["TODOER_REPLICA_BUCKET"]
        and app.get("TODOER_ATTACHMENTS_PREFIX")
        == replica["TODOER_REPLICA_PREFIX"]
    ):
        raise ValueError("attachment and replica namespaces must be distinct")
    for path in (Path(app_path), Path(replica_path)):
        if os.name == "posix" and path.stat().st_mode & 0o077:
            raise ValueError(
                "secret files must deny group and other permissions (chmod 600)"
            )
    return {
        "valid": True,
        "services": ["app", "litestream"],
        "origin": origin,
        "separate_credentials": True,
    }


class PolicyTests(unittest.TestCase):
    def test_latest_only_newest_stable(self):
        tags = ["v1.1.0", "v2.0.0-rc.1", "cli-v99.0.0", "invalid"]
        self.assertTrue(policy(REPOSITORY, "v1.2.0", tags)["latest"])
        self.assertFalse(policy(REPOSITORY, "v1.0.0", tags)["latest"])
        self.assertFalse(
            policy(REPOSITORY, "v3.0.0-rc.1", tags)["latest"]
        )
        self.assertFalse(
            policy(REPOSITORY, "v2.0.0", ["v10.0.0"])["latest"]
        )
        self.assertTrue(
            policy(REPOSITORY, "v2.0.0", ["v2.0.0"])["latest"]
        )

    def test_invalid_and_fork_rejected(self):
        for tag in [
            "cli-v1.2.3",
            "v01.2.3",
            "v1.2",
            "v1.2.3-",
            "v1.2.3-rc..1",
            "v1.2.3-01",
            "v1.2.3+build",
            "v1.2.3\n",
        ]:
            with self.assertRaises(ValueError):
                version(tag)
        with self.assertRaises(ValueError):
            policy("fork/todoer.pro", "v1.2.3", [])

    def test_release_source_must_match_tag_and_trusted_branch(self):
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(
                    ["git", *args], cwd=root, check=True, capture_output=True
                )

            git("init", "--quiet", "-b", "main")
            git("config", "user.email", "synthetic@example.invalid")
            git("config", "user.name", "Synthetic release fixture")
            git("config", "commit.gpgsign", "false")
            git("config", "tag.gpgsign", "false")
            git(
                "commit", "--quiet", "--allow-empty", "-m", "Synthetic release"
            )
            git("tag", "v1.0.0")
            tags = root / "tags.txt"
            tags.write_text("v1.0.0\n")
            command = [
                sys.executable,
                str(Path(__file__).resolve()),
                "policy",
                "--repository",
                REPOSITORY,
                "--tag",
                "v1.0.0",
                "--tags-file",
                str(tags),
                "--release-branch",
                "main",
            ]
            valid = subprocess.run(command, cwd=root, capture_output=True)
            self.assertEqual(valid.returncode, 0)
            self.assertEqual(len(json.loads(valid.stdout)["revision"]), 40)
            git(
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "Different checkout",
            )
            self.assertNotEqual(
                subprocess.run(
                    command, cwd=root, capture_output=True
                ).returncode,
                0,
            )
            git("branch", "trusted", "v1.0.0")
            git("tag", "v2.0.0")
            outside = [
                (
                    "v2.0.0"
                    if value == "v1.0.0"
                    else "trusted" if value == "main" else value
                )
                for value in command
            ]
            self.assertNotEqual(
                subprocess.run(
                    outside, cwd=root, capture_output=True
                ).returncode,
                0,
            )

    def test_secret_split_validation(self):
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            app, replica = Path(directory) / "app", Path(directory) / "replica"
            app.write_text(
                "TODOER_GOOGLE_CLIENT_ID=synthetic\nTODOER_GOOGLE_CLIENT_SECRET=synthetic\nTODOER_SERVER_KEY="
                + "x" * 32
                + "\n"
            )
            replica.write_text(
                "\n".join(
                    "TODOER_REPLICA_" + key + "=synthetic-" + key
                    for key in (
                        "ENDPOINT",
                        "REGION",
                        "BUCKET",
                        "PREFIX",
                        "ACCESS_KEY_ID",
                        "SECRET_ACCESS_KEY",
                    )
                )
            )
            replica.write_text(
                replica.read_text().replace(
                    "synthetic-ENDPOINT", "https://replica.invalid"
                )
            )
            app.chmod(0o600)
            replica.chmod(0o600)
            self.assertTrue(
                check_config(
                    app,
                    replica,
                    "https://todoer.invalid",
                    IMAGE + "@sha256:" + "a" * 64,
                )["valid"]
            )
            app.write_text(
                app.read_text()
                + "TODOER_REPLICA_SECRET_ACCESS_KEY=synthetic\n"
            )
            with self.assertRaises(ValueError):
                check_config(
                    app,
                    replica,
                    "https://todoer.invalid",
                    IMAGE + "@sha256:" + "a" * 64,
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("self-test")
    release = sub.add_parser("policy")
    release.add_argument("--repository", required=True)
    release.add_argument("--tag", required=True)
    release.add_argument("--tags-file", type=Path, required=True)
    release.add_argument(
        "--release-branch",
        help="also require the tag commit to be an ancestor of this fetched git ref",
    )
    release.add_argument("--github-output", type=Path)
    config = sub.add_parser("check-config")
    config.add_argument("--app-env", type=Path, required=True)
    config.add_argument("--replica-env", type=Path, required=True)
    config.add_argument("--origin", required=True)
    config.add_argument("--image", required=True)
    args = parser.parse_args()
    if args.command == "self-test":
        result = unittest.TextTestRunner().run(
            unittest.defaultTestLoader.loadTestsFromTestCase(PolicyTests)
        )
        return 0 if result.wasSuccessful() else 1
    if args.command == "check-config":
        result = check_config(
            args.app_env, args.replica_env, args.origin, args.image
        )
    else:
        result = policy(
            args.repository, args.tag, args.tags_file.read_text().splitlines()
        )
        if args.release_branch:
            commit = subprocess.check_output(
                [
                    "git",
                    "rev-parse",
                    "--verify",
                    f"refs/tags/{args.tag}^{{commit}}",
                ],
                text=True,
            ).strip()
            checked_out = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip()
            if checked_out != commit:
                raise ValueError(
                    "tag moved or checkout does not match the release commit"
                )
            result["revision"] = commit
            subprocess.run(
                [
                    "git",
                    "merge-base",
                    "--is-ancestor",
                    commit,
                    args.release_branch,
                ],
                check=True,
            )
        if args.github_output:
            with args.github_output.open("a") as output:
                for key, value in result.items():
                    output.write(
                        f"{key}={str(value).lower() if isinstance(value, bool) else value}\n"
                    )
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.CalledProcessError):
        print(
            "Release preparation failed validation; no release or deployment performed.",
            file=sys.stderr,
        )
        sys.exit(1)
