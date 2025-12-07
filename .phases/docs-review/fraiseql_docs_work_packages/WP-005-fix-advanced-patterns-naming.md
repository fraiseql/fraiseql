# Work Package: Fix Advanced Patterns Documentation SQL Naming

**Package ID:** WP-005
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 10 hours
**Dependencies:** WP-001 (Core docs fixed)

---

## Objective

Fix SQL naming in advanced patterns documentation to use `tb_*`, `v_*`, `tv_*` topology consistently.

---

## Files to Update

1. **`docs/advanced/database-patterns.md`** (lines 1226, 1464, 1533)
   - Replace `orders` → `tb_order`
   - Replace `categories` → `tb_category`  
   - Replace `categories_ltree` → `tv_category_tree`

2. **`docs/advanced/multi-tenancy.md`** (lines 153, 162)
   - Replace `users` → `tb_user`
   - Replace `orders` → `tb_order`
   - Update RLS examples to reference base tables

3. **`docs/advanced/bounded-contexts.md`** (lines 185, 194)
   - Replace `orders.orders` → `orders.tb_order`
   - Update schema-qualified table names

---

## Acceptance Criteria

- [ ] All 3 files use `tb_*`, `v_*`, `tv_*` naming
- [ ] Multi-tenancy RLS examples reference base tables correctly
- [ ] Bounded context examples show proper schema qualification
- [ ] No instances of simple table names (`orders`, `categories`, `users`)
- [ ] Code examples run on PostgreSQL (valid SQL)
- [ ] Follows style guide

---

## Implementation Pattern

For each file:
1. Search for problematic table names
2. Replace with trinity pattern
3. Add view definitions where appropriate
4. Test SQL examples
5. Verify RLS/bounded context logic still correct

**Example fix:**

OLD:
```sql
CREATE TABLE orders (
    id UUID PRIMARY KEY,
    user_id UUID REFERENCES users(id)
);
```

NEW:
```sql
CREATE TABLE tb_order (
    id UUID PRIMARY KEY,
    user_id UUID REFERENCES tb_user(id)
);

CREATE VIEW v_order AS
SELECT * FROM tb_order;
```

---

**Deadline:** End of Week 1

**End of Work Package WP-005**
