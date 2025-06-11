# Advanced Topics

Deep dive into FraiseQL's advanced features and optimization techniques.

## [Configuration](./configuration.md)
Comprehensive guide to configuring FraiseQL through code, environment variables, and configuration objects.

## [Authentication](./authentication.md)
Learn about FraiseQL's pluggable authentication system, including built-in Auth0 support and custom authentication providers.

## [Security](./security.md)
Comprehensive security guide covering SQL injection prevention, input validation, authentication, and production hardening.

## [Performance Optimization](./performance.md)
Optimize your FraiseQL API for production with materialized views, connection pooling, query caching, and monitoring.

## [Pagination](./pagination.md)
Implement efficient pagination using cursor-based and offset-based approaches, following the GraphQL Relay specification.

## Database First Architecture
FraiseQL leverages PostgreSQL's power with JSON/JSONB columns and database views for optimal performance and type safety.

## Development Best Practices
Follow established patterns for type definitions, field configurations, and database schema design.

## Production Deployment

### Development vs Production Modes

FraiseQL provides two distinct execution modes optimized for different environments:

#### Development Mode (Default)
- Full GraphQL schema introspection
- Runtime query validation with helpful error messages
- GraphQL Playground/GraphiQL support
- Hot reloading during development

#### Production Mode
- Bypasses GraphQL validation for known queries
- Direct SQL execution with minimal overhead
- No schema introspection for security
- Pre-compiled query cache for maximum performance

```python
# Enable production mode
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://...",
    types=[User, Post],
    production=True  # Optimized for production
)
```

### Environment Variables

Configure FraiseQL using environment variables:

```bash
# Database
DATABASE_URL=postgresql://user:pass@host:5432/db

# Authentication
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_API_IDENTIFIER=your-api

# Performance
FRAISEQL_PRODUCTION=true
FRAISEQL_QUERY_CACHE_SIZE=1000
FRAISEQL_CONNECTION_POOL_SIZE=20
```

### Monitoring and Observability

Monitor your FraiseQL API with logging and metrics:

```python
import logging
from fraiseql.monitoring import setup_monitoring

# Enable detailed logging
logging.getLogger('fraiseql.sql').setLevel(logging.INFO)
logging.getLogger('fraiseql.auth').setLevel(logging.INFO)

# Setup monitoring (optional)
setup_monitoring(app,
    enable_metrics=True,
    enable_tracing=True
)
```

## Security Best Practices

1. **Always use HTTPS** in production
2. **Enable CORS** only for trusted domains
3. **Implement rate limiting** to prevent abuse
4. **Use environment variables** for secrets
5. **Disable introspection** in production mode
6. **Validate all inputs** with proper types
7. **Implement proper authentication** for sensitive operations

## Next Steps

Explore each advanced topic to master FraiseQL and build production-ready GraphQL APIs.
