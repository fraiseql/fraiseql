-- Blog Demo Input Types
-- Following PrintOptim patterns for typed inputs

-- User input type for create/update operations
CREATE TYPE app.type_user_input AS (
    identifier TEXT,
    email TEXT,
    password_hash TEXT,
    role user_role,
    is_active BOOLEAN,
    email_verified BOOLEAN,
    profile JSONB,
    preferences JSONB,
    metadata JSONB
);

-- Post input type for create/update operations
CREATE TYPE app.type_post_input AS (
    identifier TEXT,
    fk_author UUID,
    title TEXT,
    content TEXT,
    excerpt TEXT,
    status post_status,
    featured BOOLEAN,
    published_at TIMESTAMPTZ,
    seo_metadata JSONB,
    custom_fields JSONB,
    tag_ids UUID[]
);

-- Comment input type for create/update operations
CREATE TYPE app.type_comment_input AS (
    fk_post UUID,
    fk_parent UUID,
    fk_author UUID,
    content TEXT,
    status comment_status,
    metadata JSONB
);

-- Tag input type for create/update operations
CREATE TYPE app.type_tag_input AS (
    identifier TEXT,
    name TEXT,
    description TEXT,
    color TEXT,
    metadata JSONB
);

-- Update the mutation result type to match PrintOptim patterns
DROP TYPE IF EXISTS mutation_result CASCADE;
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);
