# Blog Simple - S (Small) Example

**Organization**: Flat, multi-file structure
**Size**: S (<20 files, 3-5 tables)
**Use case**: Small blogs, simple APIs, basic CRUD apps

## Structure

```
db/
└── 00_schema/
    ├── 00_common.sql      # Extensions, types
    ├── 01_write.sql       # tb_user, tb_post, tb_comment (command side)
    ├── 02_read.sql        # v_user, v_post, v_comment (query side)
    ├── 03_functions.sql   # Business logic helpers
    ├── 04_triggers.sql    # Automated behaviors
    ├── 05_indexes.sql     # Performance optimization
    └── 09_finalize.sql    # Permissions, grants
```

## Features

- ✅ **CQRS separation**: Write (01) → Read (02) → Functions (03)
- ✅ **Layer-first organization**: All writes together, all reads together
- ✅ **Trinity pattern**: pk_* (INT), id (UUID), identifier (TEXT)
- ✅ **One concern per file**: Easy to navigate and understand
- ✅ **AI-friendly**: Clear structure, well-commented

## Load Order

Files load in alphabetical order (confiture pattern):
1. `00_common.sql` - Extensions, types
2. `01_write.sql` - Command tables (tb_*)
3. `02_read.sql` - Query views (v_*) - depends on write tables
4. `03_functions.sql` - Business logic
5. `04_triggers.sql` - Automation
6. `05_indexes.sql` - Performance
7. `09_finalize.sql` - Permissions

## Load Schema

```bash
# Using confiture
confiture build --from 00_schema --to your_db

# Or manually (files load in order)
cat 00_schema/*.sql | psql -d your_db

# Or individually
psql -d your_db -f 00_schema/00_common.sql
psql -d your_db -f 00_schema/01_write.sql
# ... etc
```

## Entities

### Write Side (Command - tb_*)
- `tb_user` - Users (normalized)
- `tb_post` - Blog posts (normalized)
- `tb_comment` - Comments with threading (normalized)

### Read Side (Query - v_*)
- `v_user` - User with metadata
- `v_post` - Post with author (denormalized)
- `v_comment` - Comment with author, post, parent (denormalized)

## When to Use S

- Small production apps
- Simple blogs or APIs
- 3-10 tables
- Single developer or small team
- Basic CRUD operations

## When to Upgrade to M

When you have:
- More than 10-15 files
- Multiple contexts/domains
- Need subdirectory organization
- Multiple teams

Upgrade path: Create `01_write/`, `02_read/`, `03_functions/` subdirectories and organize by context
