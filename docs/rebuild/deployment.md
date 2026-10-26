# Isolated Rust deployment and release

This deployment is separate from the existing Python application. It does not
mount, upgrade or replace a legacy database. No production deployment or registry
push has been performed.

On 2026-10-26 the owner enabled Docker 29.9.0 on this x86_64 host. A checksum-verified
Buildx 0.38.1 plugin was placed only in ignored task-local configuration. The
multi-stage image built successfully for **linux/amd64**, including Rust tests and
optimized binaries. An actual isolated Compose acceptance check passed with
production authentication, non-root user/read-only root, REST capture retry and
revision conflicts, CLI requests and Litestream S3 replication/restoration of the
latest committed transactions. It used synthetic users and credentials, its own
randomly named network/volume and a loopback Moto fixture; those Compose resources
were removed after the check. No other CPU architecture or production provider is
claimed tested.

Official actionlint 1.7.12 validates the workflow; Moby patternmatcher 0.6.1
validates the build-context allowlist. Native release and release-policy checks
also pass. New feature increments must rebuild the image and repeat acceptance:

```sh
python scripts/rebuild-container-check.py --image todoer-rebuild:local-verified
```

This check requires an already-built local image, Docker Compose and isolated Moto
S3 on `127.0.0.1:8937`. It creates only random test resources, forwards S3 traffic
through a private Docker bridge address, and removes its containers/network/volume.
The Compose configuration uses separate application and replication environment
files. The test never reads the old application database or uses real Google keys.
The Google public discovery endpoint is contacted at startup; tenant login/device
acceptance remains a separate external check.

## Image and assets

Build from the repository root:

```sh
docker build --platform linux/amd64 -f deploy/rebuild/Dockerfile -t todoer-rebuild:review .
```

The Node stage uses `npm ci --ignore-scripts`, the committed lockfile, Tailwind
compilation and a self-hosted htmx copy. Rust tests and release builds run with
`--locked` in the pinned Rust stage, after the compiled assets are copied in so
that the service-worker build fingerprint matches the shipped assets. Runtime
contains the server, `todoer` CLI and `todoer-migrate`, compiled assets, static
icons, CA certificates, curl for process health and OpenSSL 3 for web-push's
`ece` dependency. Source, Node, Cargo, credentials and database files are absent.
A Dockerfile-specific allowlist admits only build inputs and static icons;
databases, environment files, unrelated application files and `.local` artifacts
are excluded from the build context.

The following Docker Registry v2 index digests were verified on 2026-10-26:

| Stage | Version | Index digest |
| --- | --- | --- |
| Rust | 1.99.0-slim-bookworm | `sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa` |
| Node | 24.13.0-bookworm-slim | `sha256:4660b1ca8b28d6d1906fd644abe34b2ed81d15434d26d845ef0aced307cf4b6f` |
| Runtime | debian:bookworm-slim | `sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8` |
| Replica | litestream/litestream:0.5.17 | `sha256:4b02b9859a6b6b4087d8b8944e15f7e984bd7957cba322bbeee38b0e27b9656a` |

Only `linux/amd64` is configured and observed locally. No ARM build is claimed.
Base images, language tools and dependency resolution are pinned; Debian package
repositories and compiler output are not claimed to be byte-for-byte reproducible.
Refresh pins and rerun acceptance checks for security updates. Deployment should
use the resulting registry digest, not a movable version or `latest` tag.

## Separate configuration

Compose requires version 2.30 or newer for raw environment files. Keep both files
outside the checkout, owned by the deploying account, with mode 0600. Raw file
values are literal: no `export`, shell expansion, surrounding quotes or multiline
secrets. These are separate service files, never a shared application `.env`.

Example `/etc/todoer-rebuild/app.env` (replace placeholders privately):

```dotenv
TODOER_GOOGLE_CLIENT_ID=replace-privately
TODOER_GOOGLE_CLIENT_SECRET=replace-privately
TODOER_SERVER_KEY=replace-with-at-least-32-random-bytes
TODOER_REGISTRATION_ALLOWLIST=owner@example.invalid
```

