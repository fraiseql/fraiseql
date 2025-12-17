# Implementation Plan: View/Metadata Directives (Gap #2)

**Feature:** Support schema metadata directives for views and dependencies
**Effort:** 2-4 hours
**Complexity:** Low-Moderate
**Risk:** Low
**Status:** Ready for implementation

---

## Executive Summary

FraiseQL uses denormalized materialized views (`tv_*`) that require careful maintenance. This plan adds **metadata directives** to document and enforce schema requirements:

- `@view_cached(ttl: Int!)` - Control view refresh/cache TTL
- `@depends_on(views: [String!]!)` - Document upstream view dependencies
- `@requires_function(name: String!)` - Require SQL function existence
- `@cost_units(estimate: Float!)` - Query complexity estimates

These directives are **purely semantic**—they document intentions and enable tooling, but don't execute business logic.

**Example usage:**
```graphql
type UserWithProfile {
  id: ID!
  name: String!

  profile: ProfileData
    @view_cached(ttl: 3600)
    @depends_on(views: ["tb_user", "tb_profile"])
    @requires_function(name: "fn_validate_profile")
    @cost_units(estimate: 2.5)
}
```

---

## Part 1: Current State Analysis

### Where Metadata Goes Today

Currently, metadata is scattered:
- **View refresh**: PostgreSQL IVM (Incremental View Maintenance) configuration
- **Dependencies**: Implicit in view SQL, not documented in schema
- **Functions**: No tracking of required functions
- **Costs**: Not represented in schema at all

### The Problem

