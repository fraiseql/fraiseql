# Fraisier Integration Documentation

This directory contains comprehensive documentation for integrating Fraisier (the deployment orchestrator) into the FraiseQL monorepo.

## Documents Included

### 1. **FRAISIER_INTEGRATION_ANALYSIS.md** ⭐ START HERE

**Purpose:** Comprehensive analysis of Fraisier and its role in FraiseQL

**Contents:**

- What is Fraisier and what does it do
- Current repository structure and the duplication problem
- Detailed integration points with FraiseQL core
- Architecture patterns (CQRS, deployment strategies, provider abstraction)
- Risk assessment and mitigation strategies
- Phase-by-phase implementation roadmap
- Workspace and dependency management

**When to Read:**

- Onboarding to Fraisier
- Understanding architecture decisions
- Planning integration phases
- Understanding deployment workflows

**Length:** ~400 lines

---

### 2. **FRAISIER_QUICK_REFERENCE.md** ⭐ FOR DAILY USE

**Purpose:** Quick reference guide for development and deployment

**Contents:**

- What is Fraisier (one-paragraph summary)
- Quick architecture overview
- Configuration examples (fraises.yaml)
- CLI commands reference
- Webhook setup for all providers (GitHub, GitLab, Gitea, Bitbucket)
- Deployment workflow breakdown
- Database schema (CQRS pattern)
- Git provider abstractions
- Integration points with FraiseQL core
- Common tasks and troubleshooting
- File reference guide

**When to Use:**

- Daily development work
- Setting up webhooks
- Remembering CLI commands
- Quick lookups
- Teaching new team members

**Length:** ~300 lines

---

### 3. **FRAISIER_ACTION_ITEMS.md** ⭐ FOR PROJECT MANAGEMENT

**Purpose:** Concrete action items organized by phase

**Contents:**

- Phase 0: Immediate consolidation (remove duplication)
- Phase 1: Monorepo integration
- Phase 2: Documentation (provider setup guides, deployment guides)
- Phase 3: Testing infrastructure (fixtures, unit tests, integration tests)
- Phase 4: Schema & database (models, schema.sql)
- Phase 5: API & GraphQL (resolvers, queries)
- Phase 6: Integration testing with fraiseql-server
- Phase 7: Production hardening

**Each Action Includes:**

- Status (✅ DONE / ⏳ TODO)
- Priority (P0/P1/P2)
- Effort estimate
- Checklist items
- File locations
- Code examples

**When to Use:**

- Project planning
- Assigning work
- Tracking progress
- Understanding dependencies
- Estimating timeline

**Length:** ~250 lines

---

## Quick Navigation

### I need to understand

| Question | Document | Section |
|----------|----------|---------|
| What is Fraisier? | QUICK_REFERENCE | "What is Fraisier?" |
| How does it work? | INTEGRATION_ANALYSIS | "Core Architecture Principle" |
| How do I set up webhooks? | QUICK_REFERENCE | "Webhook Setup" |
| How do I deploy a service? | QUICK_REFERENCE | "Common Tasks" |
| What's wrong with the current setup? | INTEGRATION_ANALYSIS | "Current Issue" |
| What needs to be done? | ACTION_ITEMS | "Summary Table" |
| When should we do X? | INTEGRATION_ANALYSIS | "Implementation Roadmap" |
| How does it integrate with FraiseQL? | INTEGRATION_ANALYSIS | "Integration Points with FraiseQL Core" |
| How do I troubleshoot X? | QUICK_REFERENCE | "Troubleshooting" |

---

## Key Findings

### Current Status

- ✅ Fraisier code is **feature complete**
- ✅ Supports 4 Git providers (GitHub, GitLab, Gitea, Bitbucket)
- ✅ Supports 4 service types (API, ETL, Scheduled, Backup)
- ⚠️ **Critical Issue:** Duplicated in two locations
- ⏳ Not yet integrated with fraiseql-server and fraiseql-cli

### Integration Strategy

```
Step 1: Consolidate into monorepo (Phase 0)
  ↓
Step 2: Complete documentation (Phase 2)
  ↓
Step 3: Build test infrastructure (Phase 3)
  ↓
Step 4: Define schema & database (Phase 4)
  ↓
Step 5: Expose via GraphQL API (Phase 5-6)
  ↓
Step 6: Production hardening (Phase 7)
```

### Immediate Action Required

**Delete `/home/lionel/code/fraisier/` and establish `/home/lionel/code/fraiseql/fraisier/` as the single source of truth.**

This should take ~5 minutes but is critical to prevent maintenance headaches.

---

## File Structure

```
.claude/
├── FRAISIER_INTEGRATION_ANALYSIS.md   (This comprehensive guide)
├── FRAISIER_QUICK_REFERENCE.md        (Daily reference)
├── FRAISIER_ACTION_ITEMS.md           (Work items & timeline)
└── README_FRAISIER_DOCS.md            (This file)

fraiseql/fraisier/                     (Reference implementation)
├── fraisier/                          (Python package)
│   ├── cli.py                         (CLI interface)
│   ├── webhook.py                     (FastAPI webhook server)
│   ├── config.py                      (YAML parsing)
│   ├── database.py                    (SQLite with CQRS)
│   ├── deployers/                     (Deployment strategies)
│   └── git/                           (Git provider abstractions)
├── pyproject.toml                     (Python package config)
├── fraises.example.yaml               (Configuration example)
└── README.md                          (User documentation)
```

---

## Learning Path

