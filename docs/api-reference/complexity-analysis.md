# Query Complexity Analysis

FraiseQL includes a sophisticated query complexity analysis system that helps with:

- **Performance optimization** - Identify expensive queries before execution
- **Cache management** - Intelligently cache queries based on complexity
- **Rate limiting** - Implement query cost-based rate limiting
- **Resource protection** - Prevent denial-of-service through complex queries

## Overview

The complexity analysis system evaluates GraphQL queries based on:

- **Field count** - Number of fields requested
- **Query depth** - Maximum nesting level
- **Array fields** - Potential for large result sets
- **Type diversity** - Number of different types accessed

## Basic Usage

```python
from fraiseql.analysis import analyze_query_complexity

query = """
query GetUser {
  user(id: 1) {
    name
    posts {
      title
      comments {
        text
      }
    }
  }
}
"""

# Analyze the query
score = analyze_query_complexity(query)

print(f"Total score: {score.total_score}")
print(f"Max depth: {score.max_depth}")
print(f"Array fields: {score.array_field_count}")
print(f"Should cache: {score.should_cache()}")
```

## Complexity Configuration

The complexity scoring algorithm is fully configurable through `ComplexityConfig`:

```python
from fraiseql.analysis.complexity_config import ComplexityConfig

# Create custom configuration
config = ComplexityConfig(
    # Field scoring
    base_field_cost=1,  # Cost per field

    # Depth scoring
    depth_multiplier=1.5,  # Exponential factor for depth
    max_depth_penalty=100,  # Cap to prevent overflow

    # Array scoring
    array_field_multiplier=10,  # Base cost for array fields
    array_depth_factor=1.2,  # How depth affects arrays

    # Cache thresholds
    simple_query_threshold=10,
    moderate_query_threshold=50,
    complex_query_threshold=200,
)

# Analyze with custom config
score = analyze_query_complexity(query, config=config)
```

## Preset Configurations

FraiseQL provides three preset configurations for common use cases:

### STRICT_CONFIG

For resource-constrained environments:

```python
from fraiseql.analysis.complexity_config import STRICT_CONFIG

# Higher penalties, lower thresholds
# depth_multiplier=2.0
# array_field_multiplier=15
# complex_query_threshold=150
```

### BALANCED_CONFIG

Default balanced configuration:

```python
from fraiseql.analysis.complexity_config import BALANCED_CONFIG

# Moderate penalties and thresholds
# depth_multiplier=1.5
# array_field_multiplier=10
# complex_query_threshold=200
```

### RELAXED_CONFIG

For powerful servers with ample resources:

```python
from fraiseql.analysis.complexity_config import RELAXED_CONFIG

# Lower penalties, higher thresholds
# depth_multiplier=1.2
# array_field_multiplier=5
# complex_query_threshold=500
```

## Integration with TurboRouter

The enhanced TurboRouter uses complexity analysis for intelligent cache management:

```python
from fraiseql.fastapi.turbo_enhanced import (
    EnhancedTurboRegistry,
    EnhancedTurboRouter,
)
from fraiseql.analysis.complexity_config import STRICT_CONFIG

# Create registry with custom config
registry = EnhancedTurboRegistry(
    max_size=1000,
    max_complexity=150,  # Override threshold
    max_total_weight=2000.0,
    schema=schema,
    config=STRICT_CONFIG,  # Use strict configuration
)

# Create enhanced router
router = EnhancedTurboRouter(registry)

# Check if query should be cached
if router.should_register(query):
    # Query will be cached
    pass
```

## Cache Weight Calculation

The cache weight determines how much "space" a query occupies in the cache:

| Complexity Score | Cache Weight | Description |
|-----------------|--------------|-------------|
| < 10 | 0.1 | Simple queries |
| 10-49 | 0.5 | Moderate queries |
| 50-199 | 2.0 | Complex queries |
| ≥ 200 | Dynamic | Very complex (exponential growth) |

## Array Field Detection

The system automatically detects array fields using:

1. **Plural detection** - Fields ending with 's'
2. **Pattern matching** - Common patterns like "items", "list", "all"
3. **Custom patterns** - Configure your own patterns

```python
config = ComplexityConfig(
    array_field_patterns=[
        "customList",
        "dataSet",
        "collection",
    ]
)
```

## Performance Monitoring

Track cache performance with built-in metrics:

```python
# Get metrics from enhanced registry
metrics = registry.get_metrics()

print(f"Cache hit rate: {metrics['hit_rate']:.2%}")
print(f"Total queries analyzed: {metrics['total_queries_analyzed']}")
print(f"Queries rejected: {metrics['queries_rejected_complexity']}")
print(f"Cache size: {metrics['cache_size']}")
print(f"Weight utilization: {metrics['weight_utilization']:.2%}")
```

## Best Practices

1. **Start with BALANCED_CONFIG** and adjust based on monitoring
2. **Monitor cache hit rates** to tune thresholds
3. **Set appropriate max_complexity** based on your server capacity
4. **Use array_field_patterns** for domain-specific fields
5. **Implement rate limiting** based on total_score

## Example: Rate Limiting by Complexity

```python
from fraiseql.analysis import analyze_query_complexity

class ComplexityRateLimiter:
    def __init__(self, max_cost_per_minute: int = 1000):
        self.max_cost = max_cost_per_minute
        self.user_costs = {}  # Track per user

    def check_query(self, user_id: str, query: str) -> bool:
        score = analyze_query_complexity(query)
        cost = score.total_score

        current_cost = self.user_costs.get(user_id, 0)
        if current_cost + cost > self.max_cost:
            return False

        self.user_costs[user_id] = current_cost + cost
        return True
```

## Advanced Usage

### Custom Scoring Logic

```python
class CustomComplexityConfig(ComplexityConfig):
    def calculate_depth_penalty(self, depth: int) -> int:
        # Custom exponential growth
        if depth <= 2:
            return depth
        return int(2 ** (depth - 1))

    def is_array_field(self, field_name: str) -> bool:
        # Add domain-specific logic
        if field_name.startswith("get_all_"):
            return True
        return super().is_array_field(field_name)
```

### Schema-Aware Analysis

When a GraphQL schema is provided, the analyzer can make more accurate assessments:

```python
from fraiseql.gql import build_fraiseql_schema

# Build schema
schema = build_fraiseql_schema(types=[...])

# Analyze with schema context
score = analyze_query_complexity(query, schema=schema)
```

## API Reference

### analyze_query_complexity

```python
def analyze_query_complexity(
    query: str,
    schema: GraphQLSchema | None = None,
    config: ComplexityConfig | None = None,
) -> ComplexityScore
```

Analyzes a GraphQL query and returns its complexity score.

**Parameters:**
- `query` - GraphQL query string
- `schema` - Optional GraphQL schema for enhanced analysis
- `config` - Complexity configuration (uses default if None)

**Returns:**
- `ComplexityScore` object with analysis results

### ComplexityScore

```python
@dataclass
class ComplexityScore:
    field_count: int = 0
    max_depth: int = 0
    array_field_count: int = 0
    type_diversity: int = 0
    fragment_count: int = 0
    depth_score: int = 0
    array_score: int = 0

    @property
    def total_score(self) -> int: ...

    @property
    def cache_weight(self) -> float: ...

    def should_cache(self, threshold: int = 200) -> bool: ...
```

### ComplexityConfig

See the [source code](https://github.com/lilo-ai/fraiseql/blob/main/src/fraiseql/analysis/complexity_config.py) for the full API.
