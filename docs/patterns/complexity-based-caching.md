# Complexity-Based Caching Pattern

This pattern demonstrates how to use FraiseQL's query complexity analysis to implement intelligent caching strategies.

## Overview

Instead of caching all queries equally, this pattern:
- Analyzes query complexity before caching
- Allocates cache space based on query cost
- Evicts queries intelligently based on complexity and usage
- Prevents cache pollution from expensive queries

## Implementation

### 1. Configure Complexity Analysis

Choose a preset configuration based on your environment:

```python
from fraiseql.analysis.complexity_config import (
    STRICT_CONFIG,    # Limited resources
    BALANCED_CONFIG,  # Default
    RELAXED_CONFIG,   # Powerful servers
)

# For a resource-constrained environment
config = STRICT_CONFIG

# Or create custom configuration
from fraiseql.analysis.complexity_config import ComplexityConfig

config = ComplexityConfig(
    # Adjust thresholds
    simple_query_threshold=20,      # Higher threshold for simple
    moderate_query_threshold=100,   # Adjust based on monitoring
    complex_query_threshold=300,    # Maximum cacheable complexity
    
    # Tune scoring
    depth_multiplier=1.8,           # How much depth impacts score
    array_field_multiplier=12,      # Cost of array fields
    
    # Cache weights
    simple_query_weight=0.2,        # Simple queries use less space
    complex_query_weight=3.0,       # Complex queries use more space
)
```

### 2. Create Enhanced TurboRouter

```python
from fraiseql.fastapi.turbo_enhanced import (
    EnhancedTurboRegistry,
    EnhancedTurboRouter,
)

# Create registry with complexity limits
registry = EnhancedTurboRegistry(
    max_size=1000,              # Maximum number of queries
    max_complexity=300,         # Reject queries above this
    max_total_weight=5000.0,    # Total cache weight limit
    schema=schema,
    config=config,
)

# Create router
turbo_router = EnhancedTurboRouter(registry)
```

### 3. Integrate with FastAPI

```python
from fraiseql.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    types=[User, Post, Comment],
    mutations=[create_post, update_post],
    production=True,  # Enable turbo router
)

# Add monitoring endpoint
@app.get("/api/cache/metrics")
async def cache_metrics():
    """Get cache performance metrics."""
    return registry.get_metrics()

# Add query analysis endpoint (development only)
if not production:
    @app.post("/api/analyze-query")
    async def analyze_query(query: str):
        """Analyze query complexity."""
        score, weight = registry.analyze_query(query)
        return {
            "complexity_score": score.total_score,
            "cache_weight": weight,
            "should_cache": registry.should_cache(score),
            "details": {
                "field_count": score.field_count,
                "max_depth": score.max_depth,
                "array_fields": score.array_field_count,
            }
        }
```

### 4. Monitor and Tune

Create a monitoring dashboard:

```python
from datetime import datetime, timedelta
import asyncio

class CacheMonitor:
    def __init__(self, registry: EnhancedTurboRegistry):
        self.registry = registry
        self.history = []
    
    async def collect_metrics(self):
        """Collect metrics every minute."""
        while True:
            metrics = self.registry.get_metrics()
            self.history.append({
                "timestamp": datetime.now(),
                "metrics": metrics
            })
            
            # Keep last 24 hours
            cutoff = datetime.now() - timedelta(hours=24)
            self.history = [h for h in self.history if h["timestamp"] > cutoff]
            
            await asyncio.sleep(60)
    
    def get_report(self):
        """Generate performance report."""
        if not self.history:
            return {"error": "No data collected yet"}
        
        latest = self.history[-1]["metrics"]
        
        # Calculate trends
        hour_ago = datetime.now() - timedelta(hours=1)
        hour_data = [h for h in self.history if h["timestamp"] > hour_ago]
        
        if len(hour_data) > 1:
            start_hit_rate = hour_data[0]["metrics"]["hit_rate"]
            end_hit_rate = hour_data[-1]["metrics"]["hit_rate"]
            hit_rate_trend = end_hit_rate - start_hit_rate
        else:
            hit_rate_trend = 0
        
        return {
            "current": {
                "hit_rate": f"{latest['hit_rate']:.2%}",
                "cache_size": latest["cache_size"],
                "weight_utilization": f"{latest['weight_utilization']:.2%}",
                "queries_rejected": latest["queries_rejected_complexity"],
            },
            "trends": {
                "hit_rate_change": f"{hit_rate_trend:+.2%}",
            },
            "recommendations": self._get_recommendations(latest)
        }
    
    def _get_recommendations(self, metrics):
        """Generate tuning recommendations."""
        recommendations = []
        
        if metrics["hit_rate"] < 0.7:
            recommendations.append("Low hit rate - consider increasing cache size")
        
        if metrics["weight_utilization"] > 0.9:
            recommendations.append("High weight utilization - increase max_total_weight")
        
        if metrics["queries_rejected_complexity"] > metrics["cache_size"] * 0.1:
            recommendations.append("Many queries rejected - consider raising max_complexity")
        
        return recommendations

# Start monitoring
monitor = CacheMonitor(registry)
asyncio.create_task(monitor.collect_metrics())

# Add monitoring endpoint
@app.get("/api/cache/report")
async def cache_report():
    return monitor.get_report()
```

### 5. Implement Cache Warming

Pre-populate cache with common queries:

```python
async def warm_cache(registry: EnhancedTurboRegistry):
    """Warm cache with common queries."""
    
    common_queries = [
        # Simple queries - will have low weight
        {
            "query": """
                query GetUser($id: ID!) {
                    user(id: $id) {
                        id
                        name
                        avatar
                    }
                }
            """,
            "sql_template": "SELECT data FROM v_users WHERE data->>'id' = $1",
            "param_mapping": {"id": 0},
        },
        # Moderate complexity - dashboard query
        {
            "query": """
                query GetDashboard($userId: ID!) {
                    user(id: $userId) {
                        name
                        stats {
                            postCount
                            followerCount
                        }
                        recentPosts(limit: 5) {
                            id
                            title
                            createdAt
                        }
                    }
                }
            """,
            "sql_template": "SELECT * FROM get_user_dashboard($1)",
            "param_mapping": {"userId": 0},
        },
    ]
    
    for query_def in common_queries:
        # Check if we should cache it
        score, _ = registry.analyze_query(query_def["query"])
        
        if registry.should_cache(score):
            turbo_query = TurboQuery(
                graphql_query=query_def["query"],
                sql_template=query_def["sql_template"],
                param_mapping=query_def["param_mapping"],
            )
            registry.register(turbo_query)
    
    print(f"Cache warmed with {len(registry)} queries")

# Run on startup
@app.on_event("startup")
async def startup():
    await warm_cache(registry)
```

### 6. Implement Query Complexity Limits

Prevent abuse through complexity-based rate limiting:

```python
from collections import defaultdict
from datetime import datetime, timedelta

class ComplexityRateLimiter:
    def __init__(
        self,
        max_complexity_per_minute: int = 5000,
        max_complexity_per_hour: int = 50000,
    ):
        self.minute_limit = max_complexity_per_minute
        self.hour_limit = max_complexity_per_hour
        self.user_usage = defaultdict(list)
    
    def check_request(self, user_id: str, query: str) -> tuple[bool, str]:
        """Check if request is allowed."""
        # Analyze query
        score = analyze_query_complexity(query)
        complexity = score.total_score
        
        now = datetime.now()
        user_history = self.user_usage[user_id]
        
        # Clean old entries
        hour_ago = now - timedelta(hours=1)
        user_history[:] = [
            (timestamp, cost) 
            for timestamp, cost in user_history 
            if timestamp > hour_ago
        ]
        
        # Calculate usage
        minute_ago = now - timedelta(minutes=1)
        minute_usage = sum(
            cost for timestamp, cost in user_history 
            if timestamp > minute_ago
        )
        hour_usage = sum(cost for _, cost in user_history)
        
        # Check limits
        if minute_usage + complexity > self.minute_limit:
            return False, f"Rate limit exceeded: {minute_usage + complexity}/{self.minute_limit} complexity/minute"
        
        if hour_usage + complexity > self.hour_limit:
            return False, f"Rate limit exceeded: {hour_usage + complexity}/{self.hour_limit} complexity/hour"
        
        # Record usage
        user_history.append((now, complexity))
        return True, "OK"

# Create limiter
limiter = ComplexityRateLimiter()

# Add middleware
@app.middleware("http")
async def complexity_limit_middleware(request, call_next):
    if request.url.path == "/graphql":
        # Get user ID from auth
        user_id = request.headers.get("x-user-id", "anonymous")
        
        # Get query from request
        body = await request.body()
        import json
        try:
            data = json.loads(body)
            query = data.get("query", "")
        except:
            query = ""
        
        if query:
            allowed, message = limiter.check_request(user_id, query)
            if not allowed:
                return JSONResponse(
                    status_code=429,
                    content={"error": message}
                )
    
    return await call_next(request)
```

## Best Practices

1. **Start with BALANCED_CONFIG** and adjust based on metrics
2. **Monitor cache hit rate** - Target 70%+ for good performance
3. **Track weight utilization** - Keep below 80% for headroom
4. **Analyze rejected queries** - Adjust thresholds if too many rejections
5. **Use cache warming** for predictable query patterns
6. **Implement gradual rollout** - Test with subset of traffic first

## Example Metrics Analysis

```python
# Good metrics
{
    "hit_rate": 0.82,              # 82% - Good
    "cache_size": 743,             # Using 743/1000 slots
    "weight_utilization": 0.65,    # 65% - Healthy headroom
    "queries_rejected": 12,        # Low rejection rate
}

# Needs tuning
{
    "hit_rate": 0.45,              # 45% - Too low
    "cache_size": 1000,            # Cache full
    "weight_utilization": 0.95,    # 95% - Near limit
    "queries_rejected": 234,       # High rejection rate
}
```

## Troubleshooting

### Low Hit Rate

1. Increase `max_size` to cache more queries
2. Analyze rejected queries and raise `max_complexity`
3. Check if queries have high variability (different variables)

### High Memory Usage

1. Reduce `max_total_weight`
2. Use STRICT_CONFIG to be more selective
3. Lower `complex_query_threshold`

### Many Rejections

1. Analyze rejected queries for patterns
2. Adjust `array_field_multiplier` if array queries rejected
3. Consider separate cache pools for different query types

## Advanced: Multi-Tier Caching

For very large applications, implement multiple cache tiers:

```python
# Tier 1: Hot cache for simple queries
hot_cache = EnhancedTurboRegistry(
    max_size=5000,
    max_complexity=50,  # Only simple queries
    config=ComplexityConfig(
        complex_query_threshold=50,
        simple_query_weight=0.1,
    )
)

# Tier 2: Warm cache for moderate queries  
warm_cache = EnhancedTurboRegistry(
    max_size=1000,
    max_complexity=200,
    config=BALANCED_CONFIG,
)

# Tier 3: Cold cache for complex queries
cold_cache = EnhancedTurboRegistry(
    max_size=200,
    max_complexity=500,
    config=RELAXED_CONFIG,
)

# Router that checks all tiers
class MultiTierRouter:
    def __init__(self, tiers: list[EnhancedTurboRegistry]):
        self.tiers = tiers
    
    async def execute(self, query: str, variables: dict, context: dict):
        # Try each tier
        for tier in self.tiers:
            result = await tier.get(query)
            if result:
                return await result.execute(variables, context)
        
        return None  # Not cached
```

This pattern ensures optimal cache utilization by segregating queries based on their complexity characteristics.