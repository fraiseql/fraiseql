# Golden Patterns Reference

**Source:** `examples/blog_api/` - Complete Trinity + CQRS implementation
**Status:** ✅ Fully compliant with all rules
**Use Case:** Enterprise blog with users, posts, and comments

## Complete Trinity Table Pattern

### tb_user (Leaf Entity - Not Referenced by Others)

```sql
-- From: examples/blog_api/db/0_schema/01_write/011_tb_user.sql
CREATE TABLE IF NOT EXISTS tb_user (
    -- Trinity Identifiers (Rule TR-001, TR-002, TR-003)
    pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- ✅ TR-001: INTEGER GENERATED ALWAYS
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,         -- ✅ TR-002: UUID with DEFAULT and UNIQUE
    identifier TEXT UNIQUE NOT NULL,                           -- ✅ TR-003: TEXT UNIQUE (optional but recommended)

    -- Business fields
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    bio TEXT,
    avatar_url TEXT,
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],

    -- Audit fields (standard pattern)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_user IS 'Users table with Trinity pattern: pk_user (INT, internal), id (UUID, public API), identifier (TEXT, username)';
COMMENT ON COLUMN tb_user.pk_user IS 'Internal primary key (INT) for fast database joins - NOT exposed in GraphQL';
COMMENT ON COLUMN tb_user.id IS 'Public UUID identifier for GraphQL API - secure, prevents enumeration';
COMMENT ON COLUMN tb_user.identifier IS 'Human-readable username for SEO-friendly URLs';

-- Indexes (Rule: Index all three identifiers)
CREATE INDEX idx_tb_user_id ON tb_user(id);                    -- ✅ API lookups (UUID)
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);    -- ✅ URL lookups (TEXT)
CREATE INDEX idx_tb_user_email ON tb_user(email);              -- Business field index
CREATE INDEX idx_tb_user_active ON tb_user(is_active);         -- Query optimization
```

### tb_post (Referenced Entity - JOINs to tb_user)

```sql
-- From: examples/blog_api/db/0_schema/01_write/012_tb_post.sql
CREATE TABLE IF NOT EXISTS tb_post (
    -- Trinity Identifiers
    pk_post INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- ✅ TR-001
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,         -- ✅ TR-002
    identifier TEXT UNIQUE NOT NULL,                           -- ✅ TR-003

    -- Foreign Key (Rule FK-001, FK-002)
    fk_user INTEGER NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,  -- ✅ FK-001: References pk_user, ✅ FK-002: INTEGER type

    -- Post data
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) NOT NULL UNIQUE,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT ARRAY[]::TEXT[],
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMPTZ,
    view_count INTEGER DEFAULT 0,

    -- Metadata
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

COMMENT ON TABLE tb_post IS 'Posts table with Trinity pattern: pk_post (INT, internal), id (UUID, public API), identifier (TEXT, slug)';
COMMENT ON COLUMN tb_post.fk_user IS 'Foreign key to tb_user.pk_user (INT) - 10x faster than UUID joins';

-- Indexes
CREATE INDEX idx_tb_post_id ON tb_post(id);
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_user ON tb_post(fk_user);        -- ✅ FK index for JOIN performance
CREATE INDEX idx_tb_post_slug ON tb_post(slug);
CREATE INDEX idx_tb_post_published ON tb_post(is_published);
CREATE INDEX idx_tb_post_created ON tb_post(created_at);
CREATE INDEX idx_tb_post_tags ON tb_post USING gin(tags);    -- ✅ GIN index for array operations
```

## Complete JSONB View Patterns

### v_user (Leaf View - Not Referenced by Other Views)

```sql
-- From: examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    u.id,  -- ✅ VW-001: Direct id column for WHERE filtering
    jsonb_build_object(
        'id', u.id::text,              -- ✅ VW-005: id in JSONB
        'identifier', u.identifier,     -- Human-readable
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatar_url', u.avatar_url,
        'is_active', u.is_active,
        'roles', u.roles,
        'created_at', u.created_at,
        'updated_at', u.updated_at
        -- ✅ VW-003: No pk_user in JSONB (internal only)
    ) AS data  -- ✅ VW-004: data column with JSONB
FROM tb_user u;
```

**Why no pk_user in SELECT?**
- This view is NOT referenced by other views (VW-002)
- No other views JOIN to v_user
- Therefore, pk_user is not needed as a direct column

### v_post (Referenced View - JOINs from v_comment)