An empty or absent registration allowlist leaves registration closed except for
explicit invitations. Do not enable development authentication in production.
Google's authorized callback must be exactly
`https://YOUR_ORIGIN/auth/google/callback`; verify the configured route and origin
against [authentication](authentication.md) before enabling sign-in.

Optional attachment storage requires all six `TODOER_ATTACHMENTS_*` values:
`ENDPOINT`, `BUCKET`, `REGION`, `PREFIX`, `ACCESS_KEY_ID`, `SECRET_ACCESS_KEY`.
Optional SMTP, VAPID and legacy-source settings belong only in app.env; see
[notifications](notifications.md), [push](push.md) and
[migration/export](migration-export.md). Do not put replication credentials there.

Example `/etc/todoer-rebuild/replica.env`:

```dotenv
TODOER_REPLICA_ENDPOINT=https://s3.example.invalid
TODOER_REPLICA_REGION=replace-privately
TODOER_REPLICA_BUCKET=replace-privately
TODOER_REPLICA_PREFIX=todoer-rebuild/primary
TODOER_REPLICA_ACCESS_KEY_ID=replace-privately
TODOER_REPLICA_SECRET_ACCESS_KEY=replace-privately
```

Grant the replica identity access only to its replication namespace. Attachment
and replica credentials must differ; use different buckets or prefixes. The
replica process receives only these six settings and the fixed DB path. It never
receives Google, SMTP, attachment or application signing credentials. Conversely,
the app receives no replica credentials. File-based Compose settings become
container environment variables, so Docker administrators can inspect them;
these files are not an encrypted secret store. Restrict Docker host access.

Set only the deployment metadata in your shell:

```sh
export TODOER_REBUILD_APP_ENV=/etc/todoer-rebuild/app.env
export TODOER_REBUILD_REPLICA_ENV=/etc/todoer-rebuild/replica.env
export TODOER_PUBLIC_URL=https://todoer.example.invalid
export TODOER_REBUILD_PROXY_NETWORK=existing-proxy-network
export TODOER_REBUILD_IMAGE=ghcr.io/mattmezza/todoer.pro-rebuild@sha256:REVIEWED_DIGEST
export TODOER_REBUILD_VOLUME=todoer_rebuild_data_v1
python3 scripts/rebuild-release-prepare.py check-config \
  --app-env "$TODOER_REBUILD_APP_ENV" --replica-env "$TODOER_REBUILD_REPLICA_ENV" \
  --origin "$TODOER_PUBLIC_URL" --image "$TODOER_REBUILD_IMAGE"
docker compose -f deploy/rebuild/compose.yml config --quiet
```

The validator prints no secret values. It checks the immutable isolated image
reference, production auth, HTTPS public and replication endpoints,
complete optional attachment configuration, separate credentials and file modes.
Avoid printing full `docker compose config`: the expanded configuration contains
secrets. The image reference must be replaced with a real reviewed digest.

## Network, volume and startup

The existing external proxy network must already exist. Configure your existing
TLS reverse proxy to route the canonical public host to `todoer-rebuild:8092`
on that network. Preserve the external HTTPS scheme and Host, and support SSE
and normal request timeouts. Compose creates no reverse proxy, published host
port, DNS entry or TLS certificate. The app binds `0.0.0.0:8092` internally and
requires a canonical HTTPS public URL for production cookies and OAuth.

Both services run as UID/GID 10001 with read-only root filesystems, dropped Linux
capabilities and private temporary filesystems. The app creates the new named
volume first; Litestream waits for the app health check and uses the same volume
and UID. The app's `/tmp` is bounded to 640 MiB for bounded transfer uploads.
The database is `/data/todoer.db`; app and replica share its WAL/SHM directory.
The Litestream container is on its own egress network and is not attached to the
proxy network. The replica has write access to the database volume because
SQLite coordination and replication bookkeeping require it.

Do not choose a legacy volume name or bind-mount `deploy/data`. Before importing
legacy data, retain an independent verified backup and follow the explicit
[migration process](migration-export.md). Import is never a startup action.

After container build acceptance and a tested isolated restore:

```sh
docker compose -f deploy/rebuild/compose.yml up -d
docker compose -f deploy/rebuild/compose.yml ps
```

