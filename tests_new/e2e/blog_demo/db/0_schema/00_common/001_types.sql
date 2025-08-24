-- Blog Demo Custom Types
-- PostgreSQL enums and custom types for domain modeling

-- User role enumeration
CREATE TYPE user_role AS ENUM ('admin', 'moderator', 'author', 'user', 'guest');

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
