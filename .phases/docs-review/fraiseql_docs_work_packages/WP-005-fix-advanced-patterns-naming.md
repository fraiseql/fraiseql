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

## 🤖 Local Model Execution Instructions

**This work package is IDEAL for local 8B models** (search & replace with explicit patterns)

**Execution Strategy:**
1. **Preparation** (Claude does this - 10 minutes)
2. **Transformation** (Local model executes - 20 minutes)
3. **Verification** (Claude checks - 10 minutes)

**Total time:** ~40 minutes (vs 10 hours manual)

---

### Step 1: Preparation (Claude)

**Verify target files exist:**
```bash
cd /home/lionel/code/fraiseql

# Check files exist
ls -la docs/advanced/database-patterns.md
ls -la docs/advanced/multi-tenancy.md
ls -la docs/advanced/bounded-contexts.md
```

**Count instances to fix:**
```bash
# database-patterns.md
grep -n "CREATE TABLE orders\|CREATE TABLE categories\|categories_ltree" docs/advanced/database-patterns.md

# multi-tenancy.md
grep -n "CREATE TABLE users\|CREATE TABLE orders\|REFERENCES users(\|REFERENCES orders(" docs/advanced/multi-tenancy.md

# bounded-contexts.md
grep -n "orders\.orders\|orders\.categories" docs/advanced/bounded-contexts.md
```

**Expected:** Should find instances at lines mentioned in "Files to Update"

---

### Step 2: Transformation Patterns (Local Model)

**Pattern 1: Table name replacements**
```
File: docs/advanced/database-patterns.md

Search:  CREATE TABLE orders
Replace: CREATE TABLE tb_order

Search:  CREATE TABLE categories
Replace: CREATE TABLE tb_category

Search:  categories_ltree
Replace: tv_category_tree

Search:  FROM orders
Replace: FROM tb_order

Search:  FROM categories
Replace: FROM tb_category
```

**Pattern 2: Foreign key updates**
```
File: docs/advanced/multi-tenancy.md

Search:  CREATE TABLE users
Replace: CREATE TABLE tb_user

Search:  CREATE TABLE orders
Replace: CREATE TABLE tb_order

Search:  REFERENCES users(
Replace: REFERENCES tb_user(

Search:  REFERENCES orders(
Replace: REFERENCES tb_order(

Search:  FROM users
Replace: FROM tb_user

Search:  FROM orders
Replace: FROM tb_order

Search:  JOIN users
Replace: JOIN tb_user

Search:  JOIN orders
Replace: JOIN tb_order
```

**Pattern 3: Schema-qualified names**
```
File: docs/advanced/bounded-contexts.md

Search:  orders.orders
Replace: orders.tb_order

Search:  orders.categories
Replace: orders.tb_category

Search:  inventory.products
Replace: inventory.tb_product
```

**Pattern 4: Add view definitions (where CREATE TABLE appears)**
```
After blocks like:
CREATE TABLE tb_order (
    ...
);

Add:
CREATE VIEW v_order AS
SELECT * FROM tb_order;
```

---

### Step 3: Execution Commands (Local Model)

**Option A: Using sed (recommended for line-by-line replacements)**

