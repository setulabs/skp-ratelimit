//! Composite key example - rate limit by multiple factors.
//!
//! Run with:
//! ```
//! cargo run -p oc_ratelimit_advanced --example composite_keys --features memory
//! ```

use oc_ratelimit_advanced::{
    algorithm::Algorithm, key::{CompositeKey, Key}, storage::MemoryStorage, GCRA, Quota,
};

/// Simple mock request for demonstration
struct MockRequest {
    ip: String,
    path: String,
    user_id: Option<String>,
}

/// Custom key extractor for IP
struct IpExtractor;

impl oc_ratelimit_advanced::key::Key<MockRequest> for IpExtractor {
    fn extract(&self, request: &MockRequest) -> Option<String> {
        Some(format!("ip:{}", request.ip))
    }

    fn name(&self) -> &'static str {
        "ip"
    }
}

/// Custom key extractor for path
struct PathExtractor;

impl oc_ratelimit_advanced::key::Key<MockRequest> for PathExtractor {
    fn extract(&self, request: &MockRequest) -> Option<String> {
        Some(format!("path:{}", request.path))
    }

    fn name(&self) -> &'static str {
        "path"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();

    // Create composite key: IP + Path
    // This means rate limits are tracked per (IP, path) combination
    let key_extractor = CompositeKey::new(IpExtractor, PathExtractor);

    let quota = Quota::per_minute(5);

    println!("=== Composite Key Rate Limiting Demo ===\n");
    println!("Quota: 5 requests per minute per (IP + Path) combination\n");

    // Simulate requests from different IPs to different paths
    let scenarios = vec![
        ("192.168.1.1", "/api/users", 3),  // Same combo
        ("192.168.1.1", "/api/posts", 3),  // Same IP, different path
        ("192.168.1.2", "/api/users", 3),  // Different IP, same path
        ("192.168.1.1", "/api/users", 5),  // Back to first combo - should hit limit
    ];

    for (ip, path, count) in scenarios {
        let request = MockRequest {
            ip: ip.to_string(),
            path: path.to_string(),
            user_id: None,
        };

        let key = key_extractor.extract(&request).unwrap_or_default();
        println!("Requests from {} to {} (key: {}):", ip, path, key);

        for i in 1..=count {
            let decision = algorithm.check_and_record(&storage, &key, &quota).await?;

            if decision.is_allowed() {
                print!("  Request {}: ✅ ", i);
            } else {
                print!("  Request {}: ❌ ", i);
            }
            println!("(remaining: {})", decision.info().remaining);
        }
        println!();
    }

    Ok(())
}