```sql
-- From: examples/blog_api/db/0_schema/02_read/022_post/0221_v_post.sql
CREATE OR REPLACE VIEW v_post AS
SELECT
    p.id,  -- ✅ VW-001: Direct id column for WHERE filtering
    -- Note: No pk_post here because v_comment JOINs using id, not pk_post
    jsonb_build_object(
        'id', p.id::text,              -- ✅ VW-005: id in JSONB
        'identifier', p.identifier,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'is_published', p.is_published,
        'published_at', p.published_at,
        'view_count', p.view_count,
        'created_at', p.created_at,
        'updated_at', p.updated_at,
        'author', vu.data  -- ✅ Nested JSONB from v_user
    ) AS data  -- ✅ VW-004: data column
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_user  -- ✅ FK-001: JOIN on pk_user (INTEGER)
JOIN v_user vu ON vu.id = u.id;          -- ✅ JOIN views on id (UUID)
```

**Why no pk_post in SELECT?**
- v_comment JOINs to v_post using `vp.id = p.id` (UUID)
- No view JOINs to v_post using pk_post
- Therefore, pk_post not needed (VW-002)

### v_comment (Complex View with Recursive CTE)

```sql
-- From: examples/blog_api/db/0_schema/02_read/023_comment/0231_v_comment.sql
CREATE OR REPLACE VIEW v_comment AS
WITH RECURSIVE comment_tree AS (
    -- CTE for hierarchical comment threading
    SELECT
        c.pk_comment, c.id, c.identifier, c.content, c.is_edited,
        c.fk_post, c.fk_user, c.fk_parent_comment,
        c.created_at, c.updated_at,
        0 AS depth,
        ARRAY[c.pk_comment] AS path
    FROM tb_comment c
    WHERE c.fk_parent_comment IS NULL

    UNION ALL

    SELECT
        c.pk_comment, c.id, c.identifier, c.content, c.is_edited,
        c.fk_post, c.fk_user, c.fk_parent_comment,
        c.created_at, c.updated_at,
        ct.depth + 1,
        ct.path || c.pk_comment
    FROM tb_comment c
    JOIN comment_tree ct ON ct.pk_comment = c.fk_parent_comment
    WHERE NOT c.pk_comment = ANY(ct.path)
)
SELECT
    c.id,  -- ✅ VW-001: Direct id column
    jsonb_build_object(
        'id', c.id::text,              -- ✅ VW-005: id in JSONB
        'identifier', c.identifier,
        'content', c.content,
        'is_edited', c.is_edited,
        'depth', c.depth,
        'created_at', c.created_at,
        'updated_at', c.updated_at,
        'post', vp.data,               -- ✅ Nested JSONB
        'author', vu.data,             -- ✅ Nested JSONB
        'parent_comment', CASE
            WHEN pc.id IS NOT NULL THEN jsonb_build_object(
                'id', pc.id::text,
                'content', pc.content,
                'author', vu_pc.data
            )
            ELSE NULL
        END
    ) AS data  -- ✅ VW-004: data column
FROM comment_tree c
JOIN tb_post p ON p.pk_post = c.fk_post      -- ✅ JOIN on pk_post (INTEGER)
JOIN tb_user u ON u.pk_user = c.fk_user      -- ✅ JOIN on pk_user (INTEGER)
LEFT JOIN tb_comment pc ON pc.pk_comment = c.fk_parent_comment
LEFT JOIN tb_user pu ON pu.pk_user = pc.fk_user
JOIN v_post vp ON vp.id = p.id               -- ✅ View JOIN on id (UUID)
JOIN v_user vu ON vu.id = u.id               -- ✅ View JOIN on id (UUID)
LEFT JOIN v_user vu_pc ON vu_pc.id = pu.id;
```

## Mutation Function Patterns

### Create User - Simple vs Advanced Patterns (Rule MF-001, MF-002)

**Advanced Pattern (Structured JSONB Response):**
```sql
-- From: examples/ecommerce_api/db/0_schema/03_functions/030_customer_functions/0301_create_customer.sql
CREATE OR REPLACE FUNCTION app.create_customer(
    input_payload JSONB
) RETURNS JSONB AS $$  -- ✅ Advanced: Full JSONB response
DECLARE
    v_customer_id UUID;
BEGIN
    -- Delegate to core business logic
    v_customer_id := core.create_customer(
        input_payload->>'email',
        input_payload->>'password_hash',
        input_payload->>'first_name',
        input_payload->>'last_name'
    );

    -- Return structured JSONB response with success/error handling
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer created successfully',
        jsonb_build_object(
            'customer', jsonb_build_object(
                'id', v_customer_id,
                'email', input_payload->>'email',
                'first_name', input_payload->>'first_name',
                'last_name', input_payload->>'last_name'
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Simple Pattern (Essential Data Only):**
```sql
-- From: examples/ecommerce_api/db/0_schema/03_functions/030_customer_functions/0301_create_customer.sql
CREATE OR REPLACE FUNCTION core.create_customer(
    customer_email VARCHAR(255),
    customer_password_hash VARCHAR(255),
    customer_first_name VARCHAR(100) DEFAULT NULL,
    customer_last_name VARCHAR(100) DEFAULT NULL
) RETURNS UUID AS $$  -- ✅ Simple: Just the UUID
DECLARE
    new_customer_id UUID;