```bash
cd /home/lionel/code/fraiseql

# File 1: database-patterns.md
sed -i 's/CREATE TABLE orders/CREATE TABLE tb_order/g' docs/advanced/database-patterns.md
sed -i 's/CREATE TABLE categories/CREATE TABLE tb_category/g' docs/advanced/database-patterns.md
sed -i 's/categories_ltree/tv_category_tree/g' docs/advanced/database-patterns.md
sed -i 's/FROM orders\b/FROM tb_order/g' docs/advanced/database-patterns.md
sed -i 's/FROM categories\b/FROM tb_category/g' docs/advanced/database-patterns.md
sed -i 's/JOIN orders\b/JOIN tb_order/g' docs/advanced/database-patterns.md
sed -i 's/JOIN categories\b/JOIN tb_category/g' docs/advanced/database-patterns.md

# File 2: multi-tenancy.md
sed -i 's/CREATE TABLE users/CREATE TABLE tb_user/g' docs/advanced/multi-tenancy.md
sed -i 's/CREATE TABLE orders/CREATE TABLE tb_order/g' docs/advanced/multi-tenancy.md
sed -i 's/REFERENCES users(/REFERENCES tb_user(/g' docs/advanced/multi-tenancy.md
sed -i 's/REFERENCES orders(/REFERENCES tb_order(/g' docs/advanced/multi-tenancy.md
sed -i 's/FROM users\b/FROM tb_user/g' docs/advanced/multi-tenancy.md
sed -i 's/FROM orders\b/FROM tb_order/g' docs/advanced/multi-tenancy.md
sed -i 's/JOIN users\b/JOIN tb_user/g' docs/advanced/multi-tenancy.md
sed -i 's/JOIN orders\b/JOIN tb_order/g' docs/advanced/multi-tenancy.md

# File 3: bounded-contexts.md
sed -i 's/orders\.orders/orders.tb_order/g' docs/advanced/bounded-contexts.md
sed -i 's/orders\.categories/orders.tb_category/g' docs/advanced/bounded-contexts.md
sed -i 's/inventory\.products/inventory.tb_product/g' docs/advanced/bounded-contexts.md
```

**Option B: Using Edit tool (for careful one-by-one replacements)**

Use the Edit tool to replace each occurrence, checking context before replacement.

---

### Step 4: Verification (Claude)

**Count remaining issues (should be 0 or near-0):**
```bash
# Should find 0 instances of old naming
echo "=== database-patterns.md ==="
grep -c "CREATE TABLE orders\|CREATE TABLE categories\|categories_ltree" docs/advanced/database-patterns.md || echo "0 issues found"

echo "=== multi-tenancy.md ==="
grep -c "CREATE TABLE users\|CREATE TABLE orders" docs/advanced/multi-tenancy.md || echo "0 issues found"

echo "=== bounded-contexts.md ==="
grep -c "orders\.orders\|orders\.categories" docs/advanced/bounded-contexts.md || echo "0 issues found"
```

**Verify new naming exists:**
```bash
echo "=== Verify tb_ tables ==="
grep -c "CREATE TABLE tb_" docs/advanced/*.md

echo "=== Verify v_ views ==="
grep -c "CREATE VIEW v_" docs/advanced/*.md
```

**Manual spot-check (Claude reviews 3-5 examples):**
```bash
# Show context around changes
grep -A 5 "CREATE TABLE tb_order" docs/advanced/database-patterns.md | head -20
grep -A 5 "CREATE TABLE tb_user" docs/advanced/multi-tenancy.md | head -20
grep -A 3 "orders.tb_order" docs/advanced/bounded-contexts.md | head -10
```

---

### Step 5: Quality Check (Claude)

**Acceptance criteria check:**
- [ ] All 3 files use `tb_*`, `v_*`, `tv_*` naming (verify with grep)
- [ ] Multi-tenancy RLS examples reference base tables correctly (spot-check)
- [ ] Bounded context examples show proper schema qualification (spot-check)
- [ ] No instances of simple table names (verify with grep showing 0)
- [ ] Code examples still valid SQL (review CREATE TABLE/VIEW blocks)
- [ ] Follows style guide (review formatting)

**If issues found:**
- Small fixes (1-2 instances): Claude fixes directly with Edit tool
- Large issues (pattern missed many cases): Re-run local model with corrected pattern

---

## Success Metrics

**For local model execution:**
- **Pattern success rate:** >95% (sed commands should catch all instances)
- **Manual fixes needed:** <5 instances
- **Time savings:** 40 minutes vs 10 hours (96% faster)
- **Cost savings:** $0.00 vs ~$3-5 (Claude tokens)

**Quality:**
- Same quality as manual work (pattern-based replacements are deterministic)
- Lower risk of human error (missing instances)

---

**Deadline:** End of Week 1

**End of Work Package WP-005**
