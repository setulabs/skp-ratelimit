//! Benchmarks for storage operations.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use skp_ratelimit::storage::{MemoryStorage, Storage, StorageEntry};
use std::time::Duration;
use tokio::runtime::Runtime;

fn bench_storage_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("storage");

    // Get operation
    group.bench_function("get_existing", |b| {
        let storage = MemoryStorage::new();
        rt.block_on(async {
            storage
                .set("bench:key", StorageEntry::new(1, 1000), Duration::from_secs(3600))
                .await
                .unwrap();
        });
        b.iter(|| {
            rt.block_on(async {
                black_box(storage.get("bench:key").await)
            })
        })
    });

    group.bench_function("get_missing", |b| {
        let storage = MemoryStorage::new();
        b.iter(|| {
            rt.block_on(async {
                black_box(storage.get("nonexistent:key").await)
            })
        })
    });

    // Set operation
    group.bench_function("set", |b| {
        let storage = MemoryStorage::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("bench:set:{}", i);
            rt.block_on(async {
                black_box(
                    storage
                        .set(&key, StorageEntry::new(1, 1000), Duration::from_secs(3600))
                        .await,
                )
            })
        })
    });

    // Increment operation
    group.bench_function("increment", |b| {
        let storage = MemoryStorage::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("bench:inc:{}", i % 100);
            rt.block_on(async {
                black_box(
                    storage
                        .increment(&key, 1, 1000, Duration::from_secs(3600))
                        .await,
                )
            })
        })
    });

    group.finish();
}

fn bench_storage_scaling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("storage_scaling");

    for num_keys in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("get_with_entries", num_keys), num_keys, |b, &num_keys| {
            let storage = MemoryStorage::new();
            
            // Pre-populate storage
            rt.block_on(async {
                for i in 0..num_keys {
                    let key = format!("scale:{}", i);
                    storage
                        .set(&key, StorageEntry::new(i, 1000), Duration::from_secs(3600))
                        .await
                        .unwrap();
                }
            });

            let mut i = 0u64;
            b.iter(|| {
                i += 1;
                let key = format!("scale:{}", i % num_keys);
                rt.block_on(async {
                    black_box(storage.get(&key).await)
                })
            })
        });
    }

    group.finish();
}

fn bench_concurrent_access(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_access");

    group.bench_function("increment_same_key", |b| {
        let storage = MemoryStorage::new();
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    storage
                        .increment("hotkey", 1, 1000, Duration::from_secs(3600))
                        .await,
                )
            })
        })
    });

    group.bench_function("increment_distributed_keys", |b| {
        let storage = MemoryStorage::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("dist:{}", i % 1000);
            rt.block_on(async {
                black_box(
                    storage
                        .increment(&key, 1, 1000, Duration::from_secs(3600))
                        .await,
                )
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_storage_operations, bench_storage_scaling, bench_concurrent_access);
criterion_main!(benches);
