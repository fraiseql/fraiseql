# LLM Integration

Integrate Large Language Models with FraiseQL GraphQL APIs: schema introspection for LLM context, structured query generation, and safe execution patterns.

## Overview

FraiseQL's GraphQL schema provides structured, type-safe interfaces that LLMs can understand and generate queries for. **FraiseQL automatically generates rich schema documentation from Python docstrings**, making your API self-documenting for LLM consumption.

**Why FraiseQL is Ideal for LLM Integration:**

- **Auto-documentation**: Docstrings automatically become GraphQL descriptions (no manual schema docs)
- **Rich introspection**: LLMs can discover types, fields, and documentation via GraphQL introspection
- **Type safety**: Strong typing prevents invalid query generation
- **Built-in safety**: Complexity limits and validation protect against expensive queries

**Key Patterns:**

- Schema introspection for LLM context
- Structured query generation from natural language
- Query validation and sanitization
- Complexity limits for LLM-generated queries
- Prompt engineering for schema understanding
- Error handling and recovery

## Table of Contents

- [Schema Introspection for LLMs](#schema-introspection-for-llms)
- [Prompt Engineering](#prompt-engineering)
- [Query Generation](#query-generation)
- [Safety Mechanisms](#safety-mechanisms)
- [Error Handling](#error-handling)
- [Best Practices](#best-practices)

## Schema Introspection for LLMs

### GraphQL Schema as LLM Context

GraphQL schema provides perfect structure for LLM understanding:

```python
from fraiseql import query
from graphql import get_introspection_query, graphql_sync

@query
async def get_schema_for_llm(info) -> dict:
    """Get GraphQL schema formatted for LLM context."""
    schema = info.schema

    # Get full introspection
    introspection_query = get_introspection_query()
    result = graphql_sync(schema, introspection_query)

    # Simplify for LLM
    simplified = {
        "types": [],
        "queries": [],
        "mutations": []
    }

    for type_def in result.data["__schema"]["types"]:
        if type_def["name"].startswith("__"):
            continue  # Skip internal types

        simplified_type = {
            "name": type_def["name"],
            "kind": type_def["kind"],
            "description": type_def.get("description"),
            "fields": []
        }

        if type_def.get("fields"):
            for field in type_def["fields"]:
                simplified_type["fields"].append({
                    "name": field["name"],
                    "type": _format_type(field["type"]),
                    "description": field.get("description"),
                    "args": [
                        {
                            "name": arg["name"],
                            "type": _format_type(arg["type"]),
                            "description": arg.get("description")
                        }
                        for arg in field.get("args", [])
                    ]
                })

        simplified["types"].append(simplified_type)

    return simplified

def _format_type(type_ref: dict) -> str:
    """Format GraphQL type for LLM readability."""
    if type_ref["kind"] == "NON_NULL":
        return f"{_format_type(type_ref['ofType'])}!"
    elif type_ref["kind"] == "LIST":
        return f"[{_format_type(type_ref['ofType'])}]"
    else:
        return type_ref["name"]
```

### Compact Schema Representation

Provide minimal schema for LLM token efficiency:

```python
def schema_to_llm_prompt(schema: dict) -> str:
    """Convert GraphQL schema to compact prompt format."""
    prompt = "# GraphQL Schema\n\n"

    # Queries
    prompt += "## Queries\n\n"
    query_type = next(t for t in schema["types"] if t["name"] == "Query")
    for field in query_type["fields"]:
        args = ", ".join(f"{a['name']}: {a['type']}" for a in field["args"])
        prompt += f"- {field['name']}({args}): {field['type']}\n"
        if field.get("description"):
            prompt += f"  {field['description']}\n"

    # Mutations
    prompt += "\n## Mutations\n\n"
    mutation_type = next((t for t in schema["types"] if t["name"] == "Mutation"), None)
    if mutation_type:
        for field in mutation_type["fields"]:
            args = ", ".join(f"{a['name']}: {a['type']}" for a in field["args"])
            prompt += f"- {field['name']}({args}): {field['type']}\n"
            if field.get("description"):
                prompt += f"  {field['description']}\n"

    # Types
    prompt += "\n## Types\n\n"
    for type_def in schema["types"]:
        if type_def["kind"] == "OBJECT" and type_def["name"] not in ["Query", "Mutation"]:
            prompt += f"### {type_def['name']}\n"
            for field in type_def.get("fields", []):
                prompt += f"- {field['name']}: {field['type']}\n"
            prompt += "\n"

    return prompt
```

## Prompt Engineering

### Query Generation Prompts

Structured prompts for accurate GraphQL generation:

```python
QUERY_GENERATION_PROMPT = """
You are a GraphQL query generator. Given a natural language request and a GraphQL schema,
generate a valid GraphQL query.

Schema:
{schema}

Rules:
1. Use only fields that exist in the schema
2. Include only requested fields in the selection set
3. Use proper argument types
4. Limit queries to reasonable depth (max 3 levels)
5. Add __typename for debugging if needed

User Request: {user_request}

Generate ONLY the GraphQL query, no explanation:
"""

async def generate_query_with_llm(user_request: str, llm_client) -> str:
    """Generate GraphQL query using LLM."""
    # Get schema
    schema = await get_schema_for_llm(None)
    schema_text = schema_to_llm_prompt(schema)

    # Build prompt
    prompt = QUERY_GENERATION_PROMPT.format(
        schema=schema_text,
        user_request=user_request
    )

    # Call LLM
    response = await llm_client.complete(prompt)

    # Extract query
    query_text = extract_graphql_query(response)

    return query_text

def extract_graphql_query(llm_response: str) -> str:
    """Extract GraphQL query from LLM response."""
    # Remove markdown code blocks
    if "```graphql" in llm_response:
        query = llm_response.split("```graphql")[1].split("```")[0].strip()
    elif "```" in llm_response:
        query = llm_response.split("```")[1].split("```")[0].strip()
    else:
        query = llm_response.strip()

    return query
```

## Query Generation

### Complete LLM Pipeline

```python
from graphql import parse, validate, GraphQLError
from typing import Any

class LLMQueryGenerator:
    """Generate and execute GraphQL queries from natural language."""

    def __init__(self, schema, llm_client, max_complexity: int = 50):
        self.schema = schema
        self.llm_client = llm_client
        self.max_complexity = max_complexity

    async def query_from_natural_language(
        self,
        user_request: str,
        context: dict
    ) -> dict[str, Any]:
        """Convert natural language to GraphQL and execute."""
        # 1. Generate query
        query_text = await generate_query_with_llm(user_request, self.llm_client)

        # 2. Validate syntax
        try:
            document = parse(query_text)
        except GraphQLError as e:
            raise ValueError(f"Invalid GraphQL syntax: {e}")

        # 3. Validate against schema
        errors = validate(self.schema, document)
        if errors:
            raise ValueError(f"Schema validation failed: {errors}")

        # 4. Check complexity
        complexity = calculate_query_complexity(document, self.schema)
        if complexity > self.max_complexity:
            raise ValueError(f"Query too complex: {complexity} > {self.max_complexity}")

        # 5. Execute
        from graphql import graphql

        result = await graphql(
            self.schema,
            query_text,
            context_value=context
        )

        if result.errors:
            raise ValueError(f"Execution errors: {result.errors}")

        return result.data

def calculate_query_complexity(document, schema) -> int:
    """Calculate query complexity score."""
    # Simple implementation: count fields
    from graphql import visit, BREAK

    complexity = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal complexity
        complexity += 1

    visit(document, {"Field": {"enter": enter_field}})

    return complexity
```

### Few-Shot Learning

Provide examples to improve LLM accuracy:

```python
FEW_SHOT_EXAMPLES = """
Example 1:
Request: "Get all users"
Query:
query {
  users {
    id
    name
    email
  }
}

Example 2:
Request: "Get user with ID 123 and their orders"
Query:
query {
  user(id: "123") {
    id
    name
    orders {
      id
      total
      status
    }
  }
}

Example 3:
Request: "Find orders created in the last week"
Query:
query {
  orders(
    filter: { createdAt: { gte: "2024-01-01" } }
    orderBy: { createdAt: DESC }
    limit: 100
  ) {
    id
    total
    status
    createdAt
  }
}

Now generate a query for:
Request: {user_request}
"""
```

## Safety Mechanisms

### Query Complexity Limits

Prevent expensive queries:

```python
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    complexity_enabled=True,
    complexity_max_score=100,  # Lower for LLM queries
    complexity_max_depth=3,    # Prevent deep nesting
    complexity_default_list_size=10
)
```

### Depth Limiting

```python
def enforce_max_depth(document, max_depth: int = 3) -> None:
    """Enforce maximum query depth."""
    from graphql import visit

    current_depth = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal current_depth
        current_depth = len([a for a in ancestors if a.get("kind") == "Field"])
        if current_depth > max_depth:
            raise ValueError(f"Query depth {current_depth} exceeds maximum {max_depth}")

    visit(document, {"Field": {"enter": enter_field}})
```

### Allowed Operations Whitelist

```python
class SafeLLMExecutor:
    """Execute only safe, read-only queries from LLM."""

    ALLOWED_ROOT_FIELDS = [
        "users", "user",
        "orders", "order",
        "products", "product"
    ]

    @classmethod
    def validate_safe_query(cls, document) -> None:
        """Ensure query only uses allowed fields."""
        from graphql import visit

        def enter_field(node, key, parent, path, ancestors):
            # Check root fields
            if len(ancestors) == 3:  # Root query field
                if node.name.value not in cls.ALLOWED_ROOT_FIELDS:
                    raise ValueError(f"Field '{node.name.value}' not allowed for LLM queries")

        visit(document, {"Field": {"enter": enter_field}})

    async def execute_llm_query(self, query_text: str, context: dict) -> dict:
        """Execute LLM-generated query with safety checks."""
        document = parse(query_text)

        # Check for mutations
        has_mutation = any(
            op.operation == "mutation"
            for op in document.definitions
            if hasattr(op, "operation")
        )
        if has_mutation:
            raise ValueError("Mutations not allowed for LLM queries")

        # Validate safe operations
        self.validate_safe_query(document)

        # Check depth
        enforce_max_depth(document, max_depth=3)

        # Execute
        from graphql import graphql
        result = await graphql(self.schema, query_text, context_value=context)

        return result.data
```

## Error Handling

### Query Refinement Loop

Automatically refine queries on errors:

```python
async def generate_and_refine_query(
    user_request: str,
    llm_client,
    schema,
    max_attempts: int = 3
) -> str:
    """Generate query with automatic refinement on errors."""
    for attempt in range(max_attempts):
        # Generate query
        query_text = await generate_query_with_llm(user_request, llm_client)

        # Validate
        try:
            document = parse(query_text)
            errors = validate(schema, document)

            if not errors:
                return query_text  # Success

            # Refine prompt with error feedback
            error_feedback = "\n".join(str(e) for e in errors)
            user_request += f"\n\nPrevious attempt failed with errors:\n{error_feedback}\n\nPlease fix these errors."

        except Exception as e:
            # Syntax error
            user_request += f"\n\nPrevious attempt had syntax error: {e}\n\nPlease generate valid GraphQL."

    raise ValueError(f"Failed to generate valid query after {max_attempts} attempts")
```

### Graceful Degradation

```python
async def execute_with_fallback(query_text: str, context: dict) -> dict:
    """Execute with fallback to simpler query on failure."""
    try:
        # Try full query
        result = await graphql(schema, query_text, context_value=context)
        if not result.errors:
            return result.data

        # Try with fewer fields
        simplified_query = simplify_query(query_text)
        result = await graphql(schema, simplified_query, context_value=context)
        if not result.errors:
            return {
                "data": result.data,
                "warning": "Used simplified query due to errors"
            }

    except Exception as e:
        # Fall back to error message
        return {
            "error": str(e),
            "suggestion": "Try a simpler query or rephrase your request"
        }

def simplify_query(query_text: str) -> str:
    """Remove nested fields to simplify query."""
    # Parse and remove fields beyond depth 2
    # This is a simplified implementation
    document = parse(query_text)
    # ... implementation to remove deep fields
    return print_ast(document)
```

## Best Practices

### 1. Auto-Documentation from Docstrings

**FraiseQL automatically extracts Python docstrings into GraphQL schema descriptions**, making your API self-documenting for LLM consumption.

**How It Works:**
- Type docstrings become GraphQL type descriptions
- `Fields:` section in docstring defines field descriptions
- Query/mutation docstrings become operation descriptions
- All descriptions are available via GraphQL introspection

**Write Once, Document Everywhere:**

```python
from fraiseql import type, query
from uuid import UUID

@type(sql_source="v_user")
class User:
    """User account with profile information and order history.

    Users are created during registration and can place orders,
    manage their profile, and view order history.

    Fields:
        id: Unique user identifier (UUID format)
        email: User's email address (used for login)
        name: User's full name
        created_at: Account creation timestamp
        orders: All orders placed by this user, sorted by creation date descending
    """

    id: UUID
    email: str
    name: str
    created_at: datetime
    orders: list['Order']

@query
async def user(info, id: UUID) -> User | None:
    """Get a single user by ID.

    Args:
        id: User UUID (format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)

    Returns:
        User object with all profile fields, or null if not found.

    Example:
        query {
          user(id: "123e4567-e89b-12d3-a456-426614174000") {
            id
            name
            email
          }
        }
    """
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})
```

**What LLMs See (via introspection):**

```json
{
  "types": [
    {
      "name": "User",
      "description": "User account with profile information and order history.\n\nUsers are created during registration and can place orders,\nmanage their profile, and view order history.",
      "fields": [
        {
          "name": "id",
          "type": "String!",
          "description": "Unique user identifier (UUID format)."
        },
        {
          "name": "email",
          "type": "String!",
          "description": "User's email address (used for login)."
        },
        {
          "name": "name",
          "type": "String!",
          "description": "User's full name."
        },
        {
          "name": "orders",
          "type": "[Order!]!",
          "description": "All orders placed by this user, sorted by creation date descending."
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "user",
      "description": "Get a single user by ID.\n\nArgs:\n    id: User UUID (format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)\n\nReturns:\n    User object with all profile fields, or null if not found.\n\nExample:\n    query {\n      user(id: \"123e4567-e89b-12d3-a456-426614174000\") {\n        id\n        name\n        email\n      }\n    }",
      "type": "User",
      "args": [
        {
          "name": "id",
          "type": "String!",
          "description": null
        }
      ]
    }
  ]
}
```

**Best Practices for LLM-Friendly Docstrings:**

1. **Include examples in query/mutation docstrings** - LLMs learn patterns from examples
2. **Document field formats** - Specify UUID format, date formats, enum values
3. **Explain relationships** - "User's orders" vs "Orders user can access"
4. **Note sorting/filtering** - "sorted by creation date descending"
5. **Document edge cases** - "returns null if not found", "empty list if no results"

**No Manual Schema Documentation Needed:**

```python
# ✅ Good: Write docstrings once with Fields section
@type(sql_source="v_product")
class Product:
    """Product available for purchase.

    Fields:
        sku: Stock keeping unit (format: ABC-12345)
        name: Product name
        price: Price in USD cents (e.g., 2999 = $29.99)
        in_stock: Whether product is currently available
    """

    sku: str
    name: str
    price: Decimal
    in_stock: bool

# ❌ Bad: Don't manually maintain separate schema docs
# LLMs automatically read descriptions from introspection
```

### 2. Query Templates

Provide reusable templates for common patterns:

```python
QUERY_TEMPLATES = {
    "list_all": """
query List{entities} {
  {entities} {
    id
    {fields}
  }
}
""",
    "get_by_id": """
query Get{entity}($id: ID!) {
  {entity}(id: $id) {
    id
    {fields}
  }
}
""",
    "search": """
query Search{entities}($query: String!) {
  {entities}(filter: { search: $query }) {
    id
    {fields}
  }
}
"""
}

def fill_template(template_name: str, **kwargs) -> str:
    """Fill query template with parameters."""
    template = QUERY_TEMPLATES[template_name]
    return template.format(**kwargs)

# Usage
query = fill_template(
    "list_all",
    entities="users",
    fields="name\nemail"
)
```

### 3. Rate Limiting for LLM Endpoints

```python
from fraiseql.security import RateLimitRule, RateLimit

llm_rate_limits = [
    RateLimitRule(
        path_pattern="/graphql/llm",
        rate_limit=RateLimit(requests=10, window=60),  # 10 per minute
        message="LLM query rate limit exceeded"
    )
]
```

### 4. Logging and Monitoring

```python
import logging

logger = logging.getLogger(__name__)

async def execute_llm_query_with_logging(
    user_request: str,
    query_text: str,
    user_id: str
) -> dict:
    """Execute LLM query with comprehensive logging."""
    logger.info(
        "LLM query execution",
        extra={
            "user_id": user_id,
            "natural_language": user_request,
            "generated_query": query_text,
            "timestamp": datetime.utcnow().isoformat()
        }
    )

    try:
        result = await execute_safe_query(query_text)

        logger.info(
            "LLM query success",
            extra={
                "user_id": user_id,
                "result_size": len(str(result))
            }
        )

        return result

    except Exception as e:
        logger.error(
            "LLM query failed",
            extra={
                "user_id": user_id,
                "error": str(e),
                "query": query_text
            }
        )
        raise
```

## Next Steps

- [Security](../production/security.md) - Securing LLM endpoints
- [Performance](../performance/index.md) - Optimizing LLM-generated queries
- [Authentication](authentication.md) - User context for LLM queries
- [Monitoring](../production/monitoring.md) - Tracking LLM query patterns
