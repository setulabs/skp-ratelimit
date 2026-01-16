//! Basic GCRA rate limiting example.
//!
//! Run with:
//! ```
//! cargo run --example basic_gcra --features memory
//! ```

use skp_ratelimit::{
    Algorithm, MemoryStorage, GCRA, Quota,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory storage with garbage collection
    let storage = MemoryStorage::new();

    // Create GCRA algorithm
    let algorithm = GCRA::new();

    // Define quota: 10 requests per second with burst of 5
    let quota = Quota::per_second(10).with_burst(5);

    println!("=== Basic GCRA Rate Limiting Demo ===\n");
    println!("Quota: 10 requests/second, burst: 5\n");

    // Simulate burst of requests
    for i in 1..=15 {
        let decision = algorithm
            .check_and_record(&storage, "user:123", &quota)
            .await?;

        if decision.is_allowed() {
            println!(
                "Request {}: ✅ Allowed (remaining: {})",
                i,
                decision.info().remaining
            );
        } else {
            println!(
                "Request {}: ❌ Denied (retry after: {:?})",
                i,
                decision.info().retry_after
            );
        }
    }

    println!("\n--- Waiting 1 second for recovery ---\n");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Try again after waiting
    let decision = algorithm
        .check_and_record(&storage, "user:123", &quota)
        .await?;

    println!(
        "After recovery: {} (remaining: {})",
        if decision.is_allowed() {
            "✅ Allowed"
        } else {
            "❌ Denied"
        },
        decision.info().remaining
    );

    // Show HTTP headers
    println!("\n--- Rate Limit Headers ---");
    for (name, value) in decision.info().to_headers() {
        println!("{}: {}", name, value);
    }

    Ok(())
}
