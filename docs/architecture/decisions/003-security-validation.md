# ADR-003: Security and Validation Strategy

## Status
Accepted

## Context
FraiseQL accepts user input through GraphQL queries and mutations that get translated to SQL. This creates potential security risks including SQL injection, XSS, and other attacks. We need comprehensive validation while maintaining good performance.

## Decision
We will implement defense-in-depth with:
- **Input validation**: Pattern matching for suspicious inputs
- **Parameterized queries**: All SQL uses psycopg's parameterization
- **Type validation**: Pydantic models validate all inputs
- **Query depth limiting**: Prevent deeply nested GraphQL queries
- **Rate limiting**: Prevent abuse and DoS attacks

## Consequences

### Positive
- **Security**: Multiple layers of protection
- **User feedback**: Clear error messages for invalid input
- **Performance**: Validation happens early, before database calls
- **Compliance**: Helps meet security requirements
- **Auditability**: All validation is logged

### Negative
- **False positives**: Legitimate queries might be blocked
- **Performance overhead**: Validation adds latency
- **Complexity**: Multiple validation layers to maintain

### Mitigation
- Configurable validation rules
- Bypass options for trusted sources
- Performance monitoring of validation overhead
- Regular review of blocked requests

## Implementation

### Input Validation Layer
```python
# src/fraiseql/security/validators.py
class InputValidator:
    """Validates user input for security threats."""

    SUSPICIOUS_SQL_PATTERNS = [
        (r"(--|#|/\*|\*/)", "SQL comment syntax detected"),
        (r"\b(union\s+select|drop\s+table|delete\s+from)\b",
         "Suspicious SQL keyword pattern"),
        (r";\s*(select|insert|update|delete|drop)", "Stacked query attempt"),
    ]

    @classmethod
    def validate_field_value(cls, field: str, value: Any) -> ValidationResult:
        """Validate a single field value."""
        errors = []
        warnings = []

        if isinstance(value, str):
            # Check for SQL injection patterns
            for pattern, message in cls.SUSPICIOUS_SQL_PATTERNS:
                if re.search(pattern, value, re.IGNORECASE):
                    errors.append(f"{field}: {message}")

            # Check for XSS patterns
            if cls._contains_script_tags(value):
                errors.append(f"{field}: Script tags not allowed")

        return ValidationResult(
            is_valid=len(errors) == 0,
            errors=errors,
            warnings=warnings,
            sanitized_value=value
        )
```

### Parameterized Query Generation
```python
# src/fraiseql/sql/where_generator.py
def build_operator_composed(
    path_sql: SQL,
    op: str,
    val: object,
    field_type: type | None = None,
) -> Composed:
    """Build parameterized SQL using psycopg Composed."""
    # NEVER concatenate user input directly
    # Always use Literal() for values
    if op == "eq":
        return Composed([path_sql, SQL(" = "), Literal(val)])
    elif op == "in":
        # Safe IN clause construction
        literals = [Literal(v) for v in val]
        parts = [path_sql, SQL(" IN (")]
        for i, lit in enumerate(literals):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(lit)
        parts.append(SQL(")"))
        return Composed(parts)
```

### Query Depth Limiting
```python
# src/fraiseql/analysis/query_complexity.py
class QueryComplexityAnalyzer:
    """Analyzes GraphQL query complexity."""

    def calculate_depth(self, query: DocumentNode) -> int:
        """Calculate maximum query depth."""
        # Traverse AST and find deepest selection set
        return self._traverse_selections(query.definitions[0].selection_set)

    def validate_query(self, query: DocumentNode, max_depth: int) -> None:
        """Validate query doesn't exceed complexity limits."""
        depth = self.calculate_depth(query)
        if depth > max_depth:
            raise QueryTooDeepError(
                f"Query depth {depth} exceeds maximum {max_depth}"
            )
```

### Rate Limiting
```python
# src/fraiseql/security/rate_limiting.py
class RateLimiter:
    """Token bucket rate limiter."""

    async def check_rate_limit(
        self,
        key: str,
        requests: int = 100,
        period: int = 60
    ) -> bool:
        """Check if request is within rate limits."""
        current = await self.redis.incr(f"rl:{key}")
        if current == 1:
            await self.redis.expire(f"rl:{key}", period)

        if current > requests:
            raise RateLimitExceededError(
                f"Rate limit exceeded: {requests} requests per {period}s"
            )

        return True
```

### Security Headers Middleware
```python
# src/fraiseql/security/security_headers.py
class SecurityHeadersMiddleware:
    """Adds security headers to all responses."""

    SECURITY_HEADERS = {
        "X-Content-Type-Options": "nosniff",
        "X-Frame-Options": "DENY",
        "X-XSS-Protection": "1; mode=block",
        "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
        "Content-Security-Policy": "default-src 'self'",
    }

    async def __call__(self, request: Request, call_next):
        response = await call_next(request)
        for header, value in self.SECURITY_HEADERS.items():
            response.headers[header] = value
        return response
```

### Configuration
```python
# src/fraiseql/fastapi/config.py
class FraiseQLConfig(BaseSettings):
    # Security settings
    enable_rate_limiting: bool = True
    rate_limit_requests: int = 100
    rate_limit_period: int = 60
    max_query_depth: int = 10
    enable_query_validation: bool = True
    validation_mode: Literal["strict", "lenient", "disabled"] = "strict"
```
