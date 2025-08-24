-- Blog Demo Enterprise Custom Types
-- PostgreSQL enums and custom types for domain modeling in multi-tenant SaaS

-- Organization status enumeration
CREATE TYPE organization_status AS ENUM ('active', 'suspended', 'cancelled', 'trial');

-- Subscription plan enumeration  
CREATE TYPE subscription_plan AS ENUM ('starter', 'professional', 'enterprise', 'custom');

-- User role enumeration (enhanced for multi-tenant)
CREATE TYPE user_role AS ENUM ('platform_admin', 'org_admin', 'editor', 'author', 'user', 'guest');

-- Post status enumeration
CREATE TYPE post_status AS ENUM ('draft', 'published', 'archived', 'deleted');

-- Comment status enumeration
CREATE TYPE comment_status AS ENUM ('pending', 'approved', 'rejected', 'spam');

-- Mutation result type for standardized responses
CREATE TYPE mutation_result AS (
    success BOOLEAN,
    message TEXT,
    object_data JSONB,
    error_code TEXT,
    metadata JSONB
);