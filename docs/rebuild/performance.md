# Synthetic core performance probe

Run on 2026-10-26 in this development container (x86_64, rustc 1.99.0, debug test profile, bundled SQLite, in-memory database):

```sh
cargo test --manifest-path app/Cargo.toml --test performance -- --ignored --nocapture
```

`app/tests/performance.rs` is ignored by default and uses `Store::memory()` plus `seed_dev()`. It creates only synthetic content: exactly 10,000 tasks in 100 capture transactions of 100 tasks, each with a 504-byte description. Among the first five batches, 20 tasks per batch form five nested levels. One root's subtree contains 81 tasks (the other 19 first-level nodes are siblings). All titles, descriptions, and idempotency keys are generated locally. No existing database or personal content is read.

The test times core calls using `Instant`, verifies task counts/search results/subtree size/change pagination, and prints measurements without asserting machine-dependent latency thresholds. Timings include core SQL, object allocation and deserialization, history and FTS writes where applicable. They exclude startup/migrations, network, HTTP/HTML rendering, JavaScript, disk I/O, and browser interaction. Read calls run 20 times; change traversal runs through ten 1,000-event pages and a terminal empty page. Percentiles use sorted samples and nearest-rank p95; this small sample is a diagnostic, not a statistical service-level claim.

Baseline completed run output (before the recursive parent-index correction):

```text
fixture: tasks=10000 description_bytes=504 capture_batches=100 max_batch=100 nested_levels=5 nested_tasks=100 subtree_nodes=81 profile=debug sqlite=in_memory
capture: n=100 total_ms=5630.608 median_ms=49.665 p95_ms=74.807 max_ms=84.795
fts_prefix_100: n=20 total_ms=809.285 median_ms=39.531 p95_ms=45.183 max_ms=52.963
full_list_10000: n=20 total_ms=1272.298 median_ms=60.577 p95_ms=75.353 max_ms=76.936
subtree_81: n=20 total_ms=11042.477 median_ms=544.393 p95_ms=598.905 max_ms=648.171
changes: events=10000 pages_including_empty=11
change_pages_1000: n=11 total_ms=298.629 median_ms=28.143 p95_ms=41.945 max_ms=41.945
test ten_thousand_tasks_with_descriptions_and_nested_subtrees ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.16s
```

The prefix query `comm search` matches common description terms and returns a bounded 100 rows. This is a deliberately broad matching query. Capture measurements combine UUID generation, transactional inserts, history, FTS, and idempotent response persistence; batches later in the fixture run against more existing rows.

The baseline's slow subtree query was investigated with `EXPLAIN QUERY PLAN` against a synthetic 10,000-row table using the same bundled rusqlite dependency and relevant schema/indexes. SQLite selected `SEARCH t USING INDEX tasks_deleted (deleted_at=?)` followed by `SCAN tree` for the recursive step: it revisited live tasks instead of looking up each parent's children. Adding `INDEXED BY tasks_parent` only to the recursive child arm changes that step to `SCAN tree` followed by `SEARCH t USING INDEX tasks_parent (parent_id=?)`. Root selection, live-row filtering, recursive membership, result columns, and final ordering remain unchanged. The existing migration-001 index is reused; no schema migration is needed. Renaming/removing that index now requires updating this query too.

After this bounded query correction, the same fixture and explicit command passed with the following output. Median subtree time decreased from 544.393ms to 1.461ms on these two runs. These are sequential development-container measurements rather than a controlled production benchmark; the plan evidence explains the difference, while minor changes in other measurements should not be attributed to this correction.

```text
fixture: tasks=10000 description_bytes=504 capture_batches=100 max_batch=100 nested_levels=5 nested_tasks=100 subtree_nodes=81 profile=debug sqlite=in_memory
capture: n=100 total_ms=4788.459 median_ms=46.875 p95_ms=54.778 max_ms=70.307
fts_prefix_100: n=20 total_ms=784.924 median_ms=38.868 p95_ms=41.036 max_ms=44.151
full_list_10000: n=20 total_ms=1207.974 median_ms=58.927 p95_ms=66.026 max_ms=75.329
subtree_81: n=20 total_ms=29.635 median_ms=1.461 p95_ms=1.768 max_ms=1.776
changes: events=10000 pages_including_empty=11
change_pages_1000: n=11 total_ms=267.196 median_ms=25.350 p95_ms=37.098 max_ms=37.098
test ten_thousand_tasks_with_descriptions_and_nested_subtrees ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.18s
```

