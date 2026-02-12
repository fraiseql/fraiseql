# Complete FraiseQL Documentation Inventory

**Date:** February 4, 2026
**Project:** FraiseQL v2
**Purpose:** Comprehensive list of EVERY documentation file in the project
**Excludes:** Build artifacts, node_modules, compiled code

---

## Executive Summary

- **Total documentation files found:** 200+
- **Markdown files (.md):** ~170+
- **RestructuredText files (.rst):** 0
- **Other docs (.txt, etc.):** ~30+

### Documentation Breakdown by Category

1. **Core Project Documentation** (10 files)
2. **Language-Specific Implementations** (40+ files)
3. **Crate-Specific Documentation** (50+ files)
4. **Development & Process** (30+ files)
5. **Testing & Integration** (20+ files)
6. **Tools & Utilities** (15+ files)
7. **Archived/Historical** (40+ files)

---

## ROOT LEVEL DOCUMENTATION

### Main Project Docs

- `README.md` - Main project overview
- `DEVELOPMENT.md` - Development setup and workflow
- `CONTRIBUTING.md` - Contributing guidelines
- `TESTING.md` - Testing documentation
- `TROUBLESHOOTING.md` - Troubleshooting guide
- `SECURITY.md` - Security policy
- `LANGUAGE_IMPLEMENTATION_PLAN.md` - Language implementation strategy
- `DESIGN_QUALITY_VISION.md` - Design quality vision
- `FRAISEQL_DESIGN_RULES.md` - Design rules
- `RELEASE_NOTES.md` - Release notes

### Release Documentation

- `RELEASE_NOTES_v2.0.0-a1.md` - v2.0.0 alpha 1 release notes
- `RELEASE_NOTES_v2.1.0-agent.md` - v2.1.0 agent release notes
- `ALPHA_RELEASE_NOTES.md` - Alpha release notes

### Summaries & Reports

- `FINALIZATION_SUMMARY.txt` - Project finalization summary
- `GIT_HISTORY_VERIFICATION_REPORT.txt` - Git history verification
- `phase3_baseline_sample.txt` - Phase 3 baseline sample

---

## ADMIN DASHBOARD

- `admin-dashboard/README.md` - Admin dashboard overview

---

## `.CLAUDE/` - DEVELOPMENT PROCESS DOCUMENTATION

### Main Development Docs

- `.claude/CLAUDE.md` - Core development guidelines (473 lines)
- `.claude/ARCHITECTURE_PRINCIPLES.md` - Architecture principles (639 lines)
- `.claude/README.md` - Claude project README

### Archived Phase Documentation (15+ files)

- `.claude/archived/CYCLE1_COMPLETION_SUMMARY.md`
- `.claude/archived/CYCLE2_COMPLETION_SUMMARY.md`
- `.claude/archived/CYCLE3_COMPLETION_SUMMARY.md`
- `.claude/archived/CYCLE4_COMPLETION_SUMMARY.md`
- `.claude/archived/DEPENDENCY_ADVISORIES.md`
- `.claude/archived/FRAISEQL_V2_ROAD_TO_PRODUCTION.md`
- `.claude/archived/FRAISEQL_V2_UNIFIED_ROADMAP.md`
- `.claude/archived/GA_RELEASE_READINESS_REPORT.md`
- `.claude/archived/IMPLEMENTATION_PLAN_2_WEEK_TO_PRODUCTION.md`
- `.claude/archived/IMPLEMENTATION_STATUS_VERIFIED.md`
- `.claude/archived/NATS_VISION_ASSESSMENT.md`
- `.claude/archived/OBSERVER_E2E_IMPLEMENTATION.md`
- `.claude/archived/PHASE_10_ROADMAP.md`
- `.claude/archived/PHASE_21_TASK_1_4_FINAL_QUALITY_AUDIT.md`
- And more phase-specific archives...

---

## `.GITHUB/` - GITHUB-SPECIFIC DOCUMENTATION

