# FraiseQL v2.0: Multi-Framework HTTP Strategy

**Date**: January 8, 2026
**Status**: Final Architecture Plan
**Approach**: Balanced - Rust for performance, Python for compatibility

---

## The Strategy

**Axum / FastAPI / Starlette** - Three focused HTTP servers.

A pragmatic approach that:
- ✅ Keeps Python servers (FastAPI, Starlette) fully supported
- ✅ Adds native Rust server (Axum) for performance
- ✅ Allows teams to choose their path
- ✅ Enables gradual migration from Python to Rust

---

## Architecture

```
v2.0 Server Options:

Rust Servers (High Performance):
  🟢 Axum (Modern, recommended default)

Python Servers (Backward Compatible):
  🟢 FastAPI (Same as v1.8.x, fully supported)
  🟢 Starlette (Lightweight, restored support)

All share:
  - Same modular middleware
  - Same GraphQL execution engine
  - Same configuration options
```

---

## For Each User Type

### v1.8.x FastAPI Users

**Options**:

1. **Upgrade to v2.0 with FastAPI** (zero friction)
   - Same code, same performance
   - Optional: Migrate to Axum later when ready

2. **Upgrade directly to v2.0 with Axum** (maximum performance)
   - 7-10x faster immediately
   - Slightly different configuration (Rust)

3. **Stay on v1.8.x** (always an option)

**Recommendation**: Start with FastAPI, plan Rust migration.

### v1.8.x Starlette Users

**Options**:

1. **Upgrade to v2.0 with Starlette** (compatible)
   - Starlette support fully restored
   - Same behavior as v1.8.x

2. **Upgrade to v2.0 with Axum** (when ready for performance)
   - Significantly faster
   - Modern Rust stack

**Recommendation**: Use Starlette in v2.0, migrate to Axum for performance.

### New v2.0 Applications

**Recommendation**: Start with Axum
- Best performance (7-10x faster than Python)
- Modern async Rust ecosystem
- Recommended by FraiseQL team

**If Python required**: Use FastAPI
- Same as v1.8.x
- Large ecosystem
- Easy to migrate to Axum later

---

## Key Benefits of This Approach

### 1. Zero Breaking Changes
```
v1.8.x FastAPI user upgrades to v2.0 FastAPI
  → Exactly the same code
  → No changes needed
  → Just get benefits (middleware, security updates)
```

### 2. Performance Available When Ready
```
v2.0 FastAPI → v2.0 Axum (when team ready)
  → Same GraphQL behavior
  → Just switch server implementation
  → 7-10x performance improvement
```

### 3. Clear Framework Choice
```
- Team prefers Python? → FastAPI/Starlette
- Performance critical? → Axum
- Existing framework? → Custom adapter
```

### 4. Single Middleware System
```
All servers use same middleware:
  - Auth, RBAC, Caching, Rate Limiting
  - Logging, Tracing, CORS, CSRF
  - User-defined custom middleware

No differences between server choices
```

---

## Server Comparison

| Aspect | Axum | FastAPI | Starlette |
|--------|------|---------|-----------|
| **Speed** | 7-10x* | 1x baseline | 1x baseline |
| **Language** | Rust | Python | Python |
| **Maturity** | Growing | Mature | Mature |
| **Best For** | New apps, performance | Python teams | Minimal Python |
| **Learning** | Moderate | Low | Low |
| **Status v2.0** | Primary Rust option | Fully supported | Fully supported |
| **Cost** | Free | Free | Free |

*Compared to Python servers

---

## Migration Paths

### Path 1: Python → Rust (Immediate)
```
v1.8.x FastAPI → v2.0 Axum
  Week 1: Understand Axum basics
  Week 2: Migrate configuration
  Week 3: Test, deploy
  Result: 7-10x performance boost
```

### Path 2: Python → Rust (Gradual)
```
v1.8.x FastAPI
  ↓
v2.0 FastAPI (maintain compatibility)
  ↓
v2.0 Axum (when team ready, performance boost)
```

### Path 3: Python Only
```
v1.8.x FastAPI
  ↓
v2.0 FastAPI
  ↓
(continue with Python, always an option)
```

### Path 4: Custom Framework
```
v2.0 with Custom HTTP Adapter
  (implement adapter for your framework)
```

---

## Timeline

