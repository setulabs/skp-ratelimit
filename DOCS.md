# oc_ratelimit_advanced - Implementation Details

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Application Layer                          │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │  Axum Layer     │  │  Actix Wrap     │  │  RateLimitMgr   │  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘  │
└───────────┼─────────────────────┼─────────────────────┼─────────┘
            │                     │                     │
┌───────────┼─────────────────────┼─────────────────────┼─────────┐
│           ▼                     ▼                     ▼         │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Core Layer                              │ │
│  │  Decision │ Quota │ Key Extraction │ Policy │ Headers      │ │
│  └────────────────────────────────────────────────────────────┘ │
└───────────┬─────────────────────────────────────────────────────┘
            │
┌───────────┼─────────────────────────────────────────────────────┐
│           ▼                                                     │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                   Algorithm Layer                          │ │
│  │  GCRA │ Token Bucket │ Leaky Bucket │ Sliding │ Concurrent │ │
│  └────────────────────────────────────────────────────────────┘ │
└───────────┬─────────────────────────────────────────────────────┘
            │
┌───────────┼─────────────────────────────────────────────────────┐
│           ▼                                                     │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                   Storage Layer                            │ │
│  │            Memory + GC  │  Redis + Pool                    │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## Core Traits

### Algorithm Trait
```rust
pub trait Algorithm: Send + Sync + 'static {
    async fn check_and_record(&self, storage: &impl Storage, key: &str, quota: &Quota) -> Result<Decision>;
    async fn check(&self, storage: &impl Storage, key: &str, quota: &Quota) -> Result<Decision>;
}
```

### Storage Trait
```rust
pub trait Storage: Send + Sync + 'static {
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>>;
    async fn set(&self, key: &str, entry: StorageEntry, ttl: Duration) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn increment(&self, key: &str, delta: u64, window_start: u64, ttl: Duration) -> Result<u64>;
    async fn execute_atomic<F, T>(&self, key: &str, ttl: Duration, op: F) -> Result<T>;
    async fn compare_and_swap(&self, key: &str, expected: Option<&StorageEntry>, new: StorageEntry, ttl: Duration) -> Result<bool>;
}
```

### Key Trait
```rust
pub trait Key<R>: Send + Sync + 'static {
    fn extract(&self, request: &R) -> Option<String>;
    fn name(&self) -> &'static str;
}
```

### Policy Trait
```rust
pub trait Policy: Send + Sync + 'static {
    fn request_cost(&self, request_metadata: Option<&dyn std::any::Any>) -> u64;
    fn on_response(&self, status_code: u16, decision: &Decision) -> i64;
    fn name(&self) -> &'static str;
}
```

---

## Algorithm Details

| Algorithm | State Stored | Time Complexity | Space Complexity |
|-----------|--------------|-----------------|------------------|
| **GCRA** | TAT (timestamp) | O(1) | O(1) per key |
| Token Bucket | tokens, last_update | O(1) | O(1) per key |
| Leaky Bucket | water_level, last_update | O(1) | O(1) per key |
| Sliding Log | Vec<timestamp> | O(n) | O(n) per key |
| Sliding Window | prev_count, curr_count, window_start | O(1) | O(1) per key |
| Fixed Window | count, window_start | O(1) | O(1) per key |
| Concurrent | Set<active_keys> | O(1) | O(n) total |

---

## Storage Backends

### Memory Storage
- **Data Structure**: `DashMap<String, StorageEntry>` (concurrent hashmap)
- **GC Modes**: Request-based, time-based, manual
- **Thread Safety**: Lock-free reads, minimal write contention

### Redis Storage
- **Connection Pool**: `deadpool-redis` with configurable size
- **Key Format**: `{prefix}{key}` (default prefix: `rl:`)
- **Serialization**: JSON via serde
- **TTL**: Automatic per-key expiration

---

## Key Extractors

| Extractor | Extracts From | Output Format |
|-----------|---------------|---------------|
| `GlobalKey` | - | `"global"` |
| `StaticKey` | config | `"{value}"` |
| `IpKey` | X-Forwarded-For, X-Real-IP, peer | `"ip:{addr}"` |
| `PathKey` | Request path | `"path:{path}"` |
| `HeaderKey` | Specified header | `"header:{value}"` |
| `CompositeKey` | Two extractors | `"{key1}:{key2}"` |

---

## Middleware Integration

### Axum (Tower Layer)
```
Request → RateLimitLayer → RateLimitService → check_and_record() 
    ├─ Allowed → Inner Service → Add Headers → Response
    └─ Denied → 429 Response with Headers
```

### Actix-web (Transform)
```
Request → RateLimiter (Transform) → RateLimiterMiddleware (Service) 
    ├─ Allowed → Inner Service → Response
    └─ Denied → 429 InternalError
```

---

## HTTP Headers

| Header | Description | Example |
|--------|-------------|---------|
| `X-RateLimit-Limit` | Max requests per window | `100` |
| `X-RateLimit-Remaining` | Remaining in window | `45` |
| `X-RateLimit-Reset` | Seconds until reset | `30` |
| `X-RateLimit-Policy` | Algorithm name | `gcra` |
| `Retry-After` | Seconds to wait (on 429) | `10` |

---

## Feature Matrix

| Feature | Enables | Dependencies |
|---------|---------|--------------|
| `memory` | MemoryStorage, GcConfig | dashmap |
| `redis` | RedisStorage, RedisConfig | deadpool-redis |
| `axum` | RateLimitLayer | axum, tower, http |
| `actix` | RateLimiter | actix-web, actix-service |
| `gcra` | GCRA algorithm | - |
| `leaky-bucket` | LeakyBucket | - |
| `sliding-log` | SlidingLog | - |
| `concurrent` | ConcurrentLimiter | - |

---

## Extension Points

1. **Custom Algorithm**: Implement `Algorithm` trait
2. **Custom Storage**: Implement `Storage` trait (e.g., DynamoDB, Memcached)
3. **Custom Key Extractor**: Implement `Key<YourRequestType>` trait
4. **Custom Policy**: Implement `Policy` trait
5. **Custom Headers**: Use `RateLimitHeaders` builder

---

## File Structure

```
src/
├── lib.rs              # Re-exports, prelude
├── quota.rs            # Quota configuration
├── decision.rs         # Decision types, RateLimitInfo
├── error.rs            # Error types
├── policy.rs           # Policy trait + implementations
├── manager.rs          # RateLimitManager for per-route config
├── extensions.rs       # Request extensions for handlers
├── headers.rs          # HTTP header constants + builder
├── algorithm/
│   ├── mod.rs          # Algorithm trait
│   ├── gcra.rs         # Generic Cell Rate Algorithm
│   ├── token_bucket.rs
│   ├── leaky_bucket.rs
│   ├── sliding_log.rs
│   ├── sliding_window.rs
│   ├── fixed_window.rs
│   └── concurrent.rs   # Concurrent request limiter
├── storage/
│   ├── mod.rs          # Storage trait
│   ├── entry.rs        # StorageEntry struct
│   ├── memory_gc.rs    # Memory + garbage collection
│   └── redis_cluster.rs # Redis + connection pool
├── key/
│   ├── mod.rs          # Key trait, GlobalKey, StaticKey
│   ├── composite.rs    # CompositeKey, EitherKey
│   └── extractors.rs   # IpKey, PathKey, HeaderKey
└── middleware/
    ├── mod.rs
    ├── layer.rs        # Axum Tower Layer
    └── actix.rs        # Actix-web Transform
```