- `.github/SECRETS_SETUP.md` - GitHub secrets setup guide
- `.github/ISSUE_TEMPLATE/alpha-bug-report.md` - Alpha bug report template
- `.github/ISSUE_TEMPLATE/alpha-feedback.md` - Alpha feedback template

---

## `DOCS/` - GENERAL DOCUMENTATION

- `docs/SECURITY_MIGRATION_v2.1.md` - Security migration guide (478 lines) **[RECENTLY UPDATED!]**

---

## CRATES - Rust Crate Documentation

### fraiseql-core

- `crates/fraiseql-core/docs/SECURITY_PATTERNS.md` - Security patterns
- `crates/fraiseql-core/benches/README.md` - Benchmarks README

### fraiseql-server

- `crates/fraiseql-server/src/auth/PHASE7_SECURITY_HARDENING.md` - Security hardening notes
- `crates/fraiseql-server/src/auth/constant_time_refactor_notes.md` - Constant time comparison notes
- `crates/fraiseql-server/src/auth/state_encryption_refactor_notes.md` - State encryption notes

### fraiseql-wire

Multiple documentation files:

- `crates/fraiseql-wire/PERFORMANCE_VALIDATION_RESULTS.md`
- `crates/fraiseql-wire/TROUBLESHOOTING.md`
- `crates/fraiseql-wire/METRICS_PERFORMANCE.md`
- `crates/fraiseql-wire/CHANGELOG.md`
- `crates/fraiseql-wire/PERFORMANCE_TUNING.md`
- `crates/fraiseql-wire/PRD.md`
- `crates/fraiseql-wire/benches/README.md`
- `crates/fraiseql-wire/benches/COMPARISON_GUIDE.md`
- And archived phase documentation in `.archive/phases/` (~10+ files)

### fraiseql-observers

Extensive documentation suite (40+ files):

- `crates/fraiseql-observers/README.md`
- `crates/fraiseql-observers/DEPLOYMENT_GUIDE.md`
- `crates/fraiseql-observers/DEPLOYMENT.md`
- `crates/fraiseql-observers/SCHEMA.md`
- `crates/fraiseql-observers/RELEASE_NOTES_PHASE_8.md`
- `crates/fraiseql-observers/PHASE_9_1_ACTION_TRACING_GUIDE.md`
- `crates/fraiseql-observers/PHASE_9_1_DESIGN.md`
- `crates/fraiseql-observers/PHASE_9_1_COMPLETION_SUMMARY.md`
- `crates/fraiseql-observers/PHASE_9_1_JAEGER_INTEGRATION_GUIDE.md`
- `crates/fraiseql-observers/PHASE_9_1_IMPLEMENTATION_GUIDE.md`
- `crates/fraiseql-observers/PHASE_9_2_B_MACROS_GUIDE.md`
- `crates/fraiseql-observers/PHASE_9_2_C_LOGGING_GUIDE.md`
- `crates/fraiseql-observers/PHASE_9_2_DESIGN.md`
- `crates/fraiseql-observers/docs/README.md`
- `crates/fraiseql-observers/docs/MIGRATION_GUIDE.md`
- `crates/fraiseql-observers/docs/INTEGRATION_GUIDE.md`
- `crates/fraiseql-observers/docs/PERFORMANCE_TUNING.md`
- `crates/fraiseql-observers/examples/README.md`
- `crates/fraiseql-observers/tests/README.md`
- `.claude/` phase documentation (10+ files)

---

## LANGUAGE IMPLEMENTATIONS - Client Libraries

### fraiseql-python (Python Implementation)

- `fraiseql-python/README.md`
- `fraiseql-python/docs/GETTING_STARTED.md`
- `fraiseql-python/docs/INSTALLATION.md`
- `fraiseql-python/docs/DECORATORS_REFERENCE.md`
- `fraiseql-python/docs/EXAMPLES.md`
- `fraiseql-python/docs/TROUBLESHOOTING.md`
- `fraiseql-python/docs/ANALYTICS_GUIDE.md`

### fraiseql-typescript (TypeScript Implementation)

- `fraiseql-typescript/README.md` (likely, based on package structure)

