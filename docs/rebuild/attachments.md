# Task discussion and private attachments

The rebuild has one attachment metadata model for a task, optionally linked to a comment on that same task. Comments use independent revisions and chronological sequence cursors (`after_seq`, limit 1–200). Creating comments requires current list write access; editing and soft deletion additionally require the author or a workspace owner with current list write access. Comment history uses distinct `comment.create`, `comment.update`, and `comment.delete` kinds; it does not increment task revision or qualify for task-edit undo. History stores comment identifiers/revisions separately from its task snapshot, with human and credential attribution. Prior comment content is retained in internal `before_event` audit data. Deleted comments are hidden, along with their attachments; soft deletion does not erase stored bytes.

Attachment upload intents require current task/list write access, a safe display basename, expected byte length (0–25 MiB), and a 64-character hexadecimal SHA-256 checksum. A linked comment must exist on the same live task. Intent signing and server-proxy upload expire after 15 minutes. An intent's creator or an authorized list owner can finish it; every metadata operation rechecks live access. An intent has independent staging and final opaque keys, containing no original filename. Filenames lose directory components, control characters, and directional override/isolation characters, and must have a remaining basename no longer than 255 UTF-8 bytes.

The client can upload using a signed staging PUT or send its bytes through the authenticated application upload endpoint. Signed PUTs bind Content-Length, application/octet-stream, checksum, and If-None-Match:*. Retries reuse the same intent/body. An existing staging object is accepted by the server proxy only after verification. Finalization performs HEAD size/checksum checks and a bounded GET with server-computed SHA-256, then writes verified bytes to a separate immutable final key using If-None-Match:*. If promotion already succeeded, its bytes are verified before retrying metadata finalization. This separation prevents an unexpired upload URL from changing a finalized attachment. SQLite finalization checks current access, live task/comment, intent revision/expiry, and the opaque verified result produced by the storage layer. Repeated finalization returns the original attachment; deleted attachments cannot be resurrected this way.

Downloads must call `Store::attachment` for each request and proxy `ObjectStorage::get` through the application. No presigned GET URL is exposed. The storage GET is bounded and verifies its size and digest. Responses must set `Content-Type: application/octet-stream`, `Content-Disposition: attachment` using the exported RFC 5987 filename helper, and `X-Content-Type-Options: nosniff`. Access revoked before a new request takes effect on that request; already downloaded bytes cannot be recalled. The application must not cache private download responses.

## Storage configuration

Configure these six server environment variables together; all absent disables attachments, partial configuration is rejected:

- `TODOER_ATTACHMENTS_ENDPOINT`
- `TODOER_ATTACHMENTS_BUCKET`
- `TODOER_ATTACHMENTS_REGION`
- `TODOER_ATTACHMENTS_PREFIX`
- `TODOER_ATTACHMENTS_ACCESS_KEY_ID`
- `TODOER_ATTACHMENTS_SECRET_ACCESS_KEY`

These settings are independent of SQLite backup/replication storage. Do not derive them from Litestream settings. HTTPS is required except for explicit loopback HTTP endpoints used by local protocol tests. The client uses path-style S3 URLs and does not follow redirects. Provision a private bucket and credentials limited to the configured attachment prefix. No public ACL is requested; operators must also prohibit public bucket policies/ACLs and public anonymous access. Bucket health checks verify reachability/authorization, not the whole deployment's privacy policy. Browser direct upload additionally requires a deliberately scoped CORS policy permitting the signed PUT headers; the authenticated server-proxy path avoids that requirement.

The pinned [rusty-s3 0.10.2 documentation](https://docs.rs/rusty-s3/latest/rusty_s3/) describes its Sans-IO signing approach. Todoer sends signed requests with its existing reqwest client; [S3Action's signing contract](https://docs.rs/rusty-s3/latest/rusty_s3/actions/trait.S3Action.html) requires the signed headers to match the actual request. Signed upload URLs are temporary credentials: do not record them in logs, analytics, or history.

Network operations have a 5-second connection timeout and 60-second total timeout. Object reads consume chunks with an explicit expected-size cap and return at most 25 MiB in memory; they are bounded, not a zero-copy streaming download. A finalization may allocate the verified object bytes while promoting it. No multipart upload or files larger than the cap are supported.

## Verification performed

Domain tests use only generated task/comment/attachment data. They cover chronology, author/owner moderation, viewer denial, stale revisions, task/comment scope, filename normalization, length/checksum validation, expiration, soft deletion, live revocation, and idempotent metadata finalization. A test-only opaque verification constructor supports isolated metadata tests; production finalization receives results only from actual object verification.

An additional ignored integration test ran against **Moto 5.1.17**, a local development S3 protocol emulator. [Moto documents its standalone server mode](https://docs.getmoto.org/en/latest/docs/server_mode.html). This test made actual HTTP signed PUT, HEAD, and GET requests, rather than replacing the Rust client with an in-process mock. It verified direct and proxy upload, private anonymous GET denial (403), conditional overwrite denial (412), digest/size checks, immutable promotion, finalization retry, and retained bytes after metadata soft deletion.

Reproduce with an isolated local Moto process and an empty synthetic bucket; the credentials below are public test fixtures:

```sh
uv run --no-project --python 3.11 --with 'moto[server]==5.1.17' moto_server -H 127.0.0.1 -p 8937
```

In another terminal:

```sh
uv run --no-project --python 3.11 --with 'moto[server]==5.1.17' python -c 'import boto3; boto3.client("s3", endpoint_url="http://127.0.0.1:8937", region_name="us-east-1", aws_access_key_id="synthetic-test", aws_secret_access_key="synthetic-test-secret").create_bucket(Bucket="todoer-synthetic-discussion")'
TODOER_TEST_S3_ENDPOINT=http://127.0.0.1:8937 TODOER_TEST_S3_BUCKET=todoer-synthetic-discussion cargo test --manifest-path rebuild/Cargo.toml object_storage::tests::local_s3_put_head_get_and_immutable_finalization --lib -- --ignored --nocapture
```

Actual result on 2026-10-26:

```text
local S3 protocol: direct signed PUT + proxy PUT, HEAD, bounded GET, checksum verification, private anonymous denial, immutable retries, metadata finalize and soft-delete passed
test object_storage::tests::local_s3_put_head_get_and_immutable_finalization ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 0.21s
```

This demonstrates the HTTP integration contract with an emulator; it does not certify AWS/Garage/MinIO signing enforcement, production IAM, TLS, CORS, lifecycle policies, or durability. Run equivalent tests against the chosen private production-compatible service before deployment. The test intentionally refuses non-loopback endpoints and requires a synthetic bucket prefix.

## Retention and recovery limits

Expired or abandoned intents and staging/promoted orphan objects are retained. There is no automatic object deletion or retention job. Attachment deletion changes metadata only and keeps bytes. Establish a separately reviewed retention period and inventory objects against intents/attachments before manual cleanup; never remove recent orphan candidates while finalization may be in flight. Back up SQLite and object bytes with their immutable keys together. SQLite-only backups do not constitute complete attachment recovery. This increment does not claim cloud replication, complete disaster recovery, malware scanning, or production storage acceptance.