At the initial performance increment, `cargo test --manifest-path app/Cargo.toml --lib` also passed its then-current 15 tests, including permission, subtree completion/move, soft deletion/restoration, and migration behavior.

This does **not** establish full performance acceptance. `Store::tasks()` materializes the entire list, and the current UI does not virtualize that list. The server shares one SQLite connection behind a mutex, serializing core operations; concurrent request behavior and contention are unmeasured. This fixture covers five levels but not adversarial depth, giant sibling sets, many lists, large text bodies, attachment bytes, or comment workloads. Comments and attachments were added to the schema after these measurements; their presence does not make this earlier fixture a measurement of those features. It does not measure process memory, release builds, persistent WAL/checkpoint behavior, backup/restore, import performance, browser layout, mobile devices, or end-to-end latency. Future acceptance requires those separately, with explicit product latency and memory budgets.

## Scope after later integration

The output above is a historical 2026-10-26 before/after diagnostic, not a fresh
benchmark of the expanded application. Later code adds bounded projected subtree
reads and migration 020 child-order/history-watermark indexes. Those are distinct
from this original `Store::subtree`/`tasks_parent` correction; the original timings
must not be attributed to the later indexes. Authentication, comments, files,
structured multi-entity search, recurrence, statistics and assistance need their
own representative workloads. No additional performance run was made during the
runbook correction. The serialized-connection, full-list rendering and unmeasured
concurrency/memory/persistent-storage limits above remain acceptance work.

## Persistent synthetic HTTP probe, 2026-10-26

The new standard-library Python harness starts its **own** built server on a free
loopback port with a fresh database. It strips inherited `TODOER_` configuration,
uses synthetic development authentication, stops the server before inserting the
fixture, and maintains SQLite's FTS/search triggers. It refuses an existing output
directory/database. It never reads the legacy or ordinary development databases.

```sh
cargo build --manifest-path app/Cargo.toml --locked --bin todoer-server
python3 scripts/rebuild-http-performance.py \
  --binary target/debug/todoer-server \
  --output .local/rebuild/http-performance-20261026c --repetitions 7
```

Choose a new output path when repeating. Omitting `--output` uses a temporary
directory deleted after completion. A retained run contains only synthetic SQLite
data, a local server log and `measurements.json`; do not commit these artifacts.
Fixture insertion bypasses application capture/history intentionally, so this run
measures reads and a pair of real HTTP updates, not bulk-write/import throughput.
There are 10,000 tasks and 10,000 comments across 100 readable lists (100 tasks per
list), an additional 1,000-task stress list, and one unreadable private sentinel.
Descriptions are 540 UTF-8 bytes; titles vary from approximately 26 to 106
characters. The first list includes a 50-node binary subtree. Comments contain
representative repeated synthetic discussion text. No attachment payloads are
included.

This completed x86_64 Linux **debug** binary run recorded SHA-256
`31ac58868fccc69f8955f9131850b7ab129be258c0d55c4945a053ed2710d612`.
Python's fixture connection used SQLite 3.54.0; the HTTP server uses the bundled
rusqlite build. Each ordinary measurement has seven samples; the subtree traversal
has its three actual pages. p95 is nearest rank (therefore maximum with seven
samples), with no machine-time assertions and no artificial warmup excluded.
The measurements include loopback HTTP, server queueing/authorization/SQL,
serialization or HTML rendering, and client response-body reads. They exclude
browser parsing/layout/script execution, TLS, remote latency and asset loading.

| HTTP operation | Samples | Median ms | p95 ms | Response bytes |
| --- | ---: | ---: | ---: | ---: |
| Legacy literal task prefix, 100 results | 7 | 66.266 | 88.182 | 77,828 |
| Structured task/comment prefix, first 100 | 7 | 1,663.480 | 1,748.266 | 13,639 |
| Structured prefix cursor, next 100 | 7 | 1,750.024 | 1,877.868 | 13,639 |
| Projected subtree, first 17 | 7 | 14.919 | 17.677 | 3,590 |
| All 50 subtree nodes, per-page | 3 | 14.958 | 15.960 | 3,032–3,590 |
| Typical list HTML, 100 tasks | 7 | 153.977 | 177.826 | 225,620 |
| Stress list HTML, 1,000 tasks | 7 | 1,431.138 | 1,808.933 | 2,047,878 |

