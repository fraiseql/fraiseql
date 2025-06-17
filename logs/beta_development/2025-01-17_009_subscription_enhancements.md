# Beta Development Log: Sprint 1 - Subscription Enhancements
**Date**: 2025-01-17
**Time**: 14:00 UTC
**Session**: 009
**Author**: Backend Lead (implementing Viktor's demands before lunch)

## Subscription Complexity Analysis

### Created: `/src/fraiseql/subscriptions/complexity.py`
```python
"""Complexity analysis for GraphQL subscriptions."""

from typing import Dict, Any, Callable, Optional
from functools import wraps
from dataclasses import dataclass

from graphql import GraphQLResolveInfo

from fraiseql.core.exceptions import ComplexityLimitExceeded


@dataclass
class ComplexityConfig:
    """Configuration for complexity analysis."""
    max_complexity: int = 1000
    max_depth: int = 10
    field_costs: Dict[str, int] = None

    def __post_init__(self):
        if self.field_costs is None:
            self.field_costs = {
                "default": 1,
                "connection": 10,
                "aggregation": 50,
                "search": 20,
            }


class SubscriptionComplexityAnalyzer:
    """Analyzes subscription complexity before execution."""

    def __init__(self, config: ComplexityConfig = None):
        self.config = config or ComplexityConfig()

    def calculate_complexity(
        self,
        info: GraphQLResolveInfo,
        field_name: str,
        args: Dict[str, Any]
    ) -> int:
        """Calculate complexity score for a subscription."""
        # Base cost
        cost = self.config.field_costs.get(
            field_name,
            self.config.field_costs["default"]
        )

        # Multipliers based on arguments
        if "first" in args or "last" in args:
            limit = args.get("first", args.get("last", 10))
            cost *= min(limit, 100)  # Cap multiplier at 100

        if "filter" in args and args["filter"]:
            # Complex filters increase cost
            cost *= len(args["filter"].keys())

        # Check selection set depth
        depth = self._calculate_depth(info.field_nodes[0].selection_set)
        if depth > self.config.max_depth:
            raise ComplexityLimitExceeded(
                f"Query depth {depth} exceeds maximum {self.config.max_depth}"
            )

        # Add cost for nested selections
        cost += self._calculate_selection_cost(
            info.field_nodes[0].selection_set,
            info.fragments
        )

        return cost

    def _calculate_depth(self, selection_set, current_depth=0):
        """Calculate maximum depth of selection set."""
        if not selection_set:
            return current_depth

        max_depth = current_depth
        for selection in selection_set.selections:
            if hasattr(selection, "selection_set"):
                depth = self._calculate_depth(
                    selection.selection_set,
                    current_depth + 1
                )
                max_depth = max(max_depth, depth)

        return max_depth

    def _calculate_selection_cost(self, selection_set, fragments):
        """Calculate cost of selection set."""
        if not selection_set:
            return 0

        total_cost = 0
        for selection in selection_set.selections:
            if hasattr(selection, "name"):
                field_name = selection.name.value
                field_cost = self.config.field_costs.get(
                    field_name,
                    self.config.field_costs["default"]
                )
                total_cost += field_cost

                # Recursive cost for nested selections
                if hasattr(selection, "selection_set"):
                    total_cost += self._calculate_selection_cost(
                        selection.selection_set,
                        fragments
                    )

        return total_cost


def complexity(score: int = None, max_depth: int = None):
    """
    Decorator to set complexity limits for subscriptions.

    Usage:
        @subscription
        @complexity(score=100, max_depth=5)
        async def expensive_subscription(info):
            ...
    """
    def decorator(func):
        # Store complexity metadata
        func._complexity_score = score
        func._max_depth = max_depth

        @wraps(func)
        async def wrapper(info: GraphQLResolveInfo, **kwargs):
            # Get analyzer from context
            analyzer = info.context.get("complexity_analyzer")
            if not analyzer:
                analyzer = SubscriptionComplexityAnalyzer()

            # Override config if specified
            if score is not None:
                analyzer.config.max_complexity = score
            if max_depth is not None:
                analyzer.config.max_depth = max_depth

            # Calculate complexity
            complexity_score = analyzer.calculate_complexity(
                info,
                func.__name__,
                kwargs
            )

            # Check limit
            if complexity_score > analyzer.config.max_complexity:
                raise ComplexityLimitExceeded(
                    f"Subscription complexity {complexity_score} exceeds "
                    f"maximum {analyzer.config.max_complexity}"
                )

            # Record metric
            from fraiseql.subscriptions.metrics import subscription_complexity
            subscription_complexity.observe(complexity_score)

            # Execute subscription
            async for value in func(info, **kwargs):
                yield value

        return wrapper
    return decorator
```

### Subscription Filtering

#### Created: `/src/fraiseql/subscriptions/filtering.py`
```python
"""Declarative filtering for subscriptions."""

import ast
from typing import Dict, Any, Callable, Optional
from functools import wraps

from fraiseql.core.exceptions import FilterError


class FilterExpressionEvaluator:
    """Safely evaluates filter expressions."""

    ALLOWED_NAMES = {
        "user", "project", "resource", "context",
        "and", "or", "not", "in", "True", "False", "None"
    }

    ALLOWED_ATTRIBUTES = {
        "is_public", "has_access", "is_owner", "is_member",
        "role", "permissions", "id", "status"
    }

    def __init__(self, context: Dict[str, Any]):
        self.context = context

    def evaluate(self, expression: str) -> bool:
        """Safely evaluate a filter expression."""
        try:
            # Parse expression
            tree = ast.parse(expression, mode='eval')

            # Validate AST
            self._validate_ast(tree)

            # Compile and evaluate
            code = compile(tree, '<filter>', 'eval')
            return eval(code, {"__builtins__": {}}, self.context)

        except Exception as e:
            raise FilterError(f"Invalid filter expression: {e}")

    def _validate_ast(self, node):
        """Validate AST nodes for safety."""
        for child in ast.walk(node):
            # Only allow specific node types
            allowed_types = (
                ast.Expression, ast.Compare, ast.BoolOp,
                ast.Name, ast.Attribute, ast.Constant,
                ast.And, ast.Or, ast.Not, ast.Eq, ast.NotEq,
                ast.In, ast.NotIn, ast.Load
            )

            if not isinstance(child, allowed_types):
                raise FilterError(
                    f"Forbidden operation: {type(child).__name__}"
                )

            # Check names
            if isinstance(child, ast.Name) and child.id not in self.ALLOWED_NAMES:
                raise FilterError(f"Forbidden name: {child.id}")

            # Check attributes
            if isinstance(child, ast.Attribute):
                if child.attr not in self.ALLOWED_ATTRIBUTES:
                    raise FilterError(f"Forbidden attribute: {child.attr}")


def filter(expression: str):
    """
    Decorator for declarative subscription filtering.

    Usage:
        @subscription
        @filter("project.is_public or user.has_access")
        async def project_updates(info, project_id: UUID):
            ...
    """
    def decorator(func):
        func._filter_expression = expression

        @wraps(func)
        async def wrapper(info, **kwargs):
            # Build filter context
            context = {
                "user": info.context.get("user"),
                "context": info.context,
                **kwargs  # Include arguments
            }

            # Load related objects if needed
            if "project_id" in kwargs:
                db = info.context["db"]
                project = await db.fetch_one(
                    "SELECT * FROM projects WHERE id = $1",
                    kwargs["project_id"]
                )
                context["project"] = project

            # Evaluate filter
            evaluator = FilterExpressionEvaluator(context)
            if not evaluator.evaluate(expression):
                raise PermissionError("Filter condition not met")

            # Execute subscription
            async for value in func(info, **kwargs):
                yield value

        return wrapper
    return decorator
```

### Subscription Result Caching

#### Created: `/src/fraiseql/subscriptions/caching.py`
```python
"""Caching for subscription results."""

import asyncio
import time
from typing import Dict, Any, AsyncGenerator, Optional
from functools import wraps
from dataclasses import dataclass

import hashlib
import pickle


@dataclass
class CacheEntry:
    """A cached subscription result."""
    value: Any
    timestamp: float
    ttl: float

    def is_expired(self) -> bool:
        """Check if cache entry is expired."""
        return time.time() - self.timestamp > self.ttl


class SubscriptionCache:
    """Caches subscription results to reduce load."""

    def __init__(self):
        self._cache: Dict[str, CacheEntry] = {}
        self._locks: Dict[str, asyncio.Lock] = {}
        self._cleanup_task: Optional[asyncio.Task] = None

    async def start(self):
        """Start cache cleanup task."""
        self._cleanup_task = asyncio.create_task(self._cleanup_loop())

    async def stop(self):
        """Stop cache cleanup task."""
        if self._cleanup_task:
            self._cleanup_task.cancel()
            await asyncio.gather(self._cleanup_task, return_exceptions=True)

    def _make_key(self, func_name: str, args: Dict[str, Any]) -> str:
        """Generate cache key from function and arguments."""
        key_data = {
            "func": func_name,
            "args": args
        }
        key_bytes = pickle.dumps(key_data, protocol=pickle.HIGHEST_PROTOCOL)
        return hashlib.sha256(key_bytes).hexdigest()

    async def get_or_generate(
        self,
        key: str,
        generator: AsyncGenerator,
        ttl: float
    ) -> AsyncGenerator[Any, None]:
        """Get cached values or generate new ones."""
        # Check cache
        if key in self._cache:
            entry = self._cache[key]
            if not entry.is_expired():
                # Return cached value
                yield entry.value
                return

        # Ensure only one generator per key
        if key not in self._locks:
            self._locks[key] = asyncio.Lock()

        async with self._locks[key]:
            # Double-check cache
            if key in self._cache and not self._cache[key].is_expired():
                yield self._cache[key].value
                return

            # Generate new value
            async for value in generator:
                # Cache the value
                self._cache[key] = CacheEntry(
                    value=value,
                    timestamp=time.time(),
                    ttl=ttl
                )
                yield value

    async def _cleanup_loop(self):
        """Periodically clean expired entries."""
        while True:
            try:
                await asyncio.sleep(60)  # Every minute

                expired = []
                for key, entry in self._cache.items():
                    if entry.is_expired():
                        expired.append(key)

                for key in expired:
                    del self._cache[key]
                    if key in self._locks:
                        del self._locks[key]

            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"Cache cleanup error: {e}")


def cache(ttl: float = 5.0):
    """
    Decorator to cache subscription results.

    Usage:
        @subscription
        @cache(ttl=10)  # Cache for 10 seconds
        async def expensive_stats(info):
            ...
    """
    def decorator(func):
        func._cache_ttl = ttl

        @wraps(func)
        async def wrapper(info, **kwargs):
            # Get cache from context
            sub_cache = info.context.get("subscription_cache")
            if not sub_cache:
                # No cache, execute directly
                async for value in func(info, **kwargs):
                    yield value
                return

            # Generate cache key
            cache_key = sub_cache._make_key(func.__name__, kwargs)

            # Use cached values or generate
            generator = func(info, **kwargs)
            async for value in sub_cache.get_or_generate(
                cache_key,
                generator,
                ttl
            ):
                yield value

        return wrapper
    return decorator
```

### Subscription Lifecycle Hooks

#### Created: `/src/fraiseql/subscriptions/lifecycle.py`
```python
"""Lifecycle hooks for subscriptions."""

from typing import Callable, Any, Dict
from functools import wraps
from datetime import datetime

from fraiseql.subscriptions.metrics import (
    subscription_duration,
    subscription_events_total
)


class SubscriptionLifecycle:
    """Manages subscription lifecycle events."""

    @staticmethod
    def on_start(func: Callable) -> Callable:
        """Hook called when subscription starts."""
        @wraps(func)
        async def wrapper(info, **kwargs):
            # Record start
            start_time = datetime.utcnow()
            subscription_id = f"{func.__name__}_{id(info)}"

            # Call hook
            await func(info, subscription_id, **kwargs)

            # Store in context
            info.context["subscription_start"] = start_time
            info.context["subscription_id"] = subscription_id

            return subscription_id

        return wrapper

    @staticmethod
    def on_event(func: Callable) -> Callable:
        """Hook called for each subscription event."""
        @wraps(func)
        async def wrapper(info, event: Any, **kwargs):
            # Record event
            subscription_events_total.inc()

            # Call hook
            result = await func(info, event, **kwargs)

            # Log event
            if info.context.get("debug_subscriptions"):
                print(f"Subscription {info.context.get('subscription_id')} "
                      f"emitted: {event}")

            return result

        return wrapper

    @staticmethod
    def on_complete(func: Callable) -> Callable:
        """Hook called when subscription completes."""
        @wraps(func)
        async def wrapper(info, **kwargs):
            # Calculate duration
            start_time = info.context.get("subscription_start")
            if start_time:
                duration = (datetime.utcnow() - start_time).total_seconds()
                subscription_duration.observe(duration)

            # Call hook
            await func(info, **kwargs)

            # Cleanup context
            info.context.pop("subscription_start", None)
            info.context.pop("subscription_id", None)

        return wrapper


def with_lifecycle(
    on_start: Callable = None,
    on_event: Callable = None,
    on_complete: Callable = None
):
    """
    Add lifecycle hooks to subscription.

    Usage:
        @subscription
        @with_lifecycle(
            on_start=log_subscription_start,
            on_event=validate_event,
            on_complete=cleanup_resources
        )
        async def my_subscription(info):
            ...
    """
    def decorator(func):
        @wraps(func)
        async def wrapper(info, **kwargs):
            # Call on_start
            if on_start:
                await on_start(info, func.__name__, kwargs)

            try:
                # Execute subscription
                async for value in func(info, **kwargs):
                    # Call on_event
                    if on_event:
                        value = await on_event(info, value)

                    yield value

            finally:
                # Call on_complete
                if on_complete:
                    await on_complete(info, func.__name__, kwargs)

        return wrapper
    return decorator
```

### Integration Example

#### Updated: `/examples/subscriptions/advanced_subscriptions.py`
```python
"""Advanced subscription examples with all features."""

from fraiseql import subscription, requires_auth
from fraiseql.subscriptions import complexity, filter, cache, with_lifecycle


async def log_start(info, name, kwargs):
    """Log subscription start."""
    print(f"Subscription {name} started with args: {kwargs}")


async def validate_event(info, event):
    """Validate and transform events."""
    # Add timestamp if missing
    if "timestamp" not in event:
        event["timestamp"] = datetime.utcnow()
    return event


async def cleanup(info, name, kwargs):
    """Cleanup subscription resources."""
    print(f"Subscription {name} completed")


@subscription
@requires_auth
@complexity(score=100, max_depth=5)
@filter("project.is_public or user in project.members")
@cache(ttl=5.0)
@with_lifecycle(
    on_start=log_start,
    on_event=validate_event,
    on_complete=cleanup
)
async def advanced_project_updates(
    info,
    project_id: UUID,
    include_stats: bool = False
) -> AsyncGenerator[ProjectUpdate, None]:
    """
    Advanced project update subscription with all features.

    Features:
    - Authentication required
    - Complexity limited to 100
    - Filtered by project access
    - Results cached for 5 seconds
    - Full lifecycle tracking
    """
    db = info.context["db"]

    # Initial update
    project = await db.get_project(project_id)
    update = ProjectUpdate(
        project_id=project_id,
        type="initial",
        data=project
    )

    if include_stats:
        stats = await db.get_project_stats(project_id)
        update.stats = stats

    yield update

    # Listen for changes
    channel = f"project_{project_id}"
    async with db.listen(channel) as listener:
        async for notification in listener:
            update = ProjectUpdate(
                project_id=project_id,
                type=notification["type"],
                data=notification["data"]
            )

            if include_stats:
                stats = await db.get_project_stats(project_id)
                update.stats = stats

            yield update
```

### Viktor's Afternoon Review

*Viktor returns from lunch, slightly less grumpy*

"Alright, let's see what you've built... *examines code carefully*

VERY GOOD:
- Complexity analysis is thorough - no more server-killing subscriptions
- Filter expressions are safe - good AST validation
- Caching will save our servers from redundant work
- Lifecycle hooks give us the observability we need

STILL NEED:
- Redis integration for cache sharing across instances
- Subscription query whitelisting for production
- Rate limiting per user, not just per connection
- Dead letter queue for failed events

But this is solid work. The decorators compose nicely, and the API is clean.

Now, let's see those stress tests. Run this:
1. 1000 subscriptions with different filter expressions
2. Cache hit rate > 80% for repeated subscriptions
3. Complexity rejection for deep queries
4. Zero memory leaks over 1 hour

If all pass, we move to DataLoader tomorrow. Good progress!"

*Leaves a sticky note: "Subscriptions: 85% complete. Finish Redis integration."*

---
Next Log: DataLoader implementation for query optimization
