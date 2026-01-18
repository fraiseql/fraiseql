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
- **[Architecture: Database](../architecture/database/)** — Database type mappings

---

**Back to:** [Documentation Home](../README.md)
