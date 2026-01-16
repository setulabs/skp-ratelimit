# oc_ratelimit_advanced - Roadmap

## ✅ Completed (v0.1.0)

- [x] Core traits (Algorithm, Storage, Key, Policy)
- [x] 7 Algorithms (GCRA, Token Bucket, Leaky Bucket, Sliding Log/Window, Fixed Window, Concurrent)
- [x] Memory storage with garbage collection
- [x] Redis storage with connection pooling
- [x] Axum middleware (Tower Layer)
- [x] Actix-web middleware
- [x] Key extractors (IP, Path, Header, Composite)
- [x] Per-route configuration (RateLimitManager)
- [x] Policy system (Penalty, Credit, Composite)
- [x] HTTP headers support
- [x] Examples and documentation

---

## 🔥 High Priority

| Feature | Description | Status |
|---------|-------------|--------|
| Rate Limit Bypass | Allow bypass for admin/internal IPs | Planned |
| Metrics/Telemetry | Prometheus metrics (requests, denials, latency) | Planned |
| Redis Cluster | Multi-node Redis for high availability | Planned |
| Lua Scripts | Atomic Redis operations for true distributed consistency | Planned |

---

## 🎯 Medium Priority

| Feature | Description | Status |
|---------|-------------|--------|
| Dynamic Quotas | Change quotas at runtime via API | Planned |
| Quota Inheritance | Child routes inherit parent quotas | Planned |
| User-Tier Limits | Different tiers (free: 100/hr, pro: 1000/hr) | Planned |
| Circuit Breaker | Temporarily block after repeated violations | Planned |
| Warm-up Period | Gradual quota increase for new clients | Planned |
| Integration Tests | Full middleware tests with mock servers | Planned |
| Benchmarks | Criterion benchmarks for algorithms | Planned |

---

## 💡 Nice to Have

| Feature | Description | Status |
|---------|-------------|--------|
| Webhook on Limit | Notify external system when limit reached | Planned |
| Request Queuing | Queue requests instead of rejecting | Planned |
| Adaptive Limiting | Auto-adjust limits based on backend health | Planned |
| Response Caching | Cache 304 responses to reduce load | Planned |
| Database Quotas | Load quotas from external database | Planned |
| Distributed Sync | Sync limits across nodes without Redis | Planned |

---

## 🔧 Code Quality

| Improvement | Description | Status |
|-------------|-------------|--------|
| Fix warnings | Address dead_code warnings in composite.rs | Planned |
| Rustdoc | More comprehensive documentation | Planned |
| CI Pipeline | GitHub Actions for testing | Planned |
| Code coverage | Add coverage reporting | Planned |

---

## Version Roadmap

### v0.2.0
- [ ] Metrics/Prometheus support
- [ ] Redis Lua scripts for atomicity
- [ ] User-tier based quotas
- [ ] Fix warnings

### v0.3.0
- [ ] Redis Cluster support
- [ ] Rate limit bypass
- [ ] Circuit breaker
- [ ] Integration tests

### v1.0.0
- [ ] Stable API
- [ ] Full documentation
- [ ] Production-ready
