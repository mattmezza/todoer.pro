//! Synthetic core timing probe; run explicitly with --ignored --nocapture.
use std::time::{Duration, Instant};
use todoer::core::{Capture, NewTask, Store};

fn report(name: &str, samples: &mut [Duration]) {
    samples.sort();
    let total: Duration = samples.iter().sum();
    println!(
        "{name}: n={} total_ms={:.3} median_ms={:.3} p95_ms={:.3} max_ms={:.3}",
        samples.len(),
        total.as_secs_f64() * 1000.0,
        samples[samples.len() / 2].as_secs_f64() * 1000.0,
        samples[((samples.len() * 95).div_ceil(100) - 1).min(samples.len() - 1)].as_secs_f64()
            * 1000.0,
        samples.last().unwrap().as_secs_f64() * 1000.0
    );
}

#[test]
#[ignore = "explicit synthetic performance probe; no latency assertions"]
fn ten_thousand_tasks_with_descriptions_and_nested_subtrees() {
    let mut store = Store::memory().unwrap();
    store.seed_dev().unwrap();
    let mut capture_times = Vec::new();
    let mut root = None;
    let mut parent = None;
    // Across the first five batches, 100 tasks form five levels of 20 nodes. Each level
    // hangs from the first node of the preceding level: one 81-node subtree.
    for batch in 0..100 {
        let started = Instant::now();
        let created = store
            .capture(
                "dev",
                &format!("perf-batch-{batch}"),
                Capture {
                    list_id: "inbox".into(),
                    tasks: (0..100)
                        .map(|index| NewTask {
                            title: format!("Synthetic task batch {batch} item {index}"),
                            parent_id: if batch < 5 && index < 20 {
                                parent.clone()
                            } else {
                                None
                            },
                            description:
                                "Synthetic common searchable description with planning details. "
                                    .repeat(8),
                        })
                        .collect(),
                },
            )
            .unwrap();
        assert_eq!(created.len(), 100);
        capture_times.push(started.elapsed());
        if batch < 5 {
            if root.is_none() {
                root = Some(created[0].id.clone());
            }
            parent = Some(created[0].id.clone());
        }
    }
    println!(
        "fixture: tasks=10000 description_bytes={} capture_batches={} max_batch=100 nested_levels=5 nested_tasks=100 subtree_nodes=81 profile=debug sqlite=in_memory",
        "Synthetic common searchable description with planning details. ".len() * 8,
        capture_times.len()
    );
    report("capture", &mut capture_times);
    let mut search_times = Vec::new();
    let mut list_times = Vec::new();
    let mut subtree_times = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let results = store
            .search("dev", "comm search", Some("inbox"), None, 100, None)
            .unwrap();
        search_times.push(started.elapsed());
        assert_eq!(results.len(), 100);
        let started = Instant::now();
        let all = store.tasks("dev", "inbox").unwrap();
        list_times.push(started.elapsed());
        assert_eq!(all.len(), 10_000);
        let started = Instant::now();
        let tree = store.subtree("dev", root.as_ref().unwrap()).unwrap();
        subtree_times.push(started.elapsed());
        assert_eq!(tree.len(), 81);
    }
    report("fts_prefix_100", &mut search_times);
    report("full_list_10000", &mut list_times);
    report("subtree_81", &mut subtree_times);
    let mut cursor = 0;
    let mut count = 0;
    let mut change_times = Vec::new();
    loop {
        let started = Instant::now();
        let page = store.changes("dev", cursor).unwrap();
        change_times.push(started.elapsed());
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 1000);
        let next = page.last().unwrap()["seq"].as_i64().unwrap();
        assert!(next > cursor);
        cursor = next;
        count += page.len();
    }
    assert_eq!(count, 10_000);
    println!(
        "changes: events={count} pages_including_empty={}",
        change_times.len()
    );
    report("change_pages_1000", &mut change_times);
}
