# Example Template

Use this template to create new FraiseQL examples that are guaranteed to follow the Trinity Pattern.

## 🚀 Quick Start

```bash
# Copy this template
cp -r examples/_TEMPLATE examples/my-example

# Edit the files to implement your example
# Follow the checklist below
# Run verification before submitting
```

## 📋 Trinity Pattern Checklist

**Complete all items before submitting your example.**

### ✅ Tables (Required)

- [ ] **All tables have `pk_<entity> INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY`**
  ```sql
  CREATE TABLE tb_post (
      pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- ✅ Required
      -- ... other columns
  );
  ```

- [ ] **All tables have `id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE`**
  ```sql
  CREATE TABLE tb_post (
      pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
      id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,  -- ✅ Required
      -- ... other columns
  );
  ```

- [ ] **User-facing tables have `identifier TEXT UNIQUE`** (optional but recommended)
  ```sql
  CREATE TABLE tb_post (
      pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
      id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
      identifier TEXT UNIQUE,  -- ✅ Optional for SEO-friendly URLs
      -- ... other columns
  );
  ```

- [ ] **All foreign keys reference `pk_*` columns (INTEGER)**
  ```sql
  CREATE TABLE tb_post (
      -- ✅ Correct: INTEGER FK to pk_user
      fk_user INTEGER REFERENCES tb_user(pk_user),
      -- ❌ Wrong: UUID FK to id
      -- fk_user UUID REFERENCES tb_user(id),
  );
  ```

### ✅ Views (Required)

- [ ] **All views have direct `id` column for WHERE filtering**
  ```sql
  CREATE VIEW v_post AS
  SELECT
      id,  -- ✅ Required for WHERE id = $1
      jsonb_build_object('id', id, 'title', title) AS data
  FROM tb_post;
  ```

- [ ] **JSONB never contains `pk_*` fields** (security!)
  ```sql
  CREATE VIEW v_post AS
  SELECT
      id,
      jsonb_build_object(
          'id', id,
          'title', title
          -- ❌ NEVER: 'pk_post', pk_post
      ) AS data
  FROM tb_post;
  ```

- [ ] **Views include `pk_*` ONLY if other views JOIN to them**
  ```sql
  -- If v_comment JOINs to v_post using pk_post, then include it:
  CREATE VIEW v_post AS
  SELECT
      id,
      pk_post,  -- ✅ Only if needed for JOINs
      jsonb_build_object(...) AS data
  FROM tb_post;
  ```

### ✅ Functions (Required)

- [ ] **Mutations return JSONB** (app layer) or simple types (core layer)
  ```sql
  -- App layer: JSONB response
  CREATE FUNCTION app.create_post(...) RETURNS JSONB AS $$
  BEGIN
      RETURN app.build_mutation_response(true, 'SUCCESS', 'Created', jsonb_build_object(...));
  END;
  $$;

  -- Core layer: Simple type
  CREATE FUNCTION core.create_post(...) RETURNS UUID AS $$
  BEGIN
      RETURN v_post_id;
  END;
  $$;
  ```

- [ ] **Variables follow naming convention**
  ```sql
  CREATE FUNCTION create_post(...) RETURNS JSONB AS $$
  DECLARE
      v_user_id UUID;     -- ✅ v_<entity>_id
      v_user_pk INTEGER;  -- ✅ v_<entity>_pk
      v_post_id UUID;     -- ✅ v_<entity>_id
  BEGIN
      -- ...
  END;
  $$ LANGUAGE plpgsql;
  ```

- [ ] **Mutations call `fn_sync_tv_<entity>()` for projection tables**
  ```sql
  CREATE FUNCTION fn_create_post(...) RETURNS JSONB AS $$
  BEGIN
      INSERT INTO tb_post (...) VALUES (...);
      PERFORM fn_sync_tv_post(v_post_id);  -- ✅ Required for tv_* tables
      RETURN jsonb_build_object('success', true);
  END;
  $$ LANGUAGE plpgsql;
  ```

### ✅ Python Types (Required)

- [ ] **Never expose `pk_*` fields in GraphQL types**
  ```python
  @fraiseql.type(sql_source="v_post", jsonb_column="data")
  class Post:
      id: UUID          # ✅ Public
      title: str        # ✅ Public
      # pk_post: int    # ❌ NEVER expose pk_*
  ```

- [ ] **Types match JSONB view structure exactly**
  ```python
  # If v_post JSONB has: {'id': ..., 'title': ..., 'author': {...}}
  @fraiseql.type(sql_source="v_post", jsonb_column="data")
  class Post:
      id: UUID          # ✅ Matches JSONB
      title: str        # ✅ Matches JSONB
      author: User      # ✅ Matches JSONB nested object
  ```

## 🧪 Verification

**Run verification before submitting:**

```bash
# Verify your example
python .phases/verify-examples-compliance/verify.py examples/my-example/

# Should show: ✅ Compliance: 100%
# If not, fix the violations and re-run
```

**Common issues to check:**
- Foreign keys reference `id` instead of `pk_*`
- Views missing direct `id` column
- JSONB contains `pk_*` fields
- Python types don't match JSONB structure

## 📁 File Structure

```
examples/my-example/
├── README.md              # Your example documentation
├── app.py                 # FastAPI application
├── models.py              # Python GraphQL types
├── db/
│   ├── 0_schema/
│   │   ├── 01_write/      # tb_* tables
│   │   └── 02_read/       # v_* views, tv_* projections
│   └── functions/         # Business logic functions
└── tests/                 # Test files
```

## 🎯 Implementation Steps

1. **Copy this template**
2. **Implement your business logic** following the checklist
3. **Run verification** and fix any violations
4. **Add comprehensive tests**
5. **Update documentation** with your example's purpose
6. **Submit PR** with verification passing

## 📚 Resources

- [Trinity Pattern Guide](../../docs/guides/trinity-pattern-guide.md)
- [Common Mistakes](../../docs/guides/common-mistakes.md)
- [Migration Guide](../../docs/mutations/migration-guide.md)
- [Verification Tools](../../docs/testing/developer-guide.md)

## ✅ Success Criteria

Your example is ready when:
- [ ] Verification passes with 0 errors
- [ ] All checklist items completed
- [ ] Comprehensive documentation
- [ ] Working tests
- [ ] Clear example purpose and use case

**Remember**: Following the Trinity Pattern ensures your example is secure, performant, and maintainable! 🚀