### v2.0.0 Release
- ✅ **All servers supported**: Axum, FastAPI, Starlette
- ✅ **Full feature parity**: All features work on all servers
- ✅ **Same middleware**: Shared across all options
- ✅ **Backward compatible**: FastAPI/Starlette work exactly like v1.8.x

### v2.1.0+ (Future)
- 🟡 **Rust servers emphasized**: Primary focus
- 🟡 **Python servers maintained**: Bug fixes, critical updates
- 🟡 **New features**: Prioritized for Rust servers

### v3.0.0+ (Future)
- ⚠️ **Python servers**: May be moved to archive (depends on adoption)
- ⚠️ **Rust servers**: Primary support
- ⚠️ **Compatibility**: Always possible via custom adapters

---

## Implementation Strategy

### Phase 1: Refactor (Weeks 6-10)
Create modular HTTP core:
- Framework-agnostic router, handler traits
- Middleware pipeline system
- Response building, error handling

### Phase 2: Rust Adapter (Weeks 6-10)
Implement Rust server:
- Axum adapter (focused, modern)

### Phase 3: Python Adapters (Weeks 11-14)
Maintain Python servers:
- FastAPI adapter (keep current behavior)
- Starlette adapter (restore support)

### Phase 4: Middleware (Weeks 11-14)
Shared middleware system:
- Auth, RBAC, Caching, Rate Limiting
- Logging, Tracing, CORS, CSRF
- Custom middleware support

### Phase 5: Testing (Weeks 15-16)
Comprehensive testing:
- All server combinations tested
- Middleware interoperability
- Performance benchmarks
- Migration guide verification

---

## What This Means

### For Users
- ✅ Choose your server (Rust for speed, Python for compatibility)
- ✅ Migrate gradually or quickly - your choice
- ✅ Same GraphQL behavior across all options
- ✅ No forced changes in v2.0

### For FraiseQL Team
- ✅ Support multiple servers (not just one)
- ✅ Prioritize Rust, maintain Python
- ✅ Cleaner codebase (framework-agnostic core)
- ✅ Sustainable long-term

### For the Community
- ✅ Best of both worlds (performance + compatibility)
- ✅ Inclusive approach (Python teams welcome)
- ✅ Clear migration path (not forced)
- ✅ Professional, pragmatic solution

---

## Success Criteria

✅ **v2.0 Launch**:
- [ ] 3 servers fully supported (Axum, FastAPI, Starlette)
- [ ] Zero breaking changes for FastAPI/Starlette users
- [ ] Same middleware across all servers
- [ ] 7-10x performance improvement available via Axum
- [ ] Clear migration documentation

✅ **v2.0+ Growth**:
- [ ] Rust server adoption increases
- [ ] Users gradually migrate from Python to Rust
- [ ] Python servers remain stable, supported
- [ ] New features prioritized for Rust, backported to Python

---

## FAQ

**Q: Should I upgrade to v2.0?**
A: Yes! If using v1.8.x FastAPI or Starlette, you can upgrade to v2.0 with same server and get updates immediately.

**Q: Should I switch to Axum?**
A: Recommended for new applications. For existing deployments, migrate when team is ready (can be done gradually).

**Q: Will FastAPI be removed?**
A: Not in v2.0 or v2.1. Possible deprecation in v3.0+ (depends on adoption). Always available as option.

**Q: Do all servers support all features?**
A: Yes! Middleware, auth, RBAC, caching - all the same across Axum, Actix, Hyper, FastAPI, Starlette.

**Q: Can I use custom HTTP framework?**
A: Yes! Implement an adapter following our template. Middleware and GraphQL core reusable.

**Q: What if my team only knows Python?**
A: FastAPI in v2.0 works exactly like v1.8.x. Migrate to Rust when team learns Rust.

---

## Conclusion

This **multi-framework strategy** balances:

| Goal | Solution |
|------|----------|
| **Maximum Performance** | Axum (Rust) - 7-10x faster |
| **Backward Compatibility** | FastAPI/Starlette (Python) - unchanged |
| **Clear Choice** | 3 focused options + custom adapters |
| **Gradual Migration** | Start Python, move to Axum when ready |
| **Pragmatism** | Support what users need, minimize maintenance |

FraiseQL v2.0 provides **the best of both worlds** - performance when you need it, compatibility when you want it.

---

**Last Updated**: January 8, 2026
**Strategy**: Balanced, pragmatic, inclusive
**Status**: Ready for implementation
