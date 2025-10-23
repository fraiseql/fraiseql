# ADR 003: Unified Audit Table with CDC + Cryptographic Chain

## Status
Accepted

## Context
We needed enterprise-grade audit logging with:
- Change Data Capture (CDC) for compliance
- Cryptographic chain integrity for tamper-evidence
- Multi-tenant isolation
- PostgreSQL-native implementation (no external dependencies)

Initially considered separate tables:
- `tenant.tb_audit_log` for CDC data
- `audit_events` for cryptographic chain

## Decision
Use **one unified `audit_events` table** that combines both CDC and cryptographic features.

## Rationale
1. **Simplicity**: One table to understand, query, and maintain
2. **Performance**: No duplicate writes, no bridge synchronization
3. **Integrity**: Single source of truth, atomic operations
4. **Philosophy**: Aligns with "In PostgreSQL Everything"
5. **Developer Experience**: Easier to work with, fewer moving parts

## Consequences
### Positive
- Reduced complexity (1 table instead of 2)
- Better performance (no duplicate writes)
- Easier to query (single table)
- Simpler schema migrations

### Negative
- None identified

## Implementation
See: `src/fraiseql/enterprise/migrations/002_unified_audit.sql`
