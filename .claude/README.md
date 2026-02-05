# FraiseQL Development Documentation

**Last Updated**: January 30, 2026

This directory contains development documentation for FraiseQL v2 contributors.

---

## 📚 Documentation Index

### Start Here

| Document | Purpose | When to Read |
|----------|---------|--------------|
| **[CLAUDE.md](CLAUDE.md)** | Development workflow and standards | Starting development |
| **[ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)** | Architectural patterns and principles | Understanding the codebase |

### Historical Context

| Document | Purpose | Status |
|----------|---------|--------|
| Various `PHASE_*.md` files | Historical phase implementation tracking | ✅ Complete (archived) |
| Various `CYCLE_*.md` files | TDD cycle summaries | ✅ Complete (archived) |

These historical files document the development journey but are not needed for current development.

---

## 🎯 Quick Navigation

### I want to...

**...understand the architecture**
→ Read [ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)

**...start contributing**
→ Read [CLAUDE.md](CLAUDE.md) development workflow section

**...add a new feature**
→ See "Adding New Features" in [ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)

**...fix a bug**
→ See "Common Tasks > Fix a Bug" in [CLAUDE.md](CLAUDE.md)

**...understand testing**
→ See "Testing Strategy" in [ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)

**...learn about security**
→ See "Security Model" in [ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)

**...understand the observer system**
→ See `../crates/fraiseql-observers/README.md` (if exists) or check the crate

**...understand endpoint runtime features**
→ See `../docs/endpoint-runtime/ARCHITECTURE_UPDATE.md` for historical context

---

## 🏗️ Architecture at a Glance

### Layered Optionality Pattern

```
fraiseql-core/          Layer 1: Pure GraphQL engine (required)
    ↓
fraiseql-server/        Layer 2: HTTP server Server<DatabaseAdapter> (required)
    ↓
fraiseql-observers/     Layer 3: Optional extensions via #[cfg(feature = "...")]
fraiseql-arrow/
fraiseql-wire/
```

### Five Core Principles

1. **Compilation Boundary** - Schema compiled at build time, not runtime
2. **Trait-Based Adapters** - Every external dependency is mockable
3. **Feature-Gated Extensions** - Opt-in complexity via Cargo features
4. **Config-Driven Runtime** - All behavior via TOML, not code changes
5. **Arc-Shared State** - Zero-copy concurrency patterns

---

## 🔧 Development Workflow

```bash
# 1. Create feature branch
git checkout -b feature/my-feature

# 2. Make changes following architectural principles

# 3. Verify
cargo check
cargo clippy --all-targets --all-features
cargo test

# 4. Commit with clear message
git commit -m "feat(scope): Description

## Changes

- Change 1
- Change 2

## Verification
✅ Tests pass
✅ Clippy clean"
```

---

## 📖 Key Concepts

### Generic Server Pattern

```rust
// Server is generic over database adapter
pub struct Server<A: DatabaseAdapter> {
    config: ServerConfig,
    executor: Arc<Executor<A>>,
    // ...
}
```

**Why:** Type-safe database swapping, easy testing with mocks

### Optional Features

```rust
// Features are opt-in via #[cfg]
#[cfg(feature = "observers")]
observer_runtime: Option<Arc<RwLock<ObserverRuntime>>>
```

**Why:** Users only compile what they need

### Trait-Based Dependencies

```rust
// All external deps behind traits
pub trait DatabaseAdapter: Send + Sync {
    async fn execute_where_query(...) -> Result<Vec<JsonbValue>>;
}
```

**Why:** Mockable, testable, swappable

---

## 📊 Project Status

**Architecture**: Production-ready, layered optionality
**Tests**: 294+ passing
**Lines of Code**: ~140,000 across workspace
**Crates**: 9 (core, server, observers, arrow, wire, cli, error, observers-macros)

### Feature Completeness

| Feature | Status | Location |
|---------|--------|----------|
| GraphQL Execution | ✅ Complete | fraiseql-core |
| HTTP Server | ✅ Complete | fraiseql-server |
| Multi-Database | ✅ Complete | fraiseql-core/db |
| Webhooks | ✅ Complete | fraiseql-server/webhooks |
| File Uploads | ✅ Complete | fraiseql-server/files |
| Authentication | ✅ Complete | fraiseql-server/auth |
| Observers | ✅ Complete | fraiseql-observers |
| Rate Limiting | ✅ Complete | fraiseql-server/middleware |
| Metrics | ✅ Complete | fraiseql-server/observability |
| Arrow Flight | ✅ Complete | fraiseql-arrow |
| Federation | ✅ In Progress | fraiseql-server/federation |