### fraiseql-java (Java Implementation)

- `fraiseql-java/README.md`
- `fraiseql-java/INSTALL.md`
- `fraiseql-java/EXAMPLES.md`
- `fraiseql-java/API_GUIDE.md`
- `fraiseql-java/CHANGELOG.md`
- `fraiseql-java/CONTRIBUTING.md`
- `fraiseql-java/RELEASE_CHECKLIST.md`

### fraiseql-go (Go Implementation)

- `fraiseql-go/README.md`
- `fraiseql-go/IMPLEMENTATION_SUMMARY.md`
- `fraiseql-go/CONTRIBUTING.md`
- `fraiseql-go/examples/README.md`

### fraiseql-php (PHP Implementation)

- `fraiseql-php/PHP_FEATURE_PARITY.md`

### fraiseql-scala (Scala Implementation)

- `fraiseql-scala/README.md`
- `fraiseql-scala/SCALA_FEATURE_PARITY.md`

### fraiseql-clojure (Clojure Implementation)

- `fraiseql-clojure/README.md`
- `fraiseql-clojure/CLOJURE_FEATURE_PARITY.md`

### fraiseql-elixir (Elixir Implementation)

- `fraiseql-elixir/README.md`
- `fraiseql-elixir/ELIXIR_FEATURE_PARITY.md`

---

## TESTING & INTEGRATION

### Test Suite Documentation

- `tests/README.md` - General test documentation
- `tests/docker/README.md` - Docker test setup
- `tests/integration/README.md` - Integration test overview
- `tests/integration/APOLLO_ROUTER.md` - Apollo Router integration
- `tests/integration/FEDERATION_TESTS.md` - Federation testing
- `tests/integration/FEDERATION_OBSERVABILITY_PLAN.md` - Federation observability
- `tests/integration/EXTENDED_MUTATIONS.md` - Extended mutations
- `tests/integration/QUERY_OPTIMIZATION.md` - Query optimization

---

## TOOLS & UTILITIES

### Tool Documentation

- `tools/RECOMMENDED_TOOLS.md` - Recommended development tools
- `tools/DOCS_VALIDATION.md` - Documentation validation
- `tools/fraiseql_tools/USAGE.md` - Tool usage
- `tools/fraiseql_tools/IMPLEMENTATION.md` - Tool implementation

### Examples

- `examples/python/requirements.txt` - Python requirements

---

## SECURITY & MIGRATIONS

### Main Security Migration

- `docs/SECURITY_MIGRATION_v2.1.md` - **[UPDATED IN THIS SESSION]**
  - Covers OIDC audience validation
  - Admin endpoints protection
  - Introspection/schema export protection
  - Playground & CORS safety
  - Rate limiting (newly added section)
  - Configuration and deployment

---

## CATEGORIZED BY REVIEW PRIORITY

### Priority 1: MUST REVIEW (Core Project Docs)

1. `/home/lionel/code/fraiseql/README.md` (359 lines)
2. `/home/lionel/code/fraiseql/.claude/CLAUDE.md` (473 lines)
3. `/home/lionel/code/fraiseql/.claude/ARCHITECTURE_PRINCIPLES.md` (639 lines)
4. `/home/lionel/code/fraiseql/docs/SECURITY_MIGRATION_v2.1.md` (478 lines) **[RECENTLY UPDATED]**

### Priority 2: SHOULD REVIEW (Development & Operations)

5. `/home/lionel/code/fraiseql/DEVELOPMENT.md` (363 lines)
6. `/home/lionel/code/fraiseql/CONTRIBUTING.md` (363 lines)
7. `/home/lionel/code/fraiseql/TESTING.md` (432 lines)
8. `/home/lionel/code/fraiseql/TROUBLESHOOTING.md`
9. `/home/lionel/code/fraiseql/SECURITY.md`

### Priority 3: NICE TO REVIEW (Release & Status)

10. `/home/lionel/code/fraiseql/RELEASE_NOTES_v2.1.0-agent.md`
11. `/home/lionel/code/fraiseql/DESIGN_QUALITY_VISION.md`

