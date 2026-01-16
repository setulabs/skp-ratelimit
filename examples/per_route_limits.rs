//! Per-route rate limiting example.
//!
//! Run with:
//! ```
//! cargo run --example per_route_limits --features memory
//! ```

use skp_ratelimit::{Algorithm, MemoryStorage, GCRA, Quota};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();

    // Define per-route quotas
    let mut route_quotas = HashMap::new();
    route_quotas.insert("/api/search", Quota::per_minute(30));
    route_quotas.insert("/api/auth/login", Quota::per_minute(5));
    route_quotas.insert("/api/users", Quota::per_second(20));

    let default_quota = Quota::per_minute(100);

    println!("=== Per-Route Rate Limiting Demo ===\n");

    // Simulate requests to different routes
    let routes = vec![
        ("/api/data", 5),       // Uses default quota (100/min)
        ("/api/search", 35),    // 30/min limit - should deny some
        ("/api/auth/login", 7), // 5/min limit - should deny some
        ("/api/users", 25),     // 20/sec limit
    ];

    for (route, count) in routes {
        let quota = route_quotas.get(route).unwrap_or(&default_quota);
        println!("Route: {} (quota: {}/{}s, sending {} requests)", 
            route, quota.max_requests(), quota.window().as_secs(), count);

        let mut allowed = 0;
        let mut denied = 0;

        for _ in 0..count {
            let key = format!("route:{}", route);
            let decision = algorithm.check_and_record(&storage, &key, quota).await?;

            if decision.is_allowed() {
                allowed += 1;
            } else {
                denied += 1;
            }
        }

        println!("  ✅ Allowed: {}, ❌ Denied: {}\n", allowed, denied);
    }

    Ok(())
}

