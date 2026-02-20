<!-- Skip to main content -->
---

title: FraiseQL v2.0.0-alpha.1 - Known Limitations
description: This document outlines what's **not** included in the alpha release and when it's expected.
keywords: []
tags: ["documentation", "reference"]
---

# FraiseQL v2.0.0-alpha.1 - Known Limitations

**Version**: 2.0.0-alpha.1
**Last Updated**: February 3, 2026

This document outlines what's **not** included in the alpha release and when it's expected.

---

## 🎯 What IS Ready for Alpha Testing

These features are **fully implemented and tested**:

- ✅ Core GraphQL execution (queries, mutations ready; subscriptions planned for v2.1)
- ✅ Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)
- ✅ Schema compilation and validation
- ✅ Apollo Federation v2 with SAGA transactions
- ✅ OAuth2/OIDC authentication (Google, GitHub, Auth0, Keycloak, Generic)
- ✅ Field-level access control and row-level security
- ✅ Audit logging and compliance features
- ✅ Rate limiting and brute-force protection
- ✅ Change Data Capture (CDC) with event streaming
- ✅ Webhooks and event system (11 provider signatures)
- ✅ Query result caching with invalidation
- ✅ Apache Arrow Flight columnar export
- ✅ FraiseQL-wire streaming (PostgreSQL)
- ✅ 2,400+ tests with 100% pass rate

---

## ⚠️ Known Limitations (Alpha Phase)

### Language Support

**All 16 languages ready for alpha testing:**

**JVM Ecosystem:**

- ✅ Java
- ✅ Kotlin
- ✅ Scala
- ✅ Clojure
- ✅ Groovy

**Dynamic Languages:**

- ✅ Python
- ✅ TypeScript/Node.js
- ✅ Ruby
- ✅ PHP
- ✅ Elixir

**Compiled/Static Languages:**

- ✅ Go
- ✅ Rust
- ✅ C#
- ✅ Swift
- ✅ Dart

**Timeline**: All languages available in v2.0.0-alpha.1 release.

---

### Performance Tuning (Non-Blocking)

Some optimizations identified during Phase 10 hardening are deferred to v2.1+:

- Arrow Flight schema pre-loading optimizations
- Chrono DateTime parsing improvements
- Zero-copy conversion enhancements
- P95 latency optimization (currently ~145ms, target <100ms)

**Impact**: Negligible for typical queries. Optimization is not critical path for alpha testing.

**Timeline**: v2.1.0+ (post-GA).

---

### Breaking Changes from v1

FraiseQL v2 is a **complete architectural redesign** and is **not backwards compatible** with v1.

**Key differences:**

- **Compiled execution** (v1 was interpreted)
- **Database-centric design** (v1 was GraphQL-centric)
- **TOML configuration** (v1 used environment variables)
- **New schema conventions** (tb_*, v_*, fn_* patterns)
- **Different schema format** (v1 schemas won't work in v2)

**Migration Path**:

- Currently: Manual schema rewrite required
- Beta/GA: Migration guide will be provided

**Workaround**: Treat v2 as a fresh start. If you're running v1 in production, keep it running—v1 and v2 can coexist.

**Timeline**: Migration guide coming in beta (March 2026).

---

### Advanced Features (Deferred to Later Releases)

These features are **not in alpha** but are planned:

#### Subscriptions/Real-time Updates

- **Current**: CDC (Change Data Capture) provides event streaming
- **Planned**: WebSocket subscriptions with real-time query results
- **Timeline**: v2.1+ (post-GA)
- **Workaround**: Use CDC events to build real-time systems

#### Additional GraphQL Directives

- **Current**: `@auth` and `@cache` directives
- **Planned**: Additional directives for customization (v2.1+)
- **Timeline**: v2.1+ (post-GA)
- **Workaround**: Field-level auth covers most use cases

#### Oracle Database Support

- **Status**: Not planned (no Rust driver available)
- **Supported**: PostgreSQL, MySQL, SQLite, SQL Server
- **Workaround**: Migrate to supported database or use database gateway

#### Additional Webhook Providers

- **Current**: 11 providers (Discord, GitHub, GitLab, Slack, Stripe, Twilio, SendGrid, Mailgun, PagerDuty, Datadog, Custom)
- **Planned**: More providers in v2.1+ (based on feedback)
- **Workaround**: Use custom webhook provider

---

## 🐛 Known Issues (Non-Blocking)

### Resolved Issues

1. **Phase Documentation in Codebase**
   - `.phases/` directory has been removed (v1.9.18+)
   - Development artifacts cleaned up for release

2. **Documentation Updates**
   - Some documentation references may be outdated
   - Core functionality docs are current
   - Report any inaccuracies via GitHub Issues

3. **Database-Specific Edge Cases**
   - Some SQL Server-specific optimizations pending
   - Basic functionality works; full optimization in v2.1+

---

## ✅ What to Test in Alpha

Focus your alpha testing on:

### Critical Path

- [ ] Schema compilation with your language (Python/TypeScript/Go/PHP)
- [ ] Query execution on your database (PostgreSQL/MySQL/SQLite/SQL Server)
- [ ] Filtering and pagination
- [ ] Mutations (INSERT/UPDATE)
- [ ] Error handling

### Important Features

- [ ] Authentication flows (OAuth2/OIDC)
- [ ] Field-level access control
- [ ] Rate limiting
- [ ] Caching behavior
- [ ] Federation (if multi-service)

### Integration Features

- [ ] Webhooks
- [ ] CDC event streaming
- [ ] Arrow Flight export
- [ ] Docker and Kubernetes deployment

See [Alpha Testing Guide](alpha-testing-guide.md) for detailed testing checklist.

---

## 📈 Performance Expectations

**This is alpha, not final performance tuning.**

Expected performance characteristics:

| Metric | Alpha Target | GA Target | Notes |
|--------|--------------|-----------|-------|
| Row throughput | 100k+/sec | 200k+/sec | Achieved 498M/sec in bench |
| Query latency (P95) | <200ms | <100ms | Currently ~145ms for typical |
| Memory efficiency | Bounded | Optimized | Streaming prevents buffering |
| Arrow vs JSON | 25-40x faster | 25-40x faster | Achieved consistently |

**Alpha testing focus**: Correctness over micro-optimization. Performance tuning is post-GA work.

---

## 🔄 Migration Timeline

| Phase | When | What |
|-------|------|------|
| **Alpha** | Feb 2026 (now) | Initial release, gather feedback |
| **Beta** | Mar 2026 | Address feedback, performance tune |
| **GA** | Apr 2026 | Stable release, long-term support |
| **v2.1+** | May 2026+ | Additional languages, optimizations, new providers |

---

## 💬 Reporting Limitations

If you encounter an issue not listed here:

1. Check [alpha-testing-guide.md](alpha-testing-guide.md) for known workarounds
2. Open a [GitHub Issue](https://github.com/FraiseQL/FraiseQL/issues) with:
   - Description of the limitation
   - Environment (database, language, OS)
   - Steps to reproduce
   - Tag with `alpha` label

---

## 📚 Additional Resources

- **[Alpha Testing Guide](alpha-testing-guide.md)** — How to test effectively
- **[FAQ](faq.md)** — General questions
- **[TROUBLESHOOTING](troubleshooting.md)** — Common problems
- **[Release Notes](../ALPHA_RELEASE_NOTES.md)** — What's included in alpha
- **[GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues)** — Report problems

---

## 🙏 Alpha Testing Thank You

Your feedback on these limitations helps us prioritize development and ensures v2.0.0 GA is solid.

**Thank you for testing FraiseQL v2 alpha!**