### Priority 4: OPTIONAL (Specialized)

12. `/home/lionel/code/fraiseql/FRAISEQL_DESIGN_RULES.md`
13. `/home/lionel/code/fraiseql/.claude/README.md`
14. `/home/lionel/code/fraiseql/LANGUAGE_IMPLEMENTATION_PLAN.md`
15. `/home/lionel/code/fraiseql/.github/SECRETS_SETUP.md`

### Priority 5: IMPLEMENTATION-SPECIFIC (for context)

- Language-specific implementations (Python, TypeScript, Java, Go, etc.)
- Crate-specific documentation (fraiseql-observers, fraiseql-wire, etc.)
- Testing & integration guides
- Archived phase documentation

---

## FILES RECENTLY CHANGED (This Session)

The following file was updated during the rate limiting implementation:

### ✅ docs/SECURITY_MIGRATION_v2.1.md

- **Added Section:** Rate Limiting
  - Configuration options (TOML and environment variables)
  - Response headers (X-RateLimit-Limit, X-RateLimit-Remaining)
  - Best practices and recommended limits
  - Disabling rate limiting if needed
  - Distinction from auth endpoint rate limiting
- **Why:** Rate limiting middleware was integrated into the GraphQL server
- **Lines:** ~50 new lines added
- **Status:** Should be reviewed to ensure accuracy and completeness

---

## DOCUMENTATION CHARACTERISTICS

### By Volume (Lines of Code)

- Largest: `.claude/ARCHITECTURE_PRINCIPLES.md` (639 lines)
- Second: `.claude/CLAUDE.md` (473 lines)
- Third: `docs/SECURITY_MIGRATION_v2.1.md` (478 lines)
- Fourth: `TESTING.md` (432 lines)

### By Category

- **Implementation-Specific:** ~70 files (Python, Java, Go, etc.)
- **Process Documentation:** ~40 files (archived phases, plans)
- **Crate Documentation:** ~50 files
- **Core Project:** ~10 files
- **Testing:** ~10 files
- **Tools:** ~10 files

### By Status

- **Active/Current:** ~120 files
- **Archived/Historical:** ~40+ files
- **Auto-generated (from build):** Not counted

---

## RECOMMENDED REVIEW APPROACH

### Session 1: Core (30-45 minutes)

Review Priority 1 only:

1. README.md
2. .claude/CLAUDE.md
3. .claude/ARCHITECTURE_PRINCIPLES.md
4. docs/SECURITY_MIGRATION_v2.1.md

### Session 2: Development (45 min - 1 hour)

Review Priority 2:
5. DEVELOPMENT.md
6. CONTRIBUTING.md
7. TESTING.md
8. TROUBLESHOOTING.md
9. SECURITY.md

### Session 3: Enhanced (Optional, 30+ minutes)

Review Priority 3-4:
10. RELEASE_NOTES_v2.1.0-agent.md
11. DESIGN_QUALITY_VISION.md
12. And others as interested

### Sessions 4+: Deep Dive (Optional)

Language implementations, crate documentation, archived phases

---

## NOTES

- **Build Artifacts:** Target and node_modules directories excluded from this inventory
- **Auto-generated:** Some files in build output excluded
- **Archived:** Historical phase documentation is included but marked as archived
- **Most Important:** The 4 Priority 1 files should be your starting point
- **Recently Updated:** `docs/SECURITY_MIGRATION_v2.1.md` - pay special attention to the rate limiting section

---

## HOW TO USE THIS INVENTORY

This inventory is meant to:

1. Show you what documentation exists
2. Help prioritize your review
3. Provide context for the review process
4. Track which files have been reviewed

Use it in combination with:

- `START_HERE.txt` - Quick start guide
- `documentation_review_prompt.md` - AI agent instructions
- `FEEDBACK_QUICK_REFERENCE.txt` - Feedback format examples

---

**Document Created:** February 4, 2026
**For:** FraiseQL Documentation Review Session