BEGIN
    -- Business logic validation
    IF customer_email IS NULL OR customer_password_hash IS NULL THEN
        RAISE EXCEPTION 'Email and password are required';
    END IF;

    -- Create customer
    new_customer_id := gen_random_uuid();
    INSERT INTO customers (id, email, password_hash, first_name, last_name)
    VALUES (new_customer_id, customer_email, customer_password_hash, customer_first_name, customer_last_name);

    -- Sync projection tables (Rule MF-002)
    PERFORM app.sync_tv_customer();

    RETURN new_customer_id;
END;
$$ LANGUAGE plpgsql;
```

**Core Layer Function (Simple Type):**
```sql
-- From: examples/ecommerce_api/db/0_schema/03_functions/030_customer_functions/0301_create_customer.sql
CREATE OR REPLACE FUNCTION core.create_customer(
    customer_email VARCHAR(255),
    customer_password_hash VARCHAR(255),
    customer_first_name VARCHAR(100) DEFAULT NULL,
    customer_last_name VARCHAR(100) DEFAULT NULL
) RETURNS UUID AS $$  -- ✅ Core layer returns simple type
DECLARE
    new_customer_id UUID;
BEGIN
    -- Business logic validation
    IF customer_email IS NULL OR customer_password_hash IS NULL THEN
        RAISE EXCEPTION 'Email and password are required';
    END IF;

    -- Create customer
    new_customer_id := gen_random_uuid();
    INSERT INTO customers (id, email, password_hash, first_name, last_name)
    VALUES (new_customer_id, customer_email, customer_password_hash, customer_first_name, customer_last_name);

    -- Sync projection tables (Rule MF-002)
    PERFORM app.sync_tv_customer();

    RETURN new_customer_id;
END;
$$ LANGUAGE plpgsql;
```

### Update User

```sql
CREATE OR REPLACE FUNCTION update_user(
    user_id UUID,
    new_name TEXT DEFAULT NULL,
    new_bio TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
BEGIN
    UPDATE tb_user
    SET
        name = COALESCE(new_name, name),
        bio = COALESCE(new_bio, bio),
        updated_at = NOW()
    WHERE id = user_id;

    -- Sync Trinity table (Rule MF-002)
    PERFORM sync_tv_user();

    RETURN true;
END;
$$ LANGUAGE plpgsql;
```

## Helper Function Patterns

### Core Helper Functions (Rule HF-001)

```sql
-- From: examples/blog_api/db/functions/core_functions.sql
-- These would follow the pattern: core.get_pk_<entity>(...) RETURNS INTEGER
-- and core.get_<entity>_id(...) RETURNS UUID

-- Example pattern (not in blog_api, but follows rules):
CREATE FUNCTION core.get_pk_user(p_user_id UUID)
RETURNS INTEGER AS $$
BEGIN
    RETURN (SELECT pk_user FROM tb_user WHERE id = p_user_id);
END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION core.get_user_id(p_pk_user INTEGER)
RETURNS UUID AS $$
BEGIN
    RETURN (SELECT id FROM tb_user WHERE pk_user = p_pk_user);
END;
$$ LANGUAGE plpgsql;
```

## Variable Naming Patterns (Rule HF-002)

```sql
-- Correct variable naming in functions
CREATE FUNCTION app.fn_create_post(p_input_data JSONB)
RETURNS JSONB AS $$
DECLARE
    v_user_id UUID;        -- ✅ v_<entity>_id for resolved UUID
    v_user_pk INTEGER;     -- ✅ v_<entity>_pk for resolved INTEGER pk
    v_post_id UUID;        -- ✅ v_<entity>_id for new entity
    p_user_ids UUID[];     -- ✅ p_<entity>_ids for parameter arrays
BEGIN
    -- Extract from input
    v_user_id := (p_input_data->>'user_id')::UUID;

    -- Resolve to internal pk
    v_user_pk := core.get_pk_user(v_user_id);

    -- Create post
    INSERT INTO tb_post (fk_user, title, content)
    VALUES (v_user_pk, p_input_data->>'title', p_input_data->>'content')
    RETURNING id INTO v_post_id;

    -- Return success structure
    RETURN jsonb_build_object(
        'success', true,
        'post_id', v_post_id
    );
END;
$$ LANGUAGE plpgsql;
```

## Common Mistakes to Avoid

### ❌ Wrong: SERIAL instead of GENERATED ALWAYS

```sql
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,  -- ❌ Deprecated pattern
    ...
);
```

### ❌ Wrong: UUID Foreign Keys

```sql
CREATE TABLE tb_post (
    fk_user UUID REFERENCES tb_user(id),  -- ❌ Slow UUID FK
    ...
);
```

### ❌ Wrong: Exposing pk_* in JSONB

```sql
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'pk_user', pk_user,  -- ❌ NEVER expose pk_* in JSONB!
        'name', name
    ) as data
