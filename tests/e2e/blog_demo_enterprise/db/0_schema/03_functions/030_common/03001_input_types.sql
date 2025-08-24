-- Blog Demo Enterprise Input Types
-- PostgreSQL types for function inputs (enhanced for multi-tenancy)

-- Organization input type
CREATE TYPE app.type_organization_input AS (
    name TEXT,
    identifier TEXT,
    contact_email TEXT,
    website_url TEXT,
    subscription_plan subscription_plan,
    settings JSONB,
    limits JSONB
);

-- User input type (enhanced for multi-tenancy)
CREATE TYPE app.type_user_input AS (
    identifier TEXT,
    email TEXT,
    password_hash TEXT,
    role user_role,
    profile JSONB,
    preferences JSONB
);

-- Post input type (enhanced for multi-tenant with JSONB data)
CREATE TYPE app.type_post_input AS (
    title TEXT,
    content TEXT,
    excerpt TEXT,
    status post_status,
    featured BOOLEAN,
    published_at TIMESTAMPTZ,
    seo_metadata JSONB,
    custom_fields JSONB,
    tags TEXT[]
);

-- Comment input type  
CREATE TYPE app.type_comment_input AS (
    content TEXT,
    parent_comment_id UUID,
    metadata JSONB
);

-- Tag input type
CREATE TYPE app.type_tag_input AS (
    name TEXT,
    slug TEXT,
    description TEXT,
    color TEXT
);