---

## 🚀 Getting Started (New Contributors)

### 5-Minute Onboarding

1. **Read this file** (you are here!)
2. **Read [ARCHITECTURE_PRINCIPLES.md](ARCHITECTURE_PRINCIPLES.md)** (~15 minutes)
3. **Read [CLAUDE.md](CLAUDE.md) development workflow** (~10 minutes)
4. **Browse `../crates/fraiseql-server/src/server.rs`** (see the actual implementation)
5. **Run the tests**: `cargo test --all-features`

### First Contribution Ideas

- Add a new signature verification provider (see `fraiseql-server/src/webhooks/signature/`)
- Add a new OAuth provider (see `fraiseql-server/src/auth/providers/`)
- Add tests for edge cases
- Improve documentation
- Fix a bug from issues

---

## 🎯 Architecture Decisions (Quick Reference)

### Why Generic `Server<A>` Instead of Concrete Type?
✅ Type safety, easy testing, swappable databases

### Why Separate fraiseql-observers Crate?
✅ Large feature (9K LOC), many dependencies, can be disabled

### Why Remove RuntimeServer?
✅ Dead code, maintaining two servers was confusing, Server<A> does everything

### Why Feature Flags?
✅ Users only compile what they need, reduces binary size

### Why Trait-Based Design?
✅ Mockable dependencies, easy testing, clear contracts

---

## 📝 Documentation Structure

```
.claude/
├── README.md                           # ← You are here
├── ARCHITECTURE_PRINCIPLES.md          # Core architectural guide
├── CLAUDE.md                           # Development workflow
└── [Various historical PHASE_*.md]     # Historical tracking (archived)

../docs/
├── endpoint-runtime/
│   ├── ARCHITECTURE_UPDATE.md          # Architecture evolution explanation
│   └── [Various phase docs]            # Historical planning documents
├── auth/                               # Authentication guides
├── architecture/                       # Architecture docs
└── guides/                             # User guides

../crates/
├── fraiseql-core/                      # Core implementation
├── fraiseql-server/                    # Server implementation
└── fraiseql-observers/                 # Observer system
```

---

## 🤝 Contributing

1. **Follow the architecture principles** in ARCHITECTURE_PRINCIPLES.md
2. **Write tests** for all new features
3. **Document** public APIs with rustdoc comments
4. **Use traits** for external dependencies
5. **Feature gate** large optional subsystems
6. **Keep it simple** - avoid over-engineering

---

## ❓ Common Questions

**Q: Where do I add a new GraphQL feature?**
A: Core logic in `fraiseql-core/`, server integration in `fraiseql-server/`

**Q: How do I make a feature optional?**
A: Use Cargo feature flags + `#[cfg(feature = "...")]`

**Q: What's the difference between fraiseql-server and fraiseql-core?**
A: Core = GraphQL execution engine. Server = HTTP wrapper + optional features.

**Q: Why so many archived documents?**
A: Historical tracking. Only read ARCHITECTURE_PRINCIPLES.md and CLAUDE.md for current work.

**Q: What happened to RuntimeServer?**
A: Removed. Server<A> is the only server implementation now (simpler, clearer).

**Q: How do I test database code?**
A: Use mock implementations of DatabaseAdapter trait (see tests for examples).

---

## 🔗 External Resources

- **Main README**: `../README.md`
- **Code**: `../crates/`
- **Docs**: `../docs/`
- **Examples**: `../examples/`

---

## 📧 Help & Support

- **Questions about architecture**: Read ARCHITECTURE_PRINCIPLES.md
- **Questions about workflow**: Read CLAUDE.md
- **Bug reports**: GitHub issues
- **Feature requests**: GitHub discussions

---

**Remember**: Start with ARCHITECTURE_PRINCIPLES.md and CLAUDE.md. Everything else is either historical context or reference material.

---

**Last Major Update**: January 30, 2026 (Architectural consolidation)
**Architecture Status**: Production-ready, layered optionality
**Documentation Status**: Complete and current