FROM tb_user;
```

### ❌ Wrong: Missing sync calls

```sql
CREATE FUNCTION fn_create_user(...) RETURNS JSONB AS $$
BEGIN
    INSERT INTO tb_user (...) VALUES (...);
    -- ❌ Missing: PERFORM fn_sync_tv_user();
    RETURN jsonb_build_object('success', true);
END;
$$;
```

### ❌ Wrong: Bad variable naming

```sql
DECLARE
    userId UUID;        -- ❌ camelCase, should be v_user_id
    user_pk INTEGER;    -- ❌ Missing v_ prefix
    ids UUID[];         -- ❌ Not descriptive enough
```

## Copy-Paste Templates

### Trinity Table Template

```sql
CREATE TABLE tb_{entity} (
    -- Trinity Identifiers
    pk_{entity} INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    identifier TEXT UNIQUE,

    -- Foreign Keys (use INTEGER, reference pk_*)
    fk_{parent} INTEGER REFERENCES tb_{parent}(pk_{parent}),

    -- Business fields
    -- ...

    -- Audit fields
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_tb_{entity}_id ON tb_{entity}(id);
CREATE INDEX idx_tb_{entity}_identifier ON tb_{entity}(identifier);
CREATE INDEX idx_tb_{entity}_fk_{parent} ON tb_{entity}(fk_{parent});
```

### JSONB View Template

```sql
CREATE VIEW v_{entity} AS
SELECT
    e.id,  -- Direct column for WHERE filtering
    -- Include pk_{entity} here ONLY if other views JOIN to this view
    jsonb_build_object(
        'id', e.id::text,
        'identifier', e.identifier,
        -- Business fields...
        -- NEVER include pk_* fields here
        -- Include nested data from other views
        '{child_entity}', ve.data
    ) AS data
FROM tb_{entity} e
-- JOIN to parent tables using pk_* (INTEGER)
JOIN tb_{parent} p ON p.pk_{parent} = e.fk_{parent}
-- JOIN to parent views using id (UUID)
JOIN v_{parent} vp ON vp.id = p.id;
```

### Mutation Function Templates

**Advanced Template (Structured JSONB Response):**
```sql
CREATE FUNCTION app.create_{entity}(input_payload JSONB)
RETURNS JSONB AS $$
DECLARE
    v_{entity}_id UUID;
BEGIN
    -- Delegate to core business logic
    v_{entity}_id := core.create_{entity}(
        input_payload->>'field1',
        input_payload->>'field2'
    );

    -- Return structured response with success/error handling
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        '{Entity} created successfully',
        jsonb_build_object(
            '{entity}', jsonb_build_object(
                'id', v_{entity}_id,
                'field1', input_payload->>'field1'
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Simple Template (Essential Data Only):**
```sql
CREATE FUNCTION core.create_{entity}(
    p_field1 TEXT,
    p_field2 TEXT
) RETURNS UUID AS $$
DECLARE
    v_{entity}_id UUID;
BEGIN
    -- Validation logic
    IF p_field1 IS NULL THEN
        RAISE EXCEPTION 'field1 is required';
    END IF;

    -- Insert into tb_{entity}
    INSERT INTO tb_{entity} (field1, field2)
    VALUES (p_field1, p_field2)
    RETURNING id INTO v_{entity}_id;

    -- CRITICAL: Sync tv_* table
    PERFORM app.sync_tv_{entity}();

    RETURN v_{entity}_id;
END;
$$ LANGUAGE plpgsql;
```
