//! Algorithm comparison example.
//!
//! Run with:
//! ```
//! cargo run -p oc_ratelimit_advanced --example algorithms --features "memory all-algorithms"
//! ```

use oc_ratelimit_advanced::{
    algorithm::{Algorithm, FixedWindow, SlidingWindow, TokenBucket},
    storage::MemoryStorage,
    LeakyBucket, Quota, SlidingLog, GCRA,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let quota = Quota::per_second(5).with_burst(3);

    println!("=== Algorithm Comparison Demo ===\n");
    println!("Quota: 5 requests/second, burst: 3\n");

    // Test each algorithm
    test_algorithm("GCRA", GCRA::new(), &quota).await?;
    test_algorithm("Token Bucket", TokenBucket::new(), &quota).await?;
    test_algorithm("Leaky Bucket", LeakyBucket::new(), &quota).await?;
    test_algorithm("Sliding Log", SlidingLog::new(), &quota).await?;
    test_algorithm("Sliding Window", SlidingWindow::new(), &quota).await?;
    test_algorithm("Fixed Window", FixedWindow::new(), &quota).await?;

    println!("\n=== Algorithm Characteristics ===\n");
    println!("| Algorithm      | Memory | Burst Handling | Best For                |");
    println!("|----------------|--------|----------------|-------------------------|");
    println!("| GCRA           | Low    | Excellent      | API rate limiting       |");
    println!("| Token Bucket   | Low    | Good           | Bursty traffic          |");
    println!("| Leaky Bucket   | Low    | Smooth         | Stable backend load     |");
    println!("| Sliding Log    | High   | Excellent      | Precision critical      |");
    println!("| Sliding Window | Medium | Good           | General purpose         |");
    println!("| Fixed Window   | Low    | Poor           | Simple use cases        |");

    Ok(())
}

async fn test_algorithm<A: Algorithm>(
    name: &str,
    algorithm: A,
    quota: &Quota,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = MemoryStorage::new();
    let key = format!("test:{}", name.to_lowercase().replace(' ', "_"));

    print!("{:15} | ", name);

    let mut results = Vec::new();
    for _ in 0..8 {
        let decision = algorithm.check_and_record(&storage, &key, quota).await?;
        results.push(if decision.is_allowed() { "✅" } else { "❌" });
    }

    println!("{}", results.join(" "));
    Ok(())
}
