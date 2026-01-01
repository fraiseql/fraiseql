# Documentation Cleanup Assessment

**Date**: 2026-01-01
**Context**: Phase 14 completion revealed documentation structure issues

---

## Problem

Phase-numbered documentation files (`phase10_*.md`, `phase11_*.md`, etc.) are mixed with user-facing documentation. These files describe **internal development phases** of FraiseQL, not features for end users.

## Current State

### Misplaced Files (Root `docs/`)

Files that should be in developer documentation:

1. **`docs/phase10_rust_authentication.md`**
   - Internal: Rust authentication implementation details
   - Contains: Architecture diagrams, code examples, performance benchmarks
   - Audience: FraiseQL contributors, not users

2. **`docs/phase11_rust_rbac.md`**
   - Internal: RBAC implementation in Rust
   - Contains: Database schema, permission resolution algorithm
   - Audience: FraiseQL contributors

3. **`docs/phase12_security_advanced.md`**
   - Internal: Advanced security implementation plan
   - Status: Appears to be a planning document

4. **`docs/phase12_security_constraints.md`**
   - Internal: Security constraints implementation
   - Contains: Rate limiting, IP filtering internals

5. **`docs/phase14_audit_logging.md`**
   - Internal: Audit logging implementation plan
   - Contains: Rust code examples, database schema design
   - Audience: FraiseQL contributors

6. **`docs/phase14_task_list.md`**
   - Internal: Development task list for Phase 14
   - Contains: Step-by-step implementation checklist
   - Audience: Developer working on Phase 14

7. **`docs/PHASE7_MIGRATION.md`**
   - Internal: Migration guide for Phase 7 changes
   - Contains: Breaking changes, internal API updates

### Files That Belong in `docs/phases/`

Currently `docs/phases/` only has:
- `phase7.1-where-order-by-passthrough.md`

Should also contain the above files.

### Proper User Documentation

User-facing docs that DO belong in root `docs/`:
- `docs/guides/` - How to use FraiseQL features
- `docs/tutorials/` - Step-by-step user tutorials
- `docs/features/` - Feature documentation for users
- `docs/getting-started/` - Quickstart for new users

## Recommended Structure

```
docs/
├── README.md                      # User-facing index
├── getting-started/               # User onboarding
├── guides/                        # User guides
├── tutorials/                     # User tutorials
├── features/                      # User feature docs
├── reference/                     # User API reference
├── production/                    # User deployment docs
└── developer-docs/                # ← NEW: Internal docs
    ├── README.md                  # Developer guide index
    ├── phases/                    # Development phases
    │   ├── phase10_rust_authentication.md
    │   ├── phase11_rust_rbac.md
    │   ├── phase12_security_constraints.md
    │   ├── phase14_audit_logging.md
    │   └── phase14_task_list.md
    ├── architecture/              # Internal architecture (keep existing)
    │   ├── decisions/
    │   ├── mutation-pipeline.md
    │   └── ...
    ├── planning/                  # Planning docs (from archive)
    └── PHASE7_MIGRATION.md        # Migration guides
```

## What Users Actually Need

Instead of phase implementation docs, users need:

1. **Authentication Guide** (`docs/guides/authentication.md`)
   - How to set up JWT authentication
   - Auth0 integration example
   - Custom JWT provider setup
   - NOT: Internal Rust implementation details

2. **RBAC Guide** (`docs/guides/rbac.md`)
   - How to define roles and permissions
   - Permission checking in resolvers
   - Multi-tenant RBAC patterns
   - NOT: Internal permission resolution algorithm

3. **Security Guide** (`docs/guides/security.md`)
   - Rate limiting configuration
   - IP filtering setup
   - Query complexity limits
   - NOT: Internal Rust security constraint implementation

4. **Audit Logging Guide** (`docs/guides/audit-logging.md`)
   - How to enable audit logging
   - Querying audit logs
   - Filtering by tenant/user/level
   - NOT: Database schema and Rust implementation

## Action Items

### 1. Create Developer Documentation Structure

```bash
mkdir -p docs/developer-docs/phases
mkdir -p docs/developer-docs/architecture
mkdir -p docs/developer-docs/planning
```

### 2. Move Internal Phase Docs

```bash
# Move phase docs
mv docs/phase10_rust_authentication.md docs/developer-docs/phases/
mv docs/phase11_rust_rbac.md docs/developer-docs/phases/
mv docs/phase12_security_advanced.md docs/developer-docs/phases/
mv docs/phase12_security_constraints.md docs/developer-docs/phases/
mv docs/phase14_audit_logging.md docs/developer-docs/phases/
mv docs/phase14_task_list.md docs/developer-docs/phases/
mv docs/PHASE7_MIGRATION.md docs/developer-docs/phases/
mv docs/phases/phase7.1-where-order-by-passthrough.md docs/developer-docs/phases/
```

### 3. Create User-Facing Guides

Create simplified, user-focused guides:

- `docs/guides/authentication.md` - Extract user-relevant parts from phase10
- `docs/guides/rbac.md` - Extract user-relevant parts from phase11
- `docs/guides/security.md` - Extract user-relevant parts from phase12
- `docs/guides/audit-logging.md` - Extract user-relevant parts from phase14

### 4. Create Developer Documentation Index

Create `docs/developer-docs/README.md` explaining:
- What this section contains
- When developers should read it
- How it differs from user docs

### 5. Update Main README

Update `docs/README.md` to NOT reference internal phase docs.

## Benefits

1. **Clear Separation**: Users don't see internal implementation details
2. **Better Onboarding**: New users find relevant guides, not phase plans
3. **Contributor Clarity**: Contributors know where to find architecture docs
4. **Maintainability**: Clear boundaries for what goes where
5. **Professionalism**: Project looks mature and well-organized

## Migration Path

1. **Phase 1**: Move files (no content changes)
   - Create `developer-docs/` structure
   - Move phase files
   - Update any hardcoded links

2. **Phase 2**: Create user guides (content extraction)
   - Extract user-relevant content from phase docs
   - Write user-focused guides
   - Add examples and quickstarts

3. **Phase 3**: Update indexes
   - Update `docs/README.md`
   - Create `developer-docs/README.md`
   - Update any navigation

## Notes

- Keep `docs/architecture/` for high-level architecture (it's referenced in user docs)
- Archive planning docs are fine in `docs/archive/planning/`
- Developer docs should explain "why" and "how it works"
- User docs should explain "how to use" and "what it does"
