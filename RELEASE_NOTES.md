
# 🚀 skp-ratelimit v0.1.2

**Relaxed Trait Bounds & Improved Integration**

## ✨ Features

### 🛠️ Integration Improvements
- **Relaxed Clone Bounds**: `RateLimitLayer` no longer requires the underlying storage to implement `Clone`. It handles `Arc` wrapping internally and implements `Clone` manually. This allows direct usage of simple storage types like `MemoryStorage` with Axum without complex workarounds.
- **Flexible Storage Types**: Added blanket `Storage` trait implementations for `Arc<S>` and `Box<S>`, allowing users to pass pre-wrapped storage instances or unique owners while maintaining trait compatibility.

---
# 🚀 skp-ratelimit v0.1.0

**Advanced, modular rate limiting library for Rust**

## ✨ Features

### 🔧 Rate Limiting Algorithms
- **GCRA** (Generic Cell Rate Algorithm) - Precise rate control with excellent burst handling
- **Token Bucket** - Classic algorithm for bursty traffic patterns
- **Leaky Bucket** - Smooth, constant output rate
- **Sliding Window** - General-purpose with good accuracy
- **Sliding Log** - High precision (higher memory usage)
- **Fixed Window** - Simple and fast
- **Concurrent Limiter** - Limit concurrent requests

### 💾 Storage Backends
- **In-Memory** with automatic garbage collection
- **Redis** with connection pooling (via `deadpool-redis`)

### 🌐 Framework Middleware
- **Axum** - Tower-based middleware layer
- **Actix-web** - Native middleware integration

### 🔑 Key Extractors
- IP address (with X-Forwarded-For support)
- Request path
- Headers (API keys, Authorization)
- Composite keys (combine multiple extractors)

### ⚙️ Additional Features
- **Per-route quotas** - Different limits for different endpoints
- **Policy system** - Penalty on errors, credit for cached responses
- **Flexible burst configuration** - Burst can be less than, equal to, or greater than max requests
- **Rate limit headers** - Standard X-RateLimit-* headers

## 📦 Installation

```toml
[dependencies]
skp-ratelimit = "0.1.0"
```

## 🚀 Quick Start

```rust
use skp_ratelimit::prelude::*;

let storage = MemoryStorage::new();
let algorithm = GCRA::new();
let quota = Quota::per_second(10).with_burst(15);

let decision = algorithm.check_and_record(&storage, "user:123", &quota).await?;

if decision.is_allowed() {
    println!("✓ Request allowed");
} else {
    println!("✗ Rate limited, retry after {:?}", decision.info().retry_after);
}
```

## 📊 Benchmarks

| Algorithm | Performance |
|-----------|-------------|
| Fixed Window | ~2.4M ops/sec |
| GCRA | ~1.5M ops/sec |
| Sliding Log | ~860K ops/sec |

## 📚 Documentation

- [README](https://github.com/setulabs/skp-ratelimit#readme)
- [Examples](https://github.com/setulabs/skp-ratelimit/tree/main/examples)

## 📄 License

MIT
