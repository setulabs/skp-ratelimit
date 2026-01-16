# skp-ratelimit

Advanced, modular rate limiting library for Rust with multiple algorithms, per-route quotas, and framework middleware.

## Features

- **7 Algorithms**: GCRA, Token Bucket, Leaky Bucket, Sliding Log, Sliding Window, Fixed Window, Concurrent
- **2 Storage Backends**: Memory with GC, Redis with connection pooling
- **2 Framework Middleware**: Axum (Tower), Actix-web
- **Key Extractors**: IP, Path, Header, Composite keys
- **Per-Route Quotas**: Different limits for different endpoints
- **Policy System**: Penalty on errors, credit for cached responses

## Algorithm Comparison

| Algorithm | Best For | Memory | Burst Handling |
|-----------|----------|--------|----------------|
| **GCRA** | Precise rate control | Low | Excellent |
| Token Bucket | Bursty traffic | Low | Excellent |
| Leaky Bucket | Smooth output | Low | None |
| Sliding Log | Precision critical | High | Good |
| Sliding Window | General purpose | Low | Good |
| Fixed Window | Simple use cases | Low | Poor |

## Quick Start

```rust
use skp_ratelimit::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();
    let quota = Quota::per_second(10).with_burst(15);

    let decision = algorithm.check_and_record(&storage, "user:123", &quota).await?;

    if decision.is_allowed() {
        println!("✓ Allowed! {} remaining", decision.info().remaining);
    } else {
        println!("✗ Denied! Retry after {:?}", decision.info().retry_after);
    }
    Ok(())
}
```

## Quota vs Burst

A **Quota** consists of two key parameters:

| Parameter | Description |
|-----------|-------------|
| `max_requests` | Sustained rate - requests allowed per time window |
| `burst` | Instant capacity - requests allowed in quick succession |

### Burst Configuration Options

```rust
// Default: burst = max_requests (10 burst, 10/sec rate)
Quota::per_second(10)

// Strict burst: limits instant requests while allowing higher sustained rate
Quota::per_second(10).with_burst(5)   // Only 5 instant, 10/sec rate

// Generous burst: allows traffic spikes, borrows from future quota
Quota::per_second(10).with_burst(50)  // 50 instant, 10/sec rate
```

### When to Use Each

| Scenario | Configuration | Reason |
|----------|---------------|--------|
| API protection | `burst < max_requests` | Prevent thundering herd |
| User-facing apps | `burst > max_requests` | Better UX, forgiving spikes |
| General use | `burst = max_requests` | Balanced (default) |

### Example Flow

```text
Quota::per_second(10).with_burst(3)

Time 0s:   [###.......] 3 tokens available (burst limit)
Request 1: [##........] ✅ Allowed, 2 remaining
Request 2: [#.........] ✅ Allowed, 1 remaining  
Request 3: [..........] ✅ Allowed, 0 remaining
Request 4: [..........] ❌ DENIED (wait for refill)
~100ms later: 1 token refills (10/sec = 1 per 100ms)
Request 5: [..........] ✅ Allowed
```

## Per-Route Rate Limiting

```rust
use skp_ratelimit::{RateLimitManager, GCRA, Quota, key::GlobalKey};

let manager = RateLimitManager::builder()
    .default_quota(Quota::per_minute(100))
    .route("/api/search", Quota::per_minute(30))
    .route("/api/auth/login", Quota::per_minute(5))
    .route_pattern("/api/users/*", Quota::per_second(20))
    .build_with_key(GCRA::new(), MemoryStorage::new(), GlobalKey::new());

let decision = manager.check_and_record("/api/search", &request).await?;
```

## Axum Middleware

```rust
use axum::{Router, routing::get};
use skp_ratelimit::{middleware::RateLimitLayer, GCRA, Quota, MemoryStorage, key::GlobalKey};

let app = Router::new()
    .route("/api/data", get(handler))
    .layer(RateLimitLayer::new(
        MemoryStorage::new(),
        GCRA::new(),
        Quota::per_second(10),
        GlobalKey::new(),
    ));
```

## Actix-web Middleware

```rust
use actix_web::{web, App, HttpServer};
use skp_ratelimit::{middleware::actix::RateLimiter, GCRA, Quota, MemoryStorage};

HttpServer::new(|| {
    App::new()
        .wrap(RateLimiter::new(MemoryStorage::new(), GCRA::new(), Quota::per_second(10)))
        .route("/api", web::get().to(handler))
})
```

## Redis Storage

```rust
use skp_ratelimit::storage::{RedisStorage, RedisConfig};

let config = RedisConfig::new("redis://localhost:6379")
    .with_prefix("myapp:rl:")
    .with_pool_size(20);

let storage = RedisStorage::new(config).await?;
```

## Composite Keys

Rate limit by multiple factors (IP + Path, User + API Key):

```rust
use skp_ratelimit::key::CompositeKey;

let key = CompositeKey::new(IpKey::new(), PathKey::new());
// Generates keys like "192.168.1.1:/api/users"
```

## Policy System

```rust
use skp_ratelimit::policy::{PenaltyPolicy, CreditPolicy, CompositePolicy};

// Consume extra tokens on errors, refund on 304 Not Modified
let policy = CompositePolicy::new()
    .with(PenaltyPolicy::new(2))  // 2x cost on 4xx/5xx
    .with(CreditPolicy::new());    // Refund on 304
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `memory` | In-memory storage with GC | ✓ |
| `redis` | Redis storage with pooling | |
| `axum` | Axum middleware | |
| `actix` | Actix-web middleware | |
| `gcra` | GCRA algorithm | ✓ |
| `leaky-bucket` | Leaky bucket algorithm | ✓ |
| `sliding-log` | Sliding log algorithm | ✓ |
| `concurrent` | Concurrent limiter | ✓ |
| `full` | All features | |

## Examples

```bash
cargo run --example basic_gcra --features memory
cargo run --example per_route_limits --features memory
cargo run --example composite_keys --features memory
cargo run --example algorithms --features "memory all-algorithms"
```

## Benchmarks

```bash
cargo bench --features "memory all-algorithms"
```

## License

MIT
