# FraiseQL v2 Specifications

Detailed technical specifications for implementers and integrators.

---

## 📋 Specifications Overview

### Compilation Artifacts

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [compiled-schema.md](compiled-schema.md) | CompiledSchema JSON structure | 748 | 40 min |
| [authoring-contract.md](authoring-contract.md) | Schema authoring API and validation rules | 808 | 60 min |
| [capability-manifest.md](capability-manifest.md) | Database capability declaration | 731 | 30 min |
| [schema-conventions.md](schema-conventions.md) | Database naming conventions (tb_*, v_*, fn_*) | 1,287 | 50 min |

### Runtime Features

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [caching.md](caching.md) | Query result caching and invalidation | 1,008 | 30 min |
| [persisted-queries.md](persisted-queries.md) | Automatic Persisted Queries (APQ) | 1,172 | 60 min |
| [introspection.md](introspection.md) | GraphQL introspection policies | 967 | 25 min |
| [pagination-keyset.md](pagination-keyset.md) | Keyset-based pagination | 710 | 30 min |

### Analytics & Aggregation

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [aggregation-operators.md](aggregation-operators.md) | Aggregation functions (SUM, AVG, COUNT, etc) | 900+ | 45 min |
| [window-operators.md](window-operators.md) | Window functions (ROW_NUMBER, LAG, LEAD, etc) | 850+ | 40 min |
| [analytical-schema-conventions.md](analytical-schema-conventions.md) | Fact table and analytics patterns | 600+ | 30 min |

### Data Formats

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [cdc-format.md](cdc-format.md) | Change Data Capture event format | 872 | 45 min |

### Security & Compliance

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [security-compliance.md](security-compliance.md) | Security profiles and compliance (NIS2, GDPR) | 1,638 | 40 min |

---

## 🎯 Reading Paths

**For Compiler Developers:**

1. authoring-contract.md — What schema authors write
2. capability-manifest.md — Database-specific capabilities
3. compiled-schema.md — Compiler output format

**For Runtime Developers:**

1. compiled-schema.md — Runtime input format
2. caching.md — Query result caching
3. persisted-queries.md — APQ implementation
4. pagination-keyset.md — Pagination logic

**For Database Architects:**

1. schema-conventions.md — Required database patterns
2. cdc-format.md — Event stream format
3. capability-manifest.md — Database capability declaration

**For Operations/Security:**

1. security-compliance.md — Security profiles
2. persisted-queries.md — Query security modes
3. introspection.md — Schema introspection controls

---

## 📚 Related Documentation

- **[Architecture](../architecture/)** — System design and patterns
- **[Guides](../guides/)** — Practical implementation guides
- **[Reference](../reference/)** — API references

---

**Back to:** [Documentation Home](../README.md)
