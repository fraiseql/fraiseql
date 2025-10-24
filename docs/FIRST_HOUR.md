# Your First Hour with FraiseQL

Welcome! You've just completed the 5-minute quickstart and have a working GraphQL API. Now let's spend the next 55 minutes building your skills progressively. By the end, you'll understand how to extend FraiseQL applications and implement production patterns.

## Minute 0-5: Quickstart Recap

**[Complete the 5-minute quickstart first](quickstart.md)**

You should now have:

- A working GraphQL API at `http://localhost:8000/graphql`
- A PostgreSQL database with a `v_note` view
- A basic note-taking app

✅ **Checkpoint**: Can you run this query and get results?

```graphql
query {
  notes {
    id
    title
    content
  }
}
```

## Minute 5-15: Understanding What You Built

**[Read the Understanding Guide](UNDERSTANDING.md)**

Key concepts you should now understand:

- **Database-first GraphQL**: Start with PostgreSQL, not GraphQL types
- **JSONB Views**: `tb_*` tables → `v_*` views → GraphQL responses
- **CQRS Pattern**: Reads (views) vs Writes (functions)
- **Naming Conventions**: `tb_*`, `v_*`, `fn_*`, `tv_*`

✅ **Checkpoint**: Can you explain why FraiseQL uses JSONB views instead of traditional ORMs?

## Minute 15-30: Extend Your API - Add Tags to Notes

**Challenge**: Add a "tags" feature so notes can be categorized.

### Step 1: Update Database Schema

First, add a tags column to your note table:

```sql
-- Add tags column to tb_note
ALTER TABLE tb_note ADD COLUMN tags TEXT[] DEFAULT '{}';

-- Update sample data
UPDATE tb_note SET tags = ARRAY['work', 'urgent'] WHERE title = 'First Note';
UPDATE tb_note SET tags = ARRAY['personal', 'ideas'] WHERE title = 'Second Note';
```

### Step 2: Update the View

Modify `v_note` to include tags:

```sql
-- Drop and recreate view with tags
DROP VIEW v_note;
CREATE VIEW v_note AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'tags', tags
    ) as data
FROM tb_note;
```

### Step 3: Update Python Type

Add tags to your Note type:

```python
# app.py
from fraiseql import type, query
from typing import List

@type(sql_source="v_note")
class Note:
    id: UUID
    title: str
    content: str
    tags: List[str]  # Add this line
```

### Step 4: Add Filtering with Where Input Types

FraiseQL provides automatic Where input type generation for powerful, type-safe filtering:

```python
# app.py
from fraiseql import query
from fraiseql.sql import create_graphql_where_input

# Generate automatic Where input type for Note
NoteWhereInput = create_graphql_where_input(Note)

@query
async def notes(info, where: NoteWhereInput | None = None) -> List[Note]:
    """Get notes with optional filtering."""
    db = info.context["db"]
    # Use repository's find method with where parameter
    return await db.find("v_note", where=where)
```

### Step 5: Test Your Changes

Restart your server and test the powerful filtering capabilities:

```graphql
query {
  # Get all notes
  notes {
    id
    title
    tags
  }

  # Filter notes by title containing "work"
  workNotes: notes(where: { title: { contains: "work" } }) {
    title
    content
  }

  # Filter notes with specific tag using array contains
  urgentNotes: notes(where: { tags: { contains: "urgent" } }) {
    title
    tags
  }

  # Combine multiple conditions
  complexFilter: notes(where: {
    AND: [
      { title: { contains: "meeting" } },
      { tags: { contains: "work" } }
    ]
  }) {
    title
    content
    tags
  }
}
```

**Available Filter Operators:**
- `eq`, `neq` - equals, not equals
- `contains`, `startswith`, `endswith` - string matching
- `gt`, `gte`, `lt`, `lte` - comparisons
- `in`, `nin` - list membership
- `isnull` - null checking
- `AND`, `OR`, `NOT` - logical operators

✅ **Checkpoint**: Can you create a note with tags and use the various filtering operators?

## Minute 30-45: Add a Mutation - Delete Notes

**Challenge**: Add the ability to delete notes.

### Step 1: Create Delete Function

Create a PostgreSQL function for deletion:

```sql
-- Create delete function
CREATE OR REPLACE FUNCTION fn_delete_note(note_id UUID)
RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_note WHERE id = note_id;
    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
```

