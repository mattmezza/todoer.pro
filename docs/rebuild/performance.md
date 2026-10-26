# Synthetic core performance probe

Run on 2026-10-26 in this development container (x86_64, rustc 1.99.0, debug test profile, bundled SQLite, in-memory database):

```sh
cargo test --manifest-path rebuild/Cargo.toml --test performance -- --ignored --nocapture
```

`rebuild/tests/performance.rs` is ignored by default and uses `Store::memory()` plus `seed_dev()`. It creates only synthetic content: exactly 10,000 tasks in 100 capture transactions of 100 tasks, each with a 504-byte description. Among the first five batches, 20 tasks per batch form five nested levels. One root's subtree contains 81 tasks (the other 19 first-level nodes are siblings). All titles, descriptions, and idempotency keys are generated locally. No existing database or personal content is read.

The test times core calls using `Instant`, verifies task counts/search results/subtree size/change pagination, and prints measurements without asserting machine-dependent latency thresholds. Timings include core SQL, object allocation and deserialization, history and FTS writes where applicable. They exclude startup/migrations, network, HTTP/HTML rendering, JavaScript, disk I/O, and browser interaction. Read calls run 20 times; change traversal runs through ten 1,000-event pages and a terminal empty page. Percentiles use sorted samples and nearest-rank p95; this small sample is a diagnostic, not a statistical service-level claim.

Actual completed run output:

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

The 81-node subtree query was substantially slower than the whole-list fetch in this debug run. Investigate recursive query planning with EXPLAIN QUERY PLAN and representative disk-backed data before claiming large-tree responsiveness. An existing `tasks_parent` index is already present; no cause is established by this timing test alone.

This does **not** establish full performance acceptance. `Store::tasks()` materializes the entire list, and the current UI does not virtualize that list. The server shares one SQLite connection behind a mutex, serializing core operations; concurrent request behavior and contention are unmeasured. This fixture covers five levels but not adversarial depth, giant sibling sets, many lists, large text bodies, attachment bytes, or comments (comments are not yet in the rebuild schema). It does not measure process memory, release builds, persistent WAL/checkpoint behavior, backup/restore, import performance, browser layout, mobile devices, or end-to-end latency. Future acceptance requires those separately, with explicit product latency and memory budgets.
