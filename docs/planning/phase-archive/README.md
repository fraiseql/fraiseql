# FraiseQL Phase Planning Archive

This directory contains historical phase planning documents from the v1 → v2 development cycle.

These documents are preserved for reference and understanding the architectural decisions made during development, but are no longer actively maintained or actionable.

## Phase 2.4: Stress Testing Plan
**File**: `PHASE_2_4_STRESS_TESTING_PLAN.md`

Historical planning for subscription stress testing and extreme concurrency scenarios. Provides insight into performance targets and testing methodology that informed the final architecture.

**Status**: Complete and superseded by Phase 3c unified FFI implementation

**Key Insights**:
- Stress testing up to 10,000+ concurrent connections
- WebSocket connection handling and failure modes
- Memory pressure and degradation patterns

## Phase 3: Security Audit Plan
**File**: `PHASE_3_SECURITY_AUDIT_PLAN.md`

Historical security architecture analysis for Federation + JSONB. Identifies security gaps that were addressed during Phase 3c implementation.

**Status**: Complete and integrated into Phase 3c work

**Key Findings**:
- Identified user-level row filtering gaps
- Federation context isolation requirements
- Multi-tenant enforcement strategy
- Subscription scope verification needs

## How to Use This Archive

1. **Understanding Architecture**: Read phase docs to understand why certain decisions were made
2. **Historical Context**: See the evolution of design thinking across phases
3. **Backlog Reference**: Some items may become relevant for future v2.1+ work

## Current Development

For the latest development roadmap and planning, see:
- `/docs/` - Current v2 documentation
- `/docs/v2-ROADMAP.md` - Active roadmap (when created)
- Git commit messages - Day-to-day development decisions

## Related Files

- `.phases/` directory - Phase execution scripts and current phase planning
- `CLAUDE.md` - Project development guide

---

**Last Updated**: 2026-01-08 (moved to archive during v2 cleanup)
**Repository**: FraiseQL v2 Release Preparation
