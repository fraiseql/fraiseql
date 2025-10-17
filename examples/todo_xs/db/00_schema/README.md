# TODO App - XS (Extra Small) Example

**Organization**: Single file schema
**Size**: XS (<100 lines, 2-3 tables)
**Use case**: Prototypes, demos, learning

## Structure

```
db/
└── 00_schema/
    └── schema.sql    # Everything in one file
```

## Features

- ✅ CQRS pattern (tb_* write, v_* read)
- ✅ Trinity pattern (pk_, id, identifier)
- ✅ Simple todo list with users
- ✅ Single file for quick demos

## Load Schema

```bash
psql -d your_db -f 00_schema/schema.sql
```

## When to Use XS

- Quick prototypes
- Learning FraiseQL concepts
- Microservices with 1-3 tables
- Demos and examples

## When to Upgrade to S

When you have:
- More than 3-4 tables
- Need better organization
- Multiple developers
- Production deployment

Upgrade path: Split into `01_write.sql`, `02_read.sql`, `03_functions.sql`