The health check checks process HTTP liveness, not replica lag or SMTP delivery.
Verify authorized browser sign-in, list access, upload/download checksums, SSE,
MCP and shutdown on the isolated deployment. Confirm UID 10001 owns the new
volume and neither service can read the other service's credentials. Test
SIGTERM: the app gets 45 seconds and replica 35 seconds to finish bounded work
and final replication. Monitor replication errors and perform recurring restore
drills; a running container alone does not establish recoverability.

Litestream configuration is [deploy/rebuild/litestream.yml](../../deploy/rebuild/litestream.yml).
The synthetic real-S3 replication/restore check is
[scripts/rebuild-litestream-check.py](../../scripts/rebuild-litestream-check.py).
See the [replication runbook](replication.md) for verified evidence and restore procedures.
Restore into a **new empty volume/path**, verify it, then point an isolated app at
it. Never restore over a running database. Database replicas do not back up
private attachment objects; maintain their separate storage retention policy.

## Release workflow

`.github/workflows/rebuild-release.yml` only runs on `rebuild-v*` tags in
`mattmezza/todoer.pro`, keeping the existing legacy `v*` release workflow separate.
Maintainers should protect the release branch and restrict release-tag creation.
Tags must be valid `rebuild-vMAJOR.MINOR.PATCH[-prerelease]`. The tagged commit
must belong to the fetched default branch, or the explicitly configured trusted
`TODOER_RELEASE_BRANCH` repository variable. Invalid tags and fork repositories
fail closed. Creating a release tag is an explicit maintainer action; this work
does not create or push any tag.

Configure `TODOER_RELEASE_RUNNER_LABELS` as a JSON array of private runner labels
(default `["self-hosted","todoer-private"]`). Those runners need Docker/Buildx,
Python 3 and Rust 1.99.0 with rustfmt/Clippy, plus outbound public registry access.
No pull request or `pull_request_target` trigger reaches the release runners.
Checks run without package-write permissions. The publishing job receives only
`contents:read` and `packages:write`, uses the scoped GitHub token, pins all action
commits, emits BuildKit provenance/SBOM and publishes version plus full-commit
tags to the separate `ghcr.io/mattmezza/todoer.pro-rebuild` package.

All publishing jobs serialize. After publishing an immutable image digest, a
fresh tag fetch is performed by checkout with its scoped read token. Only a
stable version at least as new as every existing stable rebuild tag may promote
`latest`. Prereleases and older stable releases never promote it. The digest is
reported in the job summary and publishing job outputs. No repository write, remote server command or
automatic deployment occurs. `latest` is a convenience tag; rollout and rollback
use reviewed image digests plus compatible database backups, since an old binary
may not understand newer migrations.

Local policy checks:

```sh
python3 scripts/rebuild-release-prepare.py self-test
git tag --list 'rebuild-v*' > /tmp/todoer-rebuild-tags.txt
python3 scripts/rebuild-release-prepare.py policy \
  --repository mattmezza/todoer.pro --tag rebuild-v1.0.0 \
  --tags-file /tmp/todoer-rebuild-tags.txt
```

These commands prepare and validate policy; they do not publish anything.

References: [Docker environment files](https://docs.docker.com/compose/how-tos/environment-variables/set-environment-variables/),
[external networks](https://docs.docker.com/compose/how-tos/networking/),
[official Rust image](https://hub.docker.com/_/rust/),
[Litestream sidecars](https://litestream.io/guides/docker/) and
[GitHub package publishing permissions](https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions).

Apply database migrations explicitly before a controlled local release with
`TODOER_RUST_DB=/absolute/path/to/backed-up-rebuild.db make rust-migrate`.
This command opens only the rebuild database, applies pending numbered migrations
transactionally, and exits without a listener, Google configuration or workers.
Normal startup also applies pending migrations. Both paths reject a legacy schema
and reject an already newer rebuild schema before changing WAL mode or data.
Take a complete database/attachment recovery point before upgrading. An older
binary is not a schema downgrade; restore a compatible database recovery point
and its retained blobs into isolated storage before any authorized rollback.
