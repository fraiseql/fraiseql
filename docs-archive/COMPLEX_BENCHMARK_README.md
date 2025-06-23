# FraiseQL Complex Domain Benchmark

## Overview

This benchmark suite tests FraiseQL's performance on complex, deeply nested domain models and mutations - areas where FraiseQL's architecture should excel compared to traditional GraphQL frameworks.

## Benchmark Categories

### 1. Simple Queries (Baseline)
- Basic organization queries with aggregated counts
- Tests fundamental query performance

### 2. Complex Nested Queries (FraiseQL's Strength)
- **Organization Hierarchy**: 4 levels deep (org → dept → team → employees)
- **Project Deep**: Projects with department, organization, lead, and analytics
- **Project Full Details**: Ultra-complex 5+ level nesting with:
  - Department & Organization info
  - Team members with details
  - Recent tasks with comments
  - Time tracking analytics
  - Document revisions

### 3. Mutations
- **Create Project**: Complex project creation with audit logging
- **Assign Employee**: Team member assignment
- **Update Task Status**: Status updates with audit trail
- **Batch Create Tasks**: Bulk task creation

## Setup

```bash
# 1. Setup the complex benchmark environment
./setup_complex_benchmark.sh

# 2. Wait for initialization (complex schema takes ~20 seconds)

# 3. Run the benchmark
./benchmark_complex_domain.py
```

## Database Schema

The complex schema includes:
- **Organizations** → Departments → Teams → Employees
- **Projects** with milestones, dependencies, and team members
- **Tasks** with comments (nested), time entries, and attachments
- **Documents** with revision history
- **Audit Log** for all mutations

### Data Volume
- 3 Organizations
- 6 Departments
- 5 Teams
- ~100 Employees with skills and certifications
- ~60 Projects with complex relationships
- ~1,500 Tasks with assignments
- ~3,000 Task comments (nested)
- Time entries for tracking

## Key Features Tested

### 1. Deep Nesting Performance
FraiseQL should excel at queries like:
```graphql
{
  organizationsHierarchy {
    departments {
      teams {
        employees {
          skills
          certifications
        }
      }
    }
  }
}
```

### 2. Complex Aggregations
- Employee counts per team
- Task completion rates
- Time tracking analytics
- Budget rollups

### 3. JSONB Performance
- Skills arrays
- Milestone objects
- Metadata fields
- Nested comment reactions

### 4. Mutation Performance
- Transactional consistency
- Audit logging
- Cache invalidation
- Batch operations

## Expected Results

### Where FraiseQL Should Excel:
1. **Deep Nested Queries**: 2-5x faster due to optimized SQL generation
2. **Complex Aggregations**: Single SQL query vs. multiple round trips
3. **JSONB Operations**: Native PostgreSQL JSONB support
4. **Batch Operations**: Efficient bulk inserts/updates

### Performance Characteristics:
- **Simple Queries**: Similar performance (both optimized)
- **Complex Queries**: FraiseQL 2-5x faster
- **Mutations**: FraiseQL slightly faster due to direct SQL
- **Memory Usage**: FraiseQL more efficient (no N+1 queries)

## Monitoring

Check performance stats:
```bash
# Overall stats
curl http://localhost:8000/benchmark/stats | jq .

# Connection pool usage
curl http://localhost:8000/pools/stats | jq .

# Cache statistics
curl http://localhost:8000/cache/stats | jq .
```

## Optimization Notes

### FraiseQL Optimizations:
- Projection tables for complex aggregations (tv_organization_full, tv_project_deep)
- Multi-tier connection pooling (read/write/hot)
- JSONB indexes on frequently queried fields
- L1 in-memory cache with 5000 entry capacity
- Prepared statement caching

### PostgreSQL Optimizations:
- work_mem increased to 32MB for complex joins
- join_collapse_limit set to 12
- Specialized JSONB indexes
- Materialized views for complex queries

## Troubleshooting

### If services don't start:
```bash
# Check logs
docker-compose -f docker-compose.complex.yml logs

# Restart specific service
docker-compose -f docker-compose.complex.yml restart fraiseql-complex
```

### If queries are slow:
1. Check connection pool saturation
2. Verify indexes are created
3. Check cache hit rates
4. Monitor PostgreSQL query performance

## Clean Up

```bash
# Stop all containers
docker-compose -f docker-compose.complex.yml down

# Remove volumes (database data)
docker-compose -f docker-compose.complex.yml down -v
```