### For New Team Members

**Day 1: Understand the Concept**

1. Read: QUICK_REFERENCE → "What is Fraisier?"
2. Read: INTEGRATION_ANALYSIS → "What is Fraisier?"
3. Skim: QUICK_REFERENCE → "Configuration"

**Day 2: Understand the Architecture**

1. Read: INTEGRATION_ANALYSIS → "Core Architecture Principle"
2. Read: INTEGRATION_ANALYSIS → "Architecture Patterns"
3. Read: QUICK_REFERENCE → "Quick Architecture"

**Day 3: Hands-On**

1. Read: QUICK_REFERENCE → "CLI Commands"
2. Read: QUICK_REFERENCE → "Webhook Setup" (pick one provider)
3. Try: Set up a test webhook locally

**Day 4-5: Deep Dive**

1. Read: INTEGRATION_ANALYSIS → "Integration Points with FraiseQL Core"
2. Read: QUICK_REFERENCE → "Git Providers"
3. Explore: `/home/lionel/code/fraiseql/fraisier/` code

---

## Key Concepts to Understand

### 1. **Fraise** (French: Strawberry)

- A deployable service (API, ETL, scheduled job, or backup)
- Defined in `fraises.yaml`
- Has multiple environments (dev, staging, prod)
- Has a Git branch mapping

### 2. **Fraisier** (French: Strawberry Plant)

- The orchestrator that manages fraises
- Listens for Git webhooks
- Triggers deployments based on branch mapping
- Tracks deployment history

### 3. **Webhook Workflow**

```
Git Push → Webhook Sent → Fraisier Receives → Branch Matched → Deploy Triggered
```

### 4. **CQRS Pattern** (Used for Deployment History)

- **Write tables:** Append-only (tb_deployments, tb_webhook_events)
- **Read views:** Materialized views for queries (v_fraise_status, v_deployment_history)
- Benefits: Audit trail, query optimization, immutable history

### 5. **Provider Abstraction**

- Different Git platforms (GitHub, GitLab, Gitea, Bitbucket)
- Each implements `GitProvider` interface
- Signature verification (HMAC-SHA256 or tokens)
- Webhook payload normalization

### 6. **Deployment Strategies**

- **API:** systemd service, health checks, database migrations
- **ETL:** script execution, logging, notifications
- **Scheduled:** systemd timers (cron-like)
- **Backup:** database backups, remote sync, retention

---

## Development Workflow

### Typical Development Task

```
1. Assign action item from ACTION_ITEMS.md
   ↓
2. Read relevant section from INTEGRATION_ANALYSIS.md
   ↓
3. Check QUICK_REFERENCE.md for syntax/examples
   ↓
4. Implement changes
   ↓
5. Test locally
   ↓
6. Update documentation if needed
   ↓
7. Create pull request
   ↓
8. Review against acceptance criteria
```

### Common Workflows

**Adding a new Git provider:**

1. Read: ACTION_ITEMS.md → "Action 2.1"
2. Reference: QUICK_REFERENCE.md → "Git Providers"
3. Implement `fraisier/git/new_provider.py`
4. Copy pattern from `fraisier/git/github.py`
5. Add documentation: `fraisier/docs/setup-newprovider.md`

**Fixing a bug:**

1. Check: ACTION_ITEMS.md → "Phase 3: Testing Infrastructure"
2. Add test case to `tests/`
3. Implement fix
4. Verify test passes
5. Update `docs/troubleshooting.md` if applicable

**Adding a feature:**

1. Define in ACTION_ITEMS.md which phase it belongs to
2. Plan implementation steps
3. Check dependencies on other phases
4. Implement + test
5. Update QUICK_REFERENCE.md if new CLI commands

---

## Questions?

### Where do I find X?

| Looking for | Location |
|---|---|
| Configuration syntax | QUICK_REFERENCE.md → "Configuration" |
| CLI command reference | QUICK_REFERENCE.md → "CLI Commands" |
| How to set up GitHub webhook | QUICK_REFERENCE.md → "Webhook Setup" |
| Deployment workflow details | QUICK_REFERENCE.md → "Deployment Workflow" |
| CQRS pattern explanation | INTEGRATION_ANALYSIS.md → "Architecture Patterns" |
| Full implementation roadmap | ACTION_ITEMS.md → "Phase X" |
| Risk assessment | INTEGRATION_ANALYSIS.md → "Risk Assessment" |
| How Fraisier relates to Rust engine | INTEGRATION_ANALYSIS.md → "Integration Points" |
| Database schema | QUICK_REFERENCE.md → "Database Schema" |
| Code examples | QUICK_REFERENCE.md → anywhere with code blocks |

---

## Related Documentation

- **FraiseQL Architecture:** `/home/lionel/code/fraiseql/.claude/CLAUDE.md`
- **Fraisier Code:** `/home/lionel/code/fraiseql/fraisier/`
- **FraiseQL Roadmap:** `/home/lionel/code/fraiseql/.claude/IMPLEMENTATION_ROADMAP.md`

---

## Document Maintenance

**These documents should be updated when:**

- [ ] Phase completes (update action item status)
- [ ] New feature added (update QUICK_REFERENCE.md)
- [ ] Architecture decision made (update INTEGRATION_ANALYSIS.md)
- [ ] New troubleshooting steps discovered (update QUICK_REFERENCE.md)

**Last Updated:** 2026-01-15
**Maintainer:** Architecture team
**Review Cycle:** Monthly or as needed
