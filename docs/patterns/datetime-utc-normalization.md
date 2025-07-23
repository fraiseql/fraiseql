# DateTime UTC Normalization Pattern

FraiseQL supports UTC normalization for DateTime fields at the database view level, ensuring consistent timezone handling with JavaScript-friendly 'Z' suffix format.

## Database View Pattern with Z Suffix

When creating views with timestamp fields, convert them to UTC and format with 'Z' suffix:

```sql
-- Basic pattern: Convert timestamp to UTC with Z suffix
CREATE OR REPLACE VIEW user_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        -- Convert to UTC and format with Z suffix
        'createdAt', to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
        'updatedAt', to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
        'lastLoginAt', to_char(last_login_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    ) AS data
FROM users;
```

## PostgreSQL Format Patterns

For ISO 8601 format with Z suffix, use these format patterns:

```sql
-- With milliseconds (recommended for precision)
to_char(timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
-- Output: 2025-01-15T14:30:45.123Z

-- Without milliseconds
to_char(timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
-- Output: 2025-01-15T14:30:45Z

-- With microseconds (if needed)
to_char(timestamp AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')
-- Output: 2025-01-15T14:30:45.123456Z
```

## Complete Example

Here's a complete example showing proper UTC normalization with Z suffix:

```sql
CREATE OR REPLACE VIEW post_view AS
SELECT
    p.id,
    p.author_id,  -- For filtering
    p.is_published,  -- For filtering
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'isPublished', p.is_published,
        
        -- UTC normalized timestamps with Z suffix
        'createdAt', to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
        'updatedAt', to_char(p.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
        'publishedAt', CASE 
            WHEN p.published_at IS NOT NULL 
            THEN to_char(p.published_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
            ELSE NULL 
        END,
        
        -- Nested author with UTC timestamps
        'author', (
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'createdAt', to_char(u.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
            )
            FROM users u
            WHERE u.id = p.author_id
        )
    ) AS data
FROM posts p;
```

## Helper Function for Reusability

Create a helper function to make the conversion consistent:

```sql
CREATE OR REPLACE FUNCTION to_utc_z(ts timestamptz) 
RETURNS text AS $$
BEGIN
    RETURN to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Usage in views becomes cleaner
CREATE OR REPLACE VIEW user_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'createdAt', to_utc_z(created_at),
        'updatedAt', to_utc_z(updated_at)
    ) AS data
FROM users;
```

## Handling NULL Values

For nullable timestamp fields, handle NULL cases explicitly:

```sql
-- Using CASE expression
'deletedAt', CASE 
    WHEN deleted_at IS NOT NULL 
    THEN to_char(deleted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
    ELSE NULL 
END

-- Or with the helper function
CREATE OR REPLACE FUNCTION to_utc_z(ts timestamptz) 
RETURNS text AS $$
BEGIN
    IF ts IS NULL THEN
        RETURN NULL;
    END IF;
    RETURN to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

## Benefits

1. **JavaScript Compatibility**: The 'Z' suffix is the preferred format for JavaScript's Date constructor
2. **Consistency**: All DateTime values use the same format
3. **Performance**: Conversion happens at the database level
4. **Standards Compliance**: ISO 8601 recommends 'Z' for UTC

## Testing

Test your views to ensure proper formatting:

```sql
-- Insert test data with different timezones
INSERT INTO posts (title, created_at) 
VALUES 
    ('UTC Test', '2025-01-15 12:00:00+00:00'::timestamptz),
    ('EST Test', '2025-01-15 12:00:00-05:00'::timestamptz),
    ('CET Test', '2025-01-15 12:00:00+01:00'::timestamptz);

-- Query through view - all should show Z suffix
SELECT data->>'title', data->>'createdAt' FROM post_view;
-- Results:
-- UTC Test | 2025-01-15T12:00:00.000Z
-- EST Test | 2025-01-15T17:00:00.000Z  (converted to UTC)
-- CET Test | 2025-01-15T11:00:00.000Z  (converted to UTC)
```

## Migration Example

For existing views, migrate gradually:

```sql
-- Step 1: Create new function
CREATE OR REPLACE FUNCTION to_utc_z(ts timestamptz) 
RETURNS text AS $$
BEGIN
    IF ts IS NULL THEN RETURN NULL; END IF;
    RETURN to_char(ts AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Step 2: Update your views
CREATE OR REPLACE VIEW user_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        -- Old format (PostgreSQL default)
        -- 'createdAt', created_at
        
        -- New format with Z suffix
        'createdAt', to_utc_z(created_at),
        'updatedAt', to_utc_z(updated_at)
    ) AS data
FROM users;
```

## Performance Considerations

The `to_char` function is very fast, but for high-volume queries, consider:

1. **Materialized Views**: For read-heavy workloads
2. **Generated Columns**: Store pre-formatted timestamps
3. **Indexing**: Index the original timestamp columns, not the formatted output

```sql
-- Example with generated column (PostgreSQL 12+)
ALTER TABLE posts 
ADD COLUMN created_at_utc text 
GENERATED ALWAYS AS (
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
) STORED;
```

## Client-Side Benefits

With Z-suffix timestamps from the database:

```javascript
// Direct parsing - no timezone confusion
const date = new Date(post.createdAt);  // "2025-01-15T12:00:00.000Z"

// Comparison is straightforward
const isRecent = new Date(post.createdAt) > new Date(Date.now() - 86400000);

// No need for timezone libraries
console.log(post.createdAt); // Already in standard format
```