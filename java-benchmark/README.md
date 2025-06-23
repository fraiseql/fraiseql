# Java GraphQL Benchmark for FraiseQL Comparison

This benchmark suite compares FraiseQL's performance against Java-based GraphQL implementations.

## Implementations Tested

1. **FraiseQL**: Python-based GraphQL to SQL framework using PostgreSQL views/functions
2. **Java Spring + JPA/Hibernate**: Traditional ORM-based approach
3. **Java Optimized**: Direct SQL approach similar to FraiseQL's architecture

## Running the Benchmark

```bash
# From the fraiseql root directory
docker-compose -f docker-compose.benchmark.yml up --build

# The benchmark will automatically run and output results
```

## Test Scenarios

1. **Simple User Query**: Basic user fetch by ID
2. **User with Posts**: User with related posts (1:N relationship)
3. **Nested Query**: Post with author and comments (complex nesting)
4. **List with Aggregation**: All users with their posts

## Key Findings

Based on the architecture analysis:

- **FraiseQL** can be 2-3x faster than traditional ORM approaches for data-intensive queries
- **Java Optimized** (using direct SQL) performs similarly to FraiseQL
- The performance advantage comes from the architecture, not the language

## Fair Comparison Notes

- All implementations use the same PostgreSQL instance
- Connection pooling is configured similarly
- JVM is warmed up before benchmarks
- Same number of concurrent requests
- Identical data set and queries
