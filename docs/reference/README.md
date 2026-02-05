# FraiseQL v2 Reference

Complete API and operator references.

---

## 📚 Reference Documentation

### Type System

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [scalars.md](scalars.md) | Scalar type library and custom scalars | 1,492 | Reference |

**Topics Covered:**

- Built-in scalar types (String, Int, Float, Boolean, ID)
- Extended scalars (Date, DateTime, Time, UUID, JSON, etc.)
- Custom scalar creation
- Scalar validation rules
- Serialization formats
- Database type mappings

---

### Query Operators

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [where-operators.md](where-operators.md) | Complete WHERE operator catalog | 1,137 | Reference |

**Topics Covered:**

- Comparison operators (eq, neq, gt, gte, lt, lte)
- String operators (contains, startsWith, endsWith, regex)
- List operators (in, notIn, isEmpty, isNotEmpty)
- Null operators (isNull, isNotNull)
- Logical operators (and, or, not)
- JSON operators (jsonPath, jsonContains)
- Array operators (arrayContains, arrayOverlap)
- Database-specific operators (PostgreSQL JSONB, full-text search)
- SQL generation examples

---

### Authoring Tools

| Document | Description |
|----------|-------------|
| [cli-schema-format.md](cli-schema-format.md) | FraiseQL CLI schema format reference |
| [view-selection-api.md](view-selection-api.md) | View selection API for automatic schema generation |

---

### Distributed Transactions

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [SAGA_API.md](SAGA_API.md) | SAGA API reference for distributed transactions | 800+ | Reference |

**Topics Covered:**

- SAGA pattern for multi-database transactions
- Coordinator API
- Participant API
- Compensation strategies
- Retry policies
- Timeout handling

---

### REST API Reference

- **[API Reference](api/)** — Complete HTTP API endpoint documentation

---

## 🎯 Using These References

**For Schema Authors:**

- Reference [scalars.md](scalars.md) when defining types
- Use [where-operators.md](where-operators.md) to understand available filters

**For Frontend Developers:**

- Bookmark [where-operators.md](where-operators.md) for query building
- Check [scalars.md](scalars.md) for type serialization

**For Compiler/Runtime Developers:**

- Implement operators from [where-operators.md](where-operators.md)
- Add new scalars following patterns in [scalars.md](scalars.md)

---

## 📚 Related Documentation

- **[Specs: Authoring Contract](../specs/authoring-contract.md)** — Schema authoring rules
- **[Specs: Compiled Schema](../specs/compiled-schema.md)** — Compiled type system
- **[View Selection Guide](../architecture/database/view-selection-guide.md)** — Database view patterns and type mappings

---

**Back to:** [Documentation Home](../README.md)
