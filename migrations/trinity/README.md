# Trinity Identifiers Migration

This directory contains SQL migrations for implementing the Trinity Identifiers pattern.

## What is Trinity?

Three-tier ID system for every entity:
- `pk_*` (SERIAL) - Internal primary key for fast joins
- `id` (UUID) - Public API identifier (secure, no enumeration)
- `identifier` (TEXT) - Human-readable URL slug (optional, for some entities)

## Migration Order

Apply migrations in numerical order:

1. `001_add_trinity_template.sql` - Template/example migration
2. Entity-specific migrations (create per entity as needed)

## How to Create a Trinity Migration

Use the template in `001_add_trinity_template.sql` and customize:

```sql
-- Replace 'entity_name' with your entity (users, posts, etc.)
-- Adjust backfill logic based on your data
```

## Performance Considerations

**Before/After**: Benchmark your specific workload to measure performance gains.

**Expected benefits**:
- Smaller index size (4 bytes vs 16 bytes)
- Better cache locality for joins
- Faster index lookups (integer comparison vs UUID)

**Trade-offs**:
- One-time migration effort
- Must keep both pk_* and id during transition
- Need to update foreign key references

## Testing

Run tests after each migration:
```bash
# Test migration (up)
psql $DATABASE_URL -f migrations/trinity/00X_migration.sql

# Verify data integrity
psql $DATABASE_URL -c "SELECT COUNT(*) FROM entity WHERE pk_entity IS NULL"
psql $DATABASE_URL -c "SELECT COUNT(*) FROM entity WHERE id IS NULL"

# Test queries using new pk_* columns
# Benchmark against old UUID queries
```

## Rollback

Keep UUID foreign keys until verified, then drop in a separate migration:

```sql
-- After verification
ALTER TABLE dependent_table DROP COLUMN uuid_fk;
```