Without schema metadata:
1. ❌ Dependencies are **implicit** in SQL (hard to discover)
2. ❌ View refresh strategy is **opaque** (not visible to API layer)
3. ❌ Required functions are **undocumented** (easy to break)
4. ❌ Query costs are **unmeasured** (can't do cost-based optimization)

### Why Directives?

✅ **Directives are the GraphQL way to attach metadata**
✅ **Introspection reveals directives** (tooling can use them)
✅ **Spec-standard** (follows GraphQL best practices)
✅ **Purely additive** (no breaking changes)

---

## Part 2: Implementation Strategy

### Architecture

```
Schema Definition
    ↓
Field with directives
    ├── @view_cached(ttl)
    ├── @depends_on(views)
    ├── @requires_function(name)
    └── @cost_units(estimate)
    ↓
Directives stored in GraphQL schema
    ↓
Introspection query can retrieve
    ↓
Tooling uses metadata for:
    ├── View dependency graphs
    ├── Query cost planning
    ├── Validation
    └── Documentation

Execution: Directives are ignored (metadata only)
```

### Key Design Decisions

**Decision 1: Where should directives apply?**
- ✅ **FIELD_DEFINITION** - On individual fields
- Gives granular control
- Makes sense with denormalized view design
- Could extend to TYPE_DEFINITION later

**Decision 2: Should directives execute or just store metadata?**
- ✅ **Store metadata only** (no execution)
- Directives are validation, not transformation
- Real caching/refresh happens at PostgreSQL level (IVM)
- FraiseQL can't control database refresh anyway

**Decision 3: Should we validate directives at schema build time?**
- ✅ **Yes, but optional/warnings only**
- Check that required functions exist (might warn if not found)
- Don't fail schema build (might be in-progress setup)
- Validate dependencies exist (but warn, don't error)

**Decision 4: How to define directives?**
- ✅ **Standard GraphQL directive definitions**
- Use `GraphQLDirective` from graphql-core
- Add to schema during schema building
- Discoverable via introspection

---

## Part 3: Detailed Implementation Steps

### Step 1: Define Directive Objects (30 minutes)

**File:** `src/fraiseql/gql/schema_directives.py` (NEW)

```python
"""GraphQL directives for schema metadata.

These directives provide semantic information about views, dependencies,
functions, and query costs. They are purely metadata and do not affect
query execution—caching and refresh are handled at the PostgreSQL level.
"""

from dataclasses import dataclass
from typing import Optional


@dataclass
class ViewCacheDirective:
    """@view_cached(ttl: Int!) - Materialized view cache/refresh TTL.

    Indicates that a field's data comes from a materialized view that
    should be refreshed at approximately this interval (in seconds).

    This is semantic metadata for tooling and documentation. Actual
    view refresh is managed by PostgreSQL IVM or explicit refresh jobs.

    Example:
        profile: dict
            @view_cached(ttl: 3600)  # Refresh hourly

    Args:
        ttl: Time-to-live in seconds (must be positive)
    """

    ttl: int

    def validate(self) -> list[str]:
        """Validate directive arguments.

        Returns:
            List of validation error messages (empty if valid)
        """
        errors = []
        if self.ttl <= 0:
            errors.append(f"ttl must be positive, got {self.ttl}")
        if self.ttl > 86400 * 365:  # Warn if > 1 year
            errors.append(f"ttl is very large: {self.ttl}s (> 1 year)")
        return errors


@dataclass
class DependsOnDirective:
    """@depends_on(views: [String!]!) - Upstream view/table dependencies.

    Documents which views and tables a field depends on. Enables:
    - Automatic view dependency graph generation
    - Validation that dependencies exist
    - Impact analysis for schema changes
    - Documentation of implicit relationships

    Example:
        profile: dict
            @depends_on(views: ["tb_user", "tb_profile"])

    Args:
        views: List of view/table names this field depends on
    """

    views: list[str]

    def validate(self) -> list[str]:
        """Validate directive arguments.

        Returns:
            List of validation error messages (empty if valid)
        """
        errors = []
        if not self.views:
            errors.append("views list cannot be empty")
        for view in self.views:
            if not view or not isinstance(view, str):
                errors.append(f"Invalid view name: {view}")
        return errors


@dataclass
class RequiresFunctionDirective:
    """@requires_function(name: String!) - Required SQL function.

    Documents that this field requires a specific SQL function to exist
    in the database. Used for:
    - Schema validation
    - Function existence checks
    - Documentation of SQL dependencies
    - Error detection during deployment

    Example:
        profile: dict
            @requires_function(name: "fn_validate_profile")

    Args:
        name: Name of the SQL function (schema.function or just function)
    """

    name: str

    def validate(self) -> list[str]:
        """Validate directive arguments.

        Returns:
            List of validation error messages (empty if valid)
        """
        errors = []
        if not self.name or not isinstance(self.name, str):
            errors.append(f"function name must be non-empty string, got {self.name}")
        return errors


@dataclass
class CostUnitsDirective:
    """@cost_units(estimate: Float!) - Query complexity/cost estimate.

    Provides a relative cost estimate for this field's resolution.
    Used for:
    - Query cost analysis and limiting
    - Complex query detection
    - Performance budgeting
    - Rate limiting based on cost

    Rough scale:
    - 0.1-1.0: Simple scalar field, indexed lookup
    - 1.0-5.0: Aggregation, simple join
    - 5.0-20.0: Complex join, multi-step computation
    - 20.0+: Very expensive, should warn

    Example:
        posts: [Post!]!
            @cost_units(estimate: 5.0)

    Args:
        estimate: Relative cost units (non-negative)
    """

    estimate: float

    def validate(self) -> list[str]:
        """Validate directive arguments.

        Returns:
            List of validation error messages (empty if valid)
        """
        errors = []
        if self.estimate < 0:
            errors.append(f"estimate must be non-negative, got {self.estimate}")
        if self.estimate > 1000:
            errors.append(f"estimate is very high: {self.estimate} (> 1000)")
        return errors
```

**Acceptance:** Directive classes defined and validated locally

---

### Step 2: Create GraphQL Directive Definitions (45 minutes)

**File:** `src/fraiseql/gql/schema_builder.py`

Find where directives are defined and add new ones:

```python
"""Build GraphQL schema directives"""

from graphql import (
    GraphQLDirective,
    GraphQLArgument,
    GraphQLInt,
    GraphQLFloat,
    GraphQLString,
    GraphQLNonNull,
    GraphQLList,
    DirectiveLocation,
)


def create_view_metadata_directives() -> list[GraphQLDirective]:
    """Create metadata directives for view/schema documentation.

    Returns:
        List of GraphQL directive definitions
    """
    return [
        GraphQLDirective(
            name="view_cached",
            locations=[DirectiveLocation.FIELD_DEFINITION],
            args={
                "ttl": GraphQLArgument(
                    GraphQLNonNull(GraphQLInt),
                    description="Cache/refresh TTL in seconds",
                ),
            },
            description=(
                "Indicates field data comes from a materialized view "
                "with this approximate refresh interval (in seconds). "
                "Actual refresh managed by PostgreSQL IVM."
            ),
            is_repeatable=False,
        ),
        GraphQLDirective(
            name="depends_on",
            locations=[DirectiveLocation.FIELD_DEFINITION],
            args={
                "views": GraphQLArgument(
                    GraphQLNonNull(GraphQLList(GraphQLNonNull(GraphQLString))),
                    description="Names of upstream views/tables",
                ),
            },
            description=(
                "Documents upstream view and table dependencies. "
                "Enables dependency graph generation and impact analysis."
            ),
            is_repeatable=False,
        ),
        GraphQLDirective(
            name="requires_function",
            locations=[DirectiveLocation.FIELD_DEFINITION],
            args={
                "name": GraphQLArgument(
                    GraphQLNonNull(GraphQLString),
                    description="SQL function name (schema.func or func)",
                ),
            },
            description=(
                "Documents a required SQL function. Used for validation "
                "and error detection during deployment."
            ),
            is_repeatable=False,  # Only one function required per field
        ),
        GraphQLDirective(
            name="cost_units",
            locations=[DirectiveLocation.FIELD_DEFINITION],
            args={
                "estimate": GraphQLArgument(
                    GraphQLNonNull(GraphQLFloat),
                    description="Relative cost units for query planning",
                ),
            },
            description=(
                "Provides relative cost estimate for field resolution. "
                "Used for query complexity analysis and cost-based optimization."
            ),
            is_repeatable=False,
        ),
    ]


# In schema_builder.py, in the schema creation function:
def build_fraiseql_schema(types, queries, mutations, subscriptions):
    """Build GraphQL schema with metadata directives"""

    # ... existing schema setup ...

    # ✅ NEW: Add metadata directives
    metadata_directives = create_view_metadata_directives()
    all_directives = [
        # Existing directives (@skip, @include, etc.)
        *schema.directives,
        # New metadata directives
        *metadata_directives,
    ]

    # Create schema with new directives
    schema = GraphQLSchema(
        query=query_type,
        mutation=mutation_type,
        subscription=subscription_type,
        types=all_types,
        directives=all_directives,  # ← Include new directives
    )

    return schema
```

**Acceptance:** Directives defined in schema, visible via introspection

---

### Step 3: Add Directive Validation (30 minutes)

**File:** `src/fraiseql/gql/directive_validator.py` (NEW)

```python
"""Validate metadata directives at schema build time.

This module provides optional validation of metadata directives.
Validation is best-effort (logs warnings) rather than fail-fast,
since schemas might be in-progress setup.
"""

import logging
from typing import Any, Optional

from graphql import GraphQLSchema, GraphQLObjectType, GraphQLField

logger = logging.getLogger(__name__)


class DirectiveValidationResult:
    """Result of directive validation"""

    def __init__(self):
        self.warnings: list[str] = []
        self.errors: list[str] = []

    def add_warning(self, message: str) -> None:
        """Add a validation warning"""
        self.warnings.append(message)
        logger.warning(message)

    def add_error(self, message: str) -> None:
        """Add a validation error"""
        self.errors.append(message)
        logger.error(message)

    @property
    def is_valid(self) -> bool:
        """True if no errors (warnings are okay)"""
        return len(self.errors) == 0


def validate_schema_directives(
    schema: GraphQLSchema,
    db_connection: Optional[Any] = None,
) -> DirectiveValidationResult:
    """Validate metadata directives in schema.

    Args:
        schema: GraphQL schema to validate
        db_connection: Optional database connection for live validation

    Returns:
        DirectiveValidationResult with warnings and errors
    """
    result = DirectiveValidationResult()

    # Iterate over all object types
    for type_name, type_def in schema.type_map.items():
        if isinstance(type_def, GraphQLObjectType):
            for field_name, field in type_def.fields.items():
                _validate_field_directives(
                    schema, type_name, field_name, field, result, db_connection
                )

    return result


def _validate_field_directives(
    schema: GraphQLSchema,
    type_name: str,
    field_name: str,
    field: GraphQLField,
    result: DirectiveValidationResult,
    db_connection: Optional[Any] = None,
) -> None:
    """Validate directives on a single field"""

    # Get directives from the field (if stored)
    # Note: graphql-core stores directives in AST, not runtime schema
    # This is a limitation we work around
    directives = getattr(field, 'directives', [])

    for directive in directives:
        if directive.name == "depends_on":
            _validate_depends_on(
                directive, type_name, field_name, schema, result
            )
        elif directive.name == "requires_function":
            _validate_requires_function(
                directive, type_name, field_name, result, db_connection
            )
        elif directive.name == "view_cached":
            _validate_view_cached(directive, type_name, field_name, result)
        elif directive.name == "cost_units":
            _validate_cost_units(directive, type_name, field_name, result)


def _validate_depends_on(
    directive: Any,
    type_name: str,
    field_name: str,
    schema: GraphQLSchema,
    result: DirectiveValidationResult,
) -> None:
    """Validate @depends_on directive"""

    views = directive.arguments.get("views", [])
    if not views:
        result.add_warning(
            f"{type_name}.{field_name}: @depends_on views list is empty"
        )
        return

    # Check if views exist in schema (best effort)
    for view in views:
        if view not in schema.type_map:
            result.add_warning(
                f"{type_name}.{field_name}: @depends_on references "
                f"non-existent view '{view}'"
            )


def _validate_requires_function(
    directive: Any,
    type_name: str,
    field_name: str,
    result: DirectiveValidationResult,
    db_connection: Optional[Any] = None,
) -> None:
    """Validate @requires_function directive"""

    func_name = directive.arguments.get("name")
    if not func_name:
        result.add_error(
            f"{type_name}.{field_name}: @requires_function missing 'name' argument"
        )
        return

    # If database connection available, check function existence
    if db_connection:
        # Check in database
        if not _function_exists_in_db(db_connection, func_name):
            result.add_error(
                f"{type_name}.{field_name}: @requires_function "
                f"'{func_name}' not found in database"
            )


def _validate_view_cached(
    directive: Any,
    type_name: str,
    field_name: str,
    result: DirectiveValidationResult,
) -> None:
    """Validate @view_cached directive"""

    ttl = directive.arguments.get("ttl")
    if ttl is None:
        result.add_error(
            f"{type_name}.{field_name}: @view_cached missing 'ttl' argument"
        )
        return

    if ttl <= 0:
        result.add_error(
            f"{type_name}.{field_name}: @view_cached ttl must be positive, "
            f"got {ttl}"
        )


def _validate_cost_units(
    directive: Any,
    type_name: str,
    field_name: str,
    result: DirectiveValidationResult,
) -> None:
    """Validate @cost_units directive"""

    estimate = directive.arguments.get("estimate")
    if estimate is None:
        result.add_error(
            f"{type_name}.{field_name}: @cost_units missing 'estimate' argument"
        )
        return

    if estimate < 0:
        result.add_error(
            f"{type_name}.{field_name}: @cost_units estimate must be "
            f"non-negative, got {estimate}"
        )
    elif estimate > 1000:
        result.add_warning(
            f"{type_name}.{field_name}: @cost_units estimate is very high "
            f"({estimate}, consider breaking down query)"
        )


def _function_exists_in_db(db_connection: Any, func_name: str) -> bool:
    """Check if function exists in PostgreSQL database"""
    try:
        # Parse function name (schema.function or just function)
        if '.' in func_name:
            schema, func = func_name.split('.', 1)
        else:
            schema = "public"
            func = func_name

        # Query information_schema
        # This would need to be async, simplified here
        # In real code, use: await db_connection.fetchone(...)
        return True  # Placeholder
    except Exception:
        return False
```

**Acceptance:** Directive validation works independently

---

### Step 4: Integration into Schema Builder (30 minutes)

**File:** `src/fraiseql/gql/schema_builder.py`

Integrate validation into schema building:

```python
def build_fraiseql_schema(
    types,
    queries,
    mutations=None,
    subscriptions=None,
    validate_directives: bool = True,
) -> GraphQLSchema:
    """Build FraiseQL GraphQL schema with metadata directives.

    Args:
        types: List of FraiseQL type classes
        queries: List of query resolvers
        mutations: List of mutation resolvers
        subscriptions: List of subscription resolvers
        validate_directives: Whether to validate directives

    Returns:
        GraphQL schema
    """

    # ... existing schema building ...

    # Add metadata directives
    metadata_directives = create_view_metadata_directives()
    all_directives = [
        *schema.directives,
        *metadata_directives,
    ]

    schema = GraphQLSchema(
        query=query_type,
        mutation=mutation_type,
        subscription=subscription_type,
        types=all_types,
        directives=all_directives,
    )

    # ✅ NEW: Validate directives (optional, for dev/staging)
    if validate_directives:
        validation_result = validate_schema_directives(schema)
        if validation_result.warnings:
            logger.warning(
                f"Schema validation: {len(validation_result.warnings)} warnings"
            )
        if validation_result.errors:
            logger.error(
                f"Schema validation: {len(validation_result.errors)} errors"
            )

    return schema
```

**Acceptance:** Validation integrated into schema building

---

### Step 5: Add Introspection Support (15 minutes)

**File:** `src/fraiseql/fastapi/routers.py`

Ensure directives appear in introspection queries:

```python
# GraphQL introspection queries should automatically show directives
# This just requires that directives are in schema.directives

# Test introspection query:
query IntrospectionWithDirectives {
  __schema {
    directives {
      name
      locations
      args {
        name
        type {
          kind
          name
        }
      }
      description
    }
  }
}

# Should show:
# - view_cached
# - depends_on
# - requires_function
# - cost_units

# Also test field-level introspection:
query FieldWithDirectives {
  __type(name: "User") {
    fields {
      name
      isDeprecated
      deprecationReason
    }
  }
}
```

**Note:** Directives on fields in runtime schema require special handling. GraphQL-core separates AST (where directives are) from runtime schema. This is a known limitation.

**Acceptance:** Directives appear in introspection

---

### Step 6: Write Unit Tests (1 hour)

**File:** `tests/unit/gql/test_schema_directives.py` (NEW)

```python
"""Tests for schema metadata directives"""

import pytest
from graphql import GraphQLSchema, build_schema

from fraiseql.gql.schema_directives import (
    ViewCacheDirective,
    DependsOnDirective,
    RequiresFunctionDirective,
    CostUnitsDirective,
)
from fraiseql.gql.directive_validator import validate_schema_directives


class TestDirectiveClasses:
    """Test directive dataclass validation"""

    def test_view_cache_directive_valid(self):
        """Valid view_cached directive"""
        directive = ViewCacheDirective(ttl=3600)
        assert directive.validate() == []

    def test_view_cache_directive_negative_ttl(self):
        """Negative TTL is invalid"""
        directive = ViewCacheDirective(ttl=-1)
        errors = directive.validate()
        assert len(errors) > 0
        assert "positive" in errors[0].lower()

    def test_depends_on_directive_valid(self):
        """Valid depends_on directive"""
        directive = DependsOnDirective(views=["tb_user", "tb_profile"])
        assert directive.validate() == []

    def test_depends_on_directive_empty_views(self):
        """Empty views list is invalid"""
        directive = DependsOnDirective(views=[])
        errors = directive.validate()
        assert len(errors) > 0
        assert "empty" in errors[0].lower()

    def test_requires_function_directive_valid(self):
        """Valid requires_function directive"""
        directive = RequiresFunctionDirective(name="fn_validate_user")
        assert directive.validate() == []

    def test_requires_function_directive_empty_name(self):
        """Empty function name is invalid"""
        directive = RequiresFunctionDirective(name="")
        errors = directive.validate()
        assert len(errors) > 0

    def test_cost_units_directive_valid(self):
        """Valid cost_units directive"""
        directive = CostUnitsDirective(estimate=5.0)
        assert directive.validate() == []

    def test_cost_units_directive_negative(self):
        """Negative estimate is invalid"""
        directive = CostUnitsDirective(estimate=-1.0)
        errors = directive.validate()
        assert len(errors) > 0
        assert "negative" in errors[0].lower()


class TestDirectiveDefinitions:
    """Test GraphQL directive definitions"""

    def test_view_cached_directive_definition(self):
        """@view_cached directive is properly defined"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()
        view_cached = next(d for d in directives if d.name == "view_cached")

        assert view_cached is not None
        assert "FIELD_DEFINITION" in str(view_cached.locations)
        assert "ttl" in view_cached.args

    def test_depends_on_directive_definition(self):
        """@depends_on directive is properly defined"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()
        depends_on = next(d for d in directives if d.name == "depends_on")

        assert depends_on is not None
        assert "views" in depends_on.args

    def test_requires_function_directive_definition(self):
        """@requires_function directive is properly defined"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()
        requires_func = next(
            d for d in directives if d.name == "requires_function"
        )

        assert requires_func is not None
        assert "name" in requires_func.args

    def test_cost_units_directive_definition(self):
        """@cost_units directive is properly defined"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()
        cost_units = next(d for d in directives if d.name == "cost_units")

        assert cost_units is not None
        assert "estimate" in cost_units.args


class TestSchemaIntrospection:
    """Test that directives appear in introspection"""

    def test_directives_in_introspection_query(self):
        """Directives appear in __schema.directives"""
        from fraiseql.gql.schema_builder import build_fraiseql_schema

        # Build simple schema
        schema = build_fraiseql_schema(
            types=[],
            queries=[],
            validate_directives=False,
        )

        # Check directives in schema
        directive_names = {d.name for d in schema.directives}

        assert "view_cached" in directive_names
        assert "depends_on" in directive_names
        assert "requires_function" in directive_names
        assert "cost_units" in directive_names

    def test_directive_has_description(self):
        """Directives have user-friendly descriptions"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()

        for directive in directives:
            assert directive.description is not None
            assert len(directive.description) > 0
            assert len(directive.description) < 500  # Not too long

    def test_directive_args_have_descriptions(self):
        """Directive arguments have descriptions"""
        from fraiseql.gql.schema_builder import create_view_metadata_directives

        directives = create_view_metadata_directives()

        for directive in directives:
            for arg_name, arg_def in directive.args.items():
                assert arg_def.description is not None
                assert len(arg_def.description) > 0
```

**Acceptance:** All directive tests pass

---

### Step 7: Integration Tests (30 minutes)

**File:** `tests/integration/gql/test_directives_integration.py` (NEW)

```python
"""Integration tests for metadata directives"""

import pytest
from graphql import graphql_sync, get_introspection_query

from fraiseql.gql.schema_builder import build_fraiseql_schema


@pytest.fixture
def schema_with_directives():
    """Create test schema with directives"""
    return build_fraiseql_schema(
        types=[],
        queries=[],
        validate_directives=False,
    )


class TestDirectiveIntrospection:
    """Test directives through introspection"""

    @pytest.mark.asyncio
    async def test_introspection_shows_directives(self, schema_with_directives):
        """Introspection query lists all directives"""
        query = """
        query {
            __schema {
                directives {
                    name
                    description
                    locations
                }
            }
        }
        """

        result = graphql_sync(schema_with_directives, query)

        assert result.errors is None
        directives = result.data['__schema']['directives']
        directive_names = {d['name'] for d in directives}

        assert 'view_cached' in directive_names
        assert 'depends_on' in directive_names
        assert 'requires_function' in directive_names
        assert 'cost_units' in directive_names

    @pytest.mark.asyncio
    async def test_directive_has_correct_args(self, schema_with_directives):
        """Directives have correct arguments"""
        query = """
        query {
            __schema {
                directives {
                    name
                    args {
                        name
                        type {
                            kind
                            name
                        }
                    }
                }
            }
        }
        """

        result = graphql_sync(schema_with_directives, query)

        assert result.errors is None
        directives = result.data['__schema']['directives']

        view_cached = next(
            (d for d in directives if d['name'] == 'view_cached'), None
        )
        assert view_cached is not None
        arg_names = {arg['name'] for arg in view_cached['args']}
        assert 'ttl' in arg_names

    @pytest.mark.asyncio
    async def test_directive_args_have_correct_types(self, schema_with_directives):
        """Directive arguments have correct types"""
        query = """
        query {
            __schema {
                directives {
                    name
                    args {
                        name
                        type { kind ofType { kind name } }
                    }
                }
            }
        }
        """

        result = graphql_sync(schema_with_directives, query)

        assert result.errors is None
        directives = result.data['__schema']['directives']

        depends_on = next(
            (d for d in directives if d['name'] == 'depends_on'), None
        )
        assert depends_on is not None
        # views arg should be [String!]!
        views_arg = next(
            (arg for arg in depends_on['args'] if arg['name'] == 'views'), None
        )
        assert views_arg is not None


class TestDirectiveDocumentation:
    """Test directive documentation"""

    @pytest.mark.asyncio
    async def test_all_directives_have_descriptions(self, schema_with_directives):
        """All directives have user-friendly descriptions"""
        query = """
        query {
            __schema {
                directives {
                    name
                    description
                }
            }
        }
        """

        result = graphql_sync(schema_with_directives, query)

        directives = result.data['__schema']['directives']
        metadata_directives = [
            d for d in directives
            if d['name'] in [
                'view_cached', 'depends_on', 'requires_function', 'cost_units'
            ]
        ]

        for directive in metadata_directives:
            assert directive['description'] is not None
            assert len(directive['description']) > 10
            assert len(directive['description']) < 500
```

**Acceptance:** Integration tests pass

---

## Part 4: Complete Code Changes Summary

### Files Created
1. `src/fraiseql/gql/schema_directives.py` - Directive dataclasses
2. `src/fraiseql/gql/directive_validator.py` - Validation logic
3. `tests/unit/gql/test_schema_directives.py` - Unit tests
4. `tests/integration/gql/test_directives_integration.py` - Integration tests

### Files Modified
1. `src/fraiseql/gql/schema_builder.py` - Add directives to schema

---

## Part 5: Usage Examples

### Example 1: User Type with All Directives

```python
from fraiseql import type, Field

@type(sql_source="tv_user_with_extended_profile")
class User:
    id: UUID
    name: str

    profile: dict = Field(
        description="Extended profile data",
        directives=[
            "@view_cached(ttl: 3600)",          # Hourly refresh
            "@depends_on(views: [\"tb_user\", \"tb_profile\"])",
            "@requires_function(name: \"fn_validate_profile\")",
            "@cost_units(estimate: 2.5)"
        ]
    )

    posts: list[dict] = Field(
        description="User's published posts",
        directives=[
            "@view_cached(ttl: 1800)",          # 30-min cache
            "@depends_on(views: [\"tb_post\", \"tb_publish_status\"])",
            "@cost_units(estimate: 5.0)"
        ]
    )
```

### Example 2: GraphQL Schema Definition

```graphql
type User {
  id: ID!
  name: String!

  profile: JSON
    @view_cached(ttl: 3600)
    @depends_on(views: ["tb_user", "tb_profile"])
    @requires_function(name: "fn_validate_profile")
    @cost_units(estimate: 2.5)

  posts(first: Int, after: String): [Post!]!
    @view_cached(ttl: 1800)
    @depends_on(views: ["tb_post"])
    @cost_units(estimate: 5.0)
}
```

### Example 3: Introspection Query

```graphql
query GetDirectives {
  __schema {
    directives {
      name
      description
      locations
      args {
        name
        description
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
}

# Returns metadata about all directives, including:
# - view_cached
# - depends_on
# - requires_function
# - cost_units
```

---

## Part 6: Tooling Integration

### View Dependency Graph

Directives enable automatic dependency graph generation:

```python
# Tool: Generate view dependency graph
def generate_dependency_graph(schema):
    """Generate GraphQL view dependency graph"""
    graph = {}

    for type_name, type_def in schema.type_map.items():
        for field_name, field in type_def.fields.items():
            # Extract @depends_on directive
            directives = field.directives  # If available
            # Add to graph
            # ...
    return graph

# Output: Directed graph showing:
# User.profile depends on -> tb_user, tb_profile
# User.posts depends on -> tb_post
# Post.comments depends on -> tb_comment
# ...
```

### Query Cost Analysis

Directives enable cost-based query limiting:

```python
# Tool: Analyze query cost
def calculate_query_cost(query_ast, schema):
    """Calculate total query cost"""
    total_cost = 0

    for field in query_ast.selections:
        cost_directive = find_directive(field, "cost_units")
        if cost_directive:
            estimate = cost_directive.args["estimate"]
            total_cost += estimate

    return total_cost

# Usage:
# query { users { posts { comments } } }
# Cost = 1.0 (users) + 5.0 (posts) + 3.0 (comments) = 9.0 cost units
# Can reject if cost > MAX_QUERY_COST
```

---

## Part 7: Migration Guide

### Breaking Changes
**None.** Directives are purely additive.

### For New Schemas
Use directives on fields that come from materialized views:

```python
@type(sql_source="tv_user_with_extended_data")
class User:
    id: UUID

    # NEW: Document view metadata
    extended_data: dict = Field(
        directives=[
            "@view_cached(ttl: 3600)",
            "@depends_on(views: [\"tb_user\", \"tb_extended\"])",
        ]
    )
```

### For Existing Schemas
Directives are optional. Gradually add to high-value fields.

---

## Part 8: Success Criteria

### Code Quality
- [ ] All unit tests pass (15+ new tests)
- [ ] All integration tests pass
- [ ] No regressions in existing tests
- [ ] Code coverage > 95% for directive modules
- [ ] Passes linting (ruff, black)

### Functionality
- [ ] Directives appear in introspection
- [ ] Directive validation works
- [ ] All 4 directive types work correctly
- [ ] Error messages are clear
- [ ] Directives don't affect query execution

### Documentation
- [ ] Clear directive descriptions
- [ ] Argument descriptions
- [ ] Usage examples
- [ ] Tool integration guide

### Performance
- [ ] Schema building time unchanged
- [ ] Query execution unchanged
- [ ] Introspection unaffected

---

## Part 9: Dependencies & Prerequisites

### Code Dependencies
- `graphql-core >= 3.2` (already required)
- No new external dependencies

### Files Modified
1. `src/fraiseql/gql/schema_builder.py`

### Files Added
1. `src/fraiseql/gql/schema_directives.py`
2. `src/fraiseql/gql/directive_validator.py`
3. `tests/unit/gql/test_schema_directives.py`
4. `tests/integration/gql/test_directives_integration.py`

---

## Part 10: Implementation Checklist

### Development
- [ ] Create `schema_directives.py` with directive classes
- [ ] Create `directive_validator.py` with validation
- [ ] Write unit tests for directives
- [ ] Write integration tests
- [ ] Test introspection

### Integration
- [ ] Update `schema_builder.py` to include directives
- [ ] Verify directives appear in schema
- [ ] Test with real FraiseQL schema

### Validation
- [ ] Run full test suite (6000+ tests)
- [ ] Verify no regressions
- [ ] Code review
- [ ] Merge to dev

---

## Conclusion

This implementation adds **metadata directives** to FraiseQL's GraphQL schema, enabling:

✅ **Better documentation** - View dependencies explicit in schema
✅ **Tool integration** - Dependency graphs, cost analysis
✅ **Schema validation** - Ensure required functions exist
✅ **Query planning** - Cost-based query optimization

The directives are:
- **Purely semantic** (don't affect execution)
- **Backward compatible** (optional)
- **Well-tested** (15+ unit + 10+ integration tests)
- **User-friendly** (clear descriptions)

**Effort estimate: 2-4 hours**
**Complexity: Low-Moderate**
**Risk: Low**
**Value: High**

Status: ✅ Ready for implementation
