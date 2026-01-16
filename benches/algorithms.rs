//! Benchmarks for rate limiting algorithms.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use skp_ratelimit::{
    algorithm::{Algorithm, FixedWindow, SlidingWindow, TokenBucket},
    storage::MemoryStorage,
    LeakyBucket, Quota, SlidingLog, GCRA,
};
use tokio::runtime::Runtime;

fn bench_algorithms(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let quota = Quota::per_second(1000).with_burst(100);

    let mut group = c.benchmark_group("algorithms");

    // GCRA
    group.bench_function("gcra", |b| {
        let storage = MemoryStorage::new();
        let algorithm = GCRA::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("gcra:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    // Token Bucket
    group.bench_function("token_bucket", |b| {
        let storage = MemoryStorage::new();
        let algorithm = TokenBucket::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("token:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    // Fixed Window
    group.bench_function("fixed_window", |b| {
        let storage = MemoryStorage::new();
        let algorithm = FixedWindow::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("fixed:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    // Sliding Window
    group.bench_function("sliding_window", |b| {
        let storage = MemoryStorage::new();
        let algorithm = SlidingWindow::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("sliding:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    // Leaky Bucket
    group.bench_function("leaky_bucket", |b| {
        let storage = MemoryStorage::new();
        let algorithm = LeakyBucket::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("leaky:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    // Sliding Log
    group.bench_function("sliding_log", |b| {
        let storage = MemoryStorage::new();
        let algorithm = SlidingLog::new();
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            let key = format!("log:{}", i % 100);
            rt.block_on(async {
                black_box(algorithm.check_and_record(&storage, &key, &quota).await)
            })
        })
    });

    group.finish();
}

fn bench_algorithm_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let quota = Quota::per_second(10000).with_burst(100);
    
    let mut group = c.benchmark_group("algorithm_comparison");
    
    for num_keys in [1, 10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("gcra", num_keys), num_keys, |b, &num_keys| {
            let storage = MemoryStorage::new();
            let algorithm = GCRA::new();
            let mut i = 0u64;
            b.iter(|| {
                i += 1;
                let key = format!("k:{}", i % num_keys);
                rt.block_on(async {
                    black_box(algorithm.check_and_record(&storage, &key, &quota).await)
                })
            })
        });
        
        group.bench_with_input(BenchmarkId::new("fixed_window", num_keys), num_keys, |b, &num_keys| {
            let storage = MemoryStorage::new();
            let algorithm = FixedWindow::new();
            let mut i = 0u64;
            b.iter(|| {
                i += 1;
                let key = format!("k:{}", i % num_keys);
                rt.block_on(async {
                    black_box(algorithm.check_and_record(&storage, &key, &quota).await)
                })
            })
        });
    }
    
    group.finish();
}

criterion_group!(benches, bench_algorithms, bench_algorithm_comparison);
criterion_main!(benches);