### Step 2: Add Python Mutation

Add the mutation to your app:

```python
# app.py
from fraiseql import mutation

@mutation
def delete_note(id: UUID) -> bool:
    """Delete a note by ID."""
    pass  # Framework calls fn_delete_note
```

### Step 3: Test the Mutation

Try this in GraphQL playground:

```graphql
mutation {
  deleteNote(id: "your-note-id-here")
}
```

### Step 4: Handle Errors

Add error handling for non-existent notes:

```sql
-- Improved delete function with error handling
CREATE OR REPLACE FUNCTION fn_delete_note(note_id UUID)
RETURNS JSONB AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM tb_note WHERE id = note_id;
    GET DIAGNOSTICS deleted_count = ROW_COUNT;

    IF deleted_count = 0 THEN
        RETURN jsonb_build_object('success', false, 'error', 'Note not found');
    ELSE
        RETURN jsonb_build_object('success', true);
    END IF;
END;
$$ LANGUAGE plpgsql;
```

Update your Python type:

```python
# app.py
from fraiseql import mutation
from typing import Optional

class DeleteResult:
    success: bool
    error: str | None

@mutation
def delete_note(id: UUID) -> DeleteResult:
    """Delete a note by ID."""
    pass
```

✅ **Checkpoint**: Can you delete a note and handle the case where the note doesn't exist?

## Minute 45-60: Production Patterns - Timestamps

**Challenge**: Add `created_at` and `updated_at` timestamps with automatic updates.

### Step 1: Add Timestamp Columns

```sql
-- Add timestamp columns
ALTER TABLE tb_note ADD COLUMN created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW();
ALTER TABLE tb_note ADD COLUMN updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW();

-- Update existing records
UPDATE tb_note SET created_at = NOW(), updated_at = NOW();
```

### Step 2: Create Update Trigger

```sql
-- Function to update updated_at
CREATE OR REPLACE FUNCTION fn_update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger
CREATE TRIGGER tr_note_updated_at
    BEFORE UPDATE ON tb_note
    FOR EACH ROW
    EXECUTE FUNCTION fn_update_updated_at();
```

### Step 3: Update View

```sql
-- Recreate view with timestamps
DROP VIEW v_note;
CREATE VIEW v_note AS
SELECT
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'tags', tags,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) as data
FROM tb_note;
```

### Step 4: Update Python Type

```python
# app.py
from fraiseql import type
from datetime import datetime

@type(sql_source="v_note")
class Note:
    id: UUID
    title: str
    content: str
    tags: List[str]
    created_at: datetime  # Add this
    updated_at: datetime  # Add this
```

### Step 5: Test Automatic Updates

Create a note, then update it and verify `updated_at` changes but `created_at` stays the same.

✅ **Checkpoint**: Do timestamps update automatically when you modify notes?

## 🎉 Congratulations

You've completed your first hour with FraiseQL! You now know how to:

- ✅ Extend existing APIs with new fields
- ✅ Add filtering capabilities
- ✅ Implement write operations (mutations)
- ✅ Handle errors gracefully
- ✅ Add production-ready features like timestamps

## What's Next?

### Immediate Next Steps (2-3 hours)

- **[Beginner Learning Path](tutorials/beginner-path.md)** - Deep dive into all core concepts
- **[Blog API Tutorial](tutorials/blog-api.md)** - Build a complete application

### Explore Examples (30 minutes each)

- **E-commerce API (../examples/ecommerce/)** - Shopping cart, products, orders
- **Real-time Chat (../examples/real_time_chat/)** - Subscriptions and real-time updates
- **Multi-tenant SaaS (../examples/apq_multi_tenant/)** - Enterprise patterns

### Advanced Topics

- **[Performance Guide](performance/PERFORMANCE_GUIDE.md)** - Optimization techniques
- **[Multi-tenancy](advanced/multi-tenancy.md)** - Building SaaS applications
- **[Migration Guide](migration/v0-to-v1.md)** - Upgrading from older versions

### Need Help?

- **[Troubleshooting Guide](TROUBLESHOOTING.md)** - Common issues and solutions
- **[Quick Reference](reference/quick-reference.md)** - Copy-paste code patterns
- **[GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)** - Community support

---

**Ready for more?** The [Beginner Learning Path](tutorials/beginner-path.md) will take you from here to building production applications! 🚀