The broad prefix `Bench` matches 21,000 authorized task/comment documents. The
harness verifies the exact authorized total, no repeated IDs across query cursor
pages, no private sentinel, all 50 unique subtree IDs across pages, and response
bounds (17 items requested, 256 KiB subtree maximum). These are different search
implementations/projections: their times should not be treated as a controlled
comparison of only their indexes. Structured search still computes authorized
matching totals and does substantial work despite the 100-result response limit.
At this debug workload its approximately 1.7-second broad search is an explicit
performance limitation, not evidence that interactive search is fast.

A simultaneous cookie-authenticated human PATCH and development bearer-authenticated
agent PATCH used the same task revision. The agent received 200 in 22.365 ms and
the human received 409 in 23.078 ms; exactly one update persisted at revision 2.
Which writer wins is intentionally unspecified. This verifies the CAS behavior
under two concurrent local requests, not load capacity, production OAuth session
behavior or fairness under sustained contention.

The database remained in WAL mode with a WAL file present while running. After
clean stop/restart the winning title/revision persisted, an HTTP structured search
found that edited task, SQLite `integrity_check` returned `ok`, and
`foreign_key_check` returned no violations. The main database was 35,909,632 bytes
after restart. This is a graceful persistent restart check; it does not simulate
power loss or verify disaster recovery (the separate Litestream probe does).

The current full-list HTML has no virtualization: 1,000 tasks generated roughly
2 MB before browser work. Splitting a 10,000-task account across 100 lists is the
representative case here; rendering all 10,000 in one list is intentionally not
presented as an acceptable mobile interaction. Browser memory, DOM/layout time,
release profile and high concurrent load remain separate acceptance measurements.
These results establish reproducible correctness and practical local limits, not
production latency or memory SLOs.

### Optional actual Chromium phone probe

The harness now snapshots the supplied executable into its private fixture output
before starting any process, so a concurrent rebuild cannot change the binary used
for the restart phase. `--profile debug|release` records the caller's build choice;
it does not infer optimization from a filename. Optional `--browser` invokes
`scripts/rebuild-http-performance-browser.cjs` with the repository's installed
Playwright Chromium. Install the repository Node dependencies and Playwright
Chromium first; no browser is downloaded by this harness.

```sh
python3 scripts/rebuild-http-performance.py \
  --binary target/debug/todoer-server --profile debug --repetitions 3 --browser \
  --output .local/rebuild/http-performance-phone-20261026
```

This independent completed run used debug binary SHA-256
`6bfcdde3f611f476ae6f192acd8ba1a52e0cd8c798945e5336b42d286d473512`,
Chromium 141.0.7390.37, a 390×844 phone viewport, mobile/touch emulation and dark
mode. Service workers were blocked to measure the running server rather than an
offline cache. It uses a fresh cookie-authenticated browser, actual JS execution,
actual DOM/layout, two animation frames after application-ready, and viewport
screenshots saved with the synthetic output. Browser navigation samples are one
per list; these are not percentiles or repeatable SLO estimates.

| Browser measurement | 100-task list | 1,000-task stress list |
| --- | ---: | ---: |
| Actual task rows | 100 | 1,000 |
| DOM elements | 2,026 | 16,426 |
| Navigation response end, ms | 128.7 | 1,322.5 |
| DOMContentLoaded end, ms | 263.8 | 2,003.3 |
| Application ready plus two frames, ms | 364.401 | 2,229.713 |
| Document/viewport width, px | 390 / 390 | 390 / 390 |

Both lists passed overflow checks and actual task menu/open-editor/close-editor
interactions. The stress list also passed complete/reopen actions with successful
HTTP responses and expected UI state (296.356 ms combined in this run). There
were zero uncaught JavaScript errors. Menu/editor action timings include
Playwright actionability waits and API-driven dialog content; they are recorded
in JSON as diagnostics rather than treated as direct input-latency measurements.
The browser's optional `performance.memory` counter is reported only as a raw
compatibility field: its quantized value is **not** a measurement of total
browser/process memory and is not used for acceptance.

This run repeated HTTP/CAS/WAL/restart checks successfully. In it the human won
the CAS race (200/409, revision 2), confirming neither winner is assumed. The
three-sample debug structured-prefix median was 1,350.969 ms and the cursor median
1,417.707 ms; the stress HTML remained 2,047,878 bytes. Variation from the earlier
seven-sample run reinforces why these measurements are diagnostics rather than
comparative production claims. The browser result makes the unvirtualized large
list cost visible: roughly 2.2 seconds to ready with over 16,000 elements on this
development machine. Emulated viewport/touch does not emulate a physical phone's
CPU, memory, battery or network. No CPU throttling, real mobile hardware, release
profile or production TLS measurement is claimed here.
