# Endpoint Runtime Archive

## Overview

The Endpoint Runtime is a **separate sub-project** planned to extend FraiseQL v2 core with:

- **Webhooks** — Event-driven architecture
- **File uploads & processing** — Image optimization, virus scanning
- **Authentication** — OAuth 2.0, OIDC, JWT
- **Notifications** — Email, SMS, push notifications, chat integration
- **Observers** — Database-driven action execution
- **Advanced features** — Search, caching, queues, subscriptions
- **Interceptors** — WASM-based custom logic

## Current Status

**FraiseQL v2 Core (Phases 1-7)** ✅ Complete and production-ready.

The Endpoint Runtime is documented in `endpoint-runtime-archive-20260201.tar.gz` as a future initiative. This archive contains:

- 10 detailed phase plans (phases 1-10)
- Architecture decision records
- Implementation guides
- Feature specifications

## Extracting the Archive

To review Endpoint Runtime plans:

```bash
cd docs
tar -xzf endpoint-runtime-archive-20260201.tar.gz
cd endpoint-runtime
# View 00-OVERVIEW.md for complete roadmap
```

## Important Notes

1. **Not part of v2.0.0 GA release** — Endpoint Runtime is future work
2. **Separate development initiative** — Will be managed as a distinct project when started
3. **Archived for documentation** — Plans preserved for future reference
4. **Core system is complete** — FraiseQL v2 GraphQL execution engine is production-ready today

## Current Focus

FraiseQL v2 v2.0.0 focuses on the **core GraphQL compilation and execution engine** with:

- ✅ Multi-database support (PostgreSQL, MySQL, SQL Server, SQLite)
- ✅ 5 language generators (Python, TypeScript, Go, Java, PHP)
- ✅ Enterprise security features (Phase 7)
- ✅ Production-ready architecture

See [README.md](../README.md) for current capabilities.

## Future

When Endpoint Runtime development begins:

1. Extract archive: `tar -xzf endpoint-runtime-archive-20260201.tar.gz`
2. Create separate repo/branch: `fraiseql-endpoint-runtime`
3. Follow phased implementation plan in `00-OVERVIEW.md`
4. Reference architectural patterns from core engine

---

**Archive Date:** February 1, 2026
**Archive Source:** `/home/lionel/code/fraiseql/docs/endpoint-runtime/` (18 files, ~500KB)
