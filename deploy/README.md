# VPS deployment

Production runs only the Rust app. The public site and docs use separate hosts.
Nothing in CI deploys to the VPS. Requires Docker Engine, Compose v2, host nginx
with existing TLS, Git and outbound HTTPS. No `--wait`, env-file objects, profiles
or external proxy network are required. The image currently targets amd64 Linux.

## First preparation (does not migrate or start the app)

```sh
git clone --depth 1 --branch main https://github.com/mattmezza/todoer.pro.git
cd todoer.pro
cp deploy/.env.example deploy/.env
cp deploy/app.env.example deploy/app.env
chmod 600 deploy/.env deploy/app.env
sudo install -d -m 700 -o 10001 -g 10001 /var/lib/todoer
```

Edit `.env` with the released image (`ghcr.io/mattmezza/todoer.pro:vX.Y.Z`,
preferably `@sha256:...`), canonical origin and host port. Configure `app.env`
with real authentication and storage settings. Generate `TODOER_SERVER_KEY` with
`openssl rand -hex 32` and back it up separately. Never use development auth.
Optional integrations are documented under `docs/rebuild/`. Copy Google callback
paths from authentication docs and configure app.todoer.pro with the provider.
The environment examples contain placeholders, not working production secrets.
Database and private configuration live outside the disposable application image.
Never use `docker compose config` without `--quiet` in shared logs: it exposes env.

## Initial cutover

Follow [migration](MIGRATION.md) **before** starting an empty production database.
Keep the current Python app and database until migration is verified together.
Do not reuse its database path for the Rust app. The application automatically
applies Rust schema migrations on startup; upgrading is a database write.

After a verified migrated database exists at `/var/lib/todoer/todoer.db` with
ownership 10001:10001, run:

```sh
./deploy/deploy.sh v1.0.1
```

Add `nginx.conf.example` locations to your existing TLS virtual host. Validate
`sudo nginx -t`, then reload nginx. Point app.todoer.pro DNS to the VPS and check
`https://app.todoer.pro/ready`, login, task counts, a write and attachment download.
App port 8092 binds **only loopback**. Keep the DB directory private and don't
expose it through nginx. GHCR package visibility is independent of repository
visibility: make the image package public after its first release, or authenticate
the VPS with a read:packages token through `docker login ghcr.io`.

For a standalone Certbot webroot setup, see `nginx-app.conf.example`. Keep its
HTTP challenge location available for renewal. The optional
`certbot-reload-nginx.sh` deploy hook validates and reloads nginx after any
successful certificate renewal; install it executable under
`/etc/letsencrypt/renewal-hooks/deploy/`, check that the Certbot timer is enabled,
and test both the hook and `certbot renew --dry-run --cert-name app.todoer.pro`.
The deploy hook runs for renewals of other certificates on the host as well.

## Updates and rollback

1. Take a verified SQLite backup (online SQLite backup API, not a live-file copy)
   and preserve attachment objects and the server key. See recovery docs.
2. `git pull --ff-only origin main` works from the shallow clone; never check out
   release tags over private configuration. Review deploy-file changes.
3. Run `./deploy/deploy.sh v1.0.1` with the published version you intend to deploy.
   The argument selects `ghcr.io/mattmezza/todoer.pro:v1.0.1`, overriding `.env`.
   For digest-pinned operation, use direct Compose commands with `TODOER_IMAGE`
   set to the reviewed digest and perform the same health checks.
4. Inspect `cd deploy && docker compose --project-name todoer logs --tail 100 app`
   privately; verify the HTTPS endpoint and a task read/write.

The script polls Docker health after `up -d`; it does not roll back data on failure.
For rollback, stop the new service, preserve the failed DB for investigation,
restore the matching pre-upgrade backup to a new private directory, set
`TODOER_DATA_DIR` and the prior image digest, then start and verify. An older image
may not understand a newer schema. Never blindly downgrade against the current DB.
Backups exclude work written after the snapshot; reconcile that work before rollback.
Do not use `down -v` or delete the original data during cutover.

## Container design

The runtime uses digest-pinned [Distroless](https://github.com/GoogleContainerTools/distroless)
with glibc, OpenSSL and CA roots required by the Rust binaries. Build tooling stays
in build stages. There is no runtime shell, package manager or curl; a small Rust
probe checks `/ready`. Compose runs UID 10001, read-only, with capabilities dropped.
Smaller images still need reviewed base-image updates and vulnerability scanning.

## Optional continuous backups

Copy `replica.env.example` to `replica.env`, chmod 600, configure a private S3
bucket and credentials distinct from attachments, then enable `COMPOSE_FILE` in
`.env`. Litestream gets only replica credentials. Run the normal deploy script.
Rehearse a restore into a new path and verify integrity and task counts before
relying on replication. A replica does not replace pre-upgrade backups; preserve
attachment objects and the encryption key independently. Health success is for
the app; also inspect replica status/logs and periodically rehearse recovery.

Litestream stays in an optional Compose overlay because self-hosters can choose
their own backup system. Its [official Docker guide](https://litestream.io/guides/docker/)
recommends the sidecar pattern for Docker and also supports `replicate -exec` for
single-container hosts. Compose can manage both processes independently, so the
app image does not need Litestream or a supervisor. `replica.env` keeps replication
credentials out of the app process; `app.env` keeps authentication, AI and attachment
credentials out of Litestream. Use separate bucket-scoped credentials so an app
credential compromise does not also grant access to database backups.
