# Local attachment storage

Start the private S3-compatible MinIO service and initialize its buckets:

```sh
docker compose -f deploy/local/compose.yml up -d
source deploy/local/attachments.env
```

The environment file contains deliberately public **local-only** credentials.
Start the app from that same shell so it inherits the six storage settings.
For development authentication with the existing local database:

```sh
TODOER_DEV_AUTH=1 \
TODOER_DEV_TOKEN=local-integration-token-not-for-production \
TODOER_RUST_DB=.local/rebuild/rust.db \
TODOER_RUST_PORT=8092 target/debug/todoer-server
```

MinIO console: http://127.0.0.1:9001. User `synthetic-test`, password
`synthetic-test-secret`. Both console and S3 ports bind only to loopback.
Application uploads use `todoer-attachments/local-dev`; automated fixtures use
a separate `todoer-synthetic-attachments` bucket. Data persists in a Docker volume.

In the app, open a task's full page, select a file in Files, upload, reload,
and download it. Check that the downloaded bytes match the original. Delete the
attachment and confirm it disappears after reload (metadata deletion retains bytes).
To test an outage, stop storage, try a new upload, and confirm the error retains
its retry state; restart storage and retry the same file:

```sh
docker compose -f deploy/local/compose.yml stop storage
docker compose -f deploy/local/compose.yml start storage
```

Run the isolated HTTP/storage integration tests:

```sh
TODOER_TEST_S3_ENDPOINT=http://127.0.0.1:9000 \
TODOER_TEST_S3_BUCKET=todoer-synthetic-attachments \
CARGO_INCREMENTAL=0 cargo test -p todoer --lib local_s3_ -- --ignored --nocapture
```

Verified against this MinIO setup: both tests passed, including signed and proxy
uploads, 200 KiB HTTP uploads, checksums, immutable finalization/retries, private
anonymous denial, downloads and access revocation. These tests use in-memory
application databases and do not add tasks to the development database.

Stop containers while keeping uploads: `docker compose -f deploy/local/compose.yml down`.
