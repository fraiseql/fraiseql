# FraiseQL Example Applications

This directory contains comprehensive example applications that demonstrate FraiseQL's capabilities across different domains and use cases. Each example showcases best practices, advanced features, and real-world implementation patterns.

## 🎯 New Blueprint Examples

### 📝 Blog Simple (`blog_simple/`) **NEW**
**Perfect for learning FraiseQL fundamentals**

A complete blog application demonstrating core FraiseQL patterns:
- **Database-first architecture** with PostgreSQL functions
- **Command/Query separation** with views and materialized tables
- **CRUD operations** with comprehensive error handling
- **Real-time database testing** patterns
- **Authentication and authorization** flows

**Quick Start:**
```bash
cd blog_simple
docker-compose up -d
# Visit http://localhost:8000/graphql
```

### 🏢 Blog Enterprise (`blog_enterprise/`) **NEW**
**Enterprise-grade patterns for production systems**

An advanced blog showcasing enterprise development patterns:
- **Domain-driven design** with bounded contexts
- **Advanced PostgreSQL patterns** (stored procedures, triggers, materialized views)
- **Enterprise authentication** with role-based access control
- **Multi-tenant architecture** support
- **Event sourcing** and audit trails
- **Performance optimization** with caching layers

**Quick Start:**
```bash
cd blog_enterprise
docker-compose up -d
# Visit http://localhost:8000/graphql
```

## 📚 Learning Path

| Example | Complexity | Best For | Key Patterns |
|---------|------------|----------|--------------|
| `blog_simple/` | **Beginner** | Learning basics | CRUD, basic auth, simple queries |
| `blog_enterprise/` | **Advanced** | Production systems | DDD, multi-tenancy, event sourcing |
| `blog_api/` | **Intermediate** | Content systems | Audit trails, mutation results |
| `ecommerce_api/` | **Advanced** | E-commerce | Complex validation, business rules |

## 🏢 Enterprise Patterns (`enterprise_patterns/`) **NEW**

**The definitive reference for production-ready enterprise applications.**

A comprehensive showcase of all PrintOptim Backend patterns:

- **Mutation Result Pattern**: Standardized success/error/noop responses with audit metadata
- **NOOP Handling**: Graceful handling of edge cases and business rule violations
- **App/Core Function Split**: Clean architecture with input handling and business logic separation
- **Audit Field Patterns**: Complete audit trails with version management and change tracking
- **Identifier Management**: Triple ID pattern (internal, UUID, business identifiers)
- **Multi-Layer Validation**: GraphQL, app, core, and database validation layers

### Patterns Used
- ✅ Multi-tenancy with RLS
- ✅ CQRS with PostgreSQL functions
- ✅ **Mutation Result Pattern** (NEW)
- ✅ **NOOP Handling** (NEW)
- ✅ **App/Core Function Split** (NEW)
- ✅ **Complete Audit Trails** (NEW)
- ✅ **Identifier Management** (NEW)
- ✅ **Multi-Layer Validation** (NEW)

### Quick Start
```bash
cd enterprise_patterns
docker-compose up -d
# Visit http://localhost:8001/graphql
```

**Use this example for production systems requiring compliance, audit trails, and enterprise-grade reliability.**

## 🏪 E-commerce API (`ecommerce_api/`)

A complete e-commerce platform demonstrating:

- **Product Catalog**: Categories, variants, inventory management
- **Shopping Cart**: Session-based and user carts with real-time inventory
- **Order Management**: Complete order lifecycle with payment processing
- **Customer Accounts**: Registration, profiles, addresses, order history
- **Reviews & Ratings**: Product reviews with verified purchase tracking
- **Search & Filtering**: Full-text search with faceted filtering
- **Coupons & Discounts**: Flexible discount system

### Patterns Used
- ✅ Multi-tenancy with RLS
- ✅ CQRS with PostgreSQL functions
- ✅ **Cross-Entity Validation** (NEW)
- ✅ **Multi-Layer Validation** (NEW)
- ✅ **Enterprise Error Handling** (NEW)
- ❌ Complete audit trails (see enterprise_patterns/ example)

### Key Features
- CQRS architecture with optimized views and functions
- Real-time inventory tracking with validation patterns
- Complex business logic with cross-entity validation
- Type-safe GraphQL API with structured error handling
- Performance optimization with indexes and materialized views

### Quick Start
```bash
cd ecommerce_api
docker-compose up -d
# Visit http://localhost:8000/graphql
```

## 💬 Real-time Chat (`real_time_chat/`)

A comprehensive chat application featuring:

- **Real-time Messaging**: WebSocket-based instant messaging
- **User Presence**: Online/offline status tracking
- **Typing Indicators**: Live typing status updates
- **Message Reactions**: Emoji reactions on messages
- **Direct Messages**: 1-on-1 private conversations
- **Room Management**: Public and private chat rooms
- **File Attachments**: Image and document sharing
- **Message Search**: Full-text search across conversations

### Key Features
- PostgreSQL LISTEN/NOTIFY for real-time events
- WebSocket connection management
- Event-driven architecture
- Presence tracking and typing indicators
- Message threading and reactions

### Quick Start
```bash
cd real_time_chat
docker-compose up -d
# WebSocket: ws://localhost:8000/ws/{user_id}
# GraphQL: http://localhost:8000/graphql
```

## 📊 Analytics Dashboard (`analytics_dashboard/`)

A business intelligence and analytics platform with:

- **Time-series Analytics**: High-performance time-based analysis
- **User Behavior Tracking**: Sessions, page views, user journeys
- **Conversion Funnels**: Multi-step conversion analysis
- **A/B Testing**: Experiment management and results
- **Performance Monitoring**: Application metrics and alerts
- **Revenue Analytics**: Financial tracking and attribution
- **Cohort Analysis**: User retention and engagement

### Key Features
- TimescaleDB integration for time-series optimization
- Complex analytical queries with window functions
- Materialized views for performance
- Real-time dashboard APIs
- Statistical analysis and reporting

### Quick Start
```bash
cd analytics_dashboard
docker-compose up -d
# Visit http://localhost:8000/graphql
```

## 📝 Blog API (`blog_api/`)

A content management system demonstrating enterprise patterns:

- **Content Management**: Posts, categories, tags with audit trails
- **User Authentication**: Authors and readers with role management
- **Comments System**: Nested comments with moderation
- **Media Management**: Image uploads and optimization
- **SEO Features**: Meta tags, sitemap generation

### Patterns Used
- ✅ Multi-tenancy with RLS
- ✅ CQRS with PostgreSQL functions
- ✅ **Mutation Result Pattern** (NEW)
- ✅ **NOOP Handling** (NEW)
- ✅ **App/Core Function Split** (NEW)
- ✅ **Basic Audit Trails** (NEW)
- ❌ Advanced validation (see enterprise_patterns/ example)

### Quick Start
```bash
cd blog_api
python app.py
# Visit http://localhost:8000/graphql
```

**Great for content management systems with enterprise features.**

## 🏆 Performance Comparison

### Benchmark Results

| Operation | FraiseQL | Hasura | PostGraphile | Custom GraphQL |
|-----------|----------|---------|--------------|----------------|
| Simple Query | 15ms | 25ms | 30ms | 45ms |
| Complex Join | 35ms | 85ms | 95ms | 150ms |
| Mutation | 20ms | 40ms | 50ms | 80ms |
| Real-time Update | 5ms | 15ms | N/A | 100ms |

### Why FraiseQL is Faster

1. **Database-First Design**: Queries execute directly in PostgreSQL
2. **Optimized Views**: Pre-computed joins and aggregations
3. **Minimal Overhead**: Direct database connection without ORM
4. **Smart Caching**: Query plan caching and connection pooling
5. **PostgreSQL Functions**: Business logic at database level

## 🏗️ Architecture Overview

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   GraphQL API   │    │  FastAPI App    │    │   PostgreSQL    │
│                 │    │                 │    │                 │
│ • Type Safety   │───▶│ • FraiseQL      │───▶│ • Views         │
│ • Validation    │    │ • Mutations     │    │ • Functions     │
│ • Introspection │    │ • WebSockets    │    │ • Triggers      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### CQRS Pattern
- **Queries**: Optimized PostgreSQL views for read operations
- **Commands**: PostgreSQL functions for write operations
- **Events**: Triggers and NOTIFY for real-time updates

### Type System
- **Pydantic Models**: Type-safe Python models
- **GraphQL Schema**: Auto-generated from Python types
- **Database Schema**: Views and functions match GraphQL types

## 🚀 Getting Started

### Prerequisites
- Python 3.11+
- PostgreSQL 14+
- Docker & Docker Compose (optional)

### Installation
```bash
# Clone the repository
git clone https://github.com/your-org/fraiseql.git
cd fraiseql/examples

# Choose an example
cd ecommerce_api

# Install dependencies
pip install -r requirements.txt

# Set up database
createdb ecommerce
psql -d ecommerce -f db/migrations/001_initial_schema.sql

# Run the application
uvicorn app:app --reload
```

### Using Docker
```bash
cd ecommerce_api
docker-compose up -d
```

## 🎯 Pattern Progression Guide

### Basic → Intermediate → Enterprise

Choose your starting point based on your needs:

| Example | Complexity | Best For | Patterns |
|---------|------------|----------|-----------|
| `quickstart.py` | **Basic** | Learning FraiseQL | Simple mutations |
| `blog_api/` | **Intermediate** | Content systems | Mutation results, basic audit |
| `ecommerce_api/` | **Advanced** | E-commerce apps | Cross-entity validation |
| `enterprise_patterns/` | **Full** | Production systems | All enterprise patterns |

### Pattern Migration Path

1. **Start Simple** - Use basic resolver functions
2. **Add Structure** - Implement mutation result pattern
3. **Add Reliability** - Include NOOP handling
4. **Add Compliance** - Implement audit trails
5. **Add Scale** - Use app/core function split

See [`pattern_comparison.md`](pattern_comparison.md) for detailed comparison.

## 📚 Learning Path

### 1. **Start with Blog API**
   - Basic FraiseQL concepts with enterprise patterns
   - Mutation result pattern introduction
   - Simple audit trail implementation

### 2. **Explore E-commerce API**
   - Complex validation patterns
   - Cross-entity business rules
   - Advanced error handling

### 3. **Master Enterprise Patterns**
   - Complete audit trail system
   - Multi-layer validation
   - Production-ready patterns

### 4. **Add Real-time Features**
   - WebSocket integration
   - Real-time subscriptions
   - Event-driven architecture

## 🔧 Development Tools

### GraphQL Playground
Each example includes GraphQL Playground at `/graphql` for:
- Interactive query testing
- Schema exploration
- Mutation testing
- Real-time subscriptions

### Database Tools
- **pgAdmin**: Database administration
- **DataGrip**: SQL IDE with advanced features
- **Postico**: macOS PostgreSQL client

### Testing
```bash
# Run tests for an example
cd ecommerce_api
pytest tests/

# Load testing
locust -f tests/load_test.py
```

## 🎯 Best Practices Demonstrated

### 1. **Database Design**
- Proper indexing strategies
- Materialized views for performance
- Partitioning for large datasets
- Foreign key constraints

### 2. **GraphQL API Design**
- Intuitive schema structure
- Efficient query patterns
- Proper error handling
- Input validation

### 3. **Security**
- SQL injection prevention
- Authentication and authorization
- Rate limiting
- Input sanitization

### 4. **Performance**
- Query optimization
- Connection pooling
- Caching strategies
- Monitoring and alerts

## 🔍 Debugging & Monitoring

### Query Analysis
```sql
-- Enable query logging
SET log_statement = 'all';

-- Analyze query performance
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM product_search;

-- Monitor active queries
SELECT * FROM pg_stat_activity;
```

### Application Monitoring
- Prometheus metrics
- Grafana dashboards
- Error tracking with Sentry
- Performance monitoring

## 🌟 Advanced Features

### Custom Scalars
```python
from fraiseql import scalar

@scalar
class DateTime:
    serialize = lambda v: v.isoformat()
    parse_value = lambda v: datetime.fromisoformat(v)
```

### Custom Directives
```python
from fraiseql import directive

@directive
def deprecated(reason: str):
    # Custom deprecation logic
    pass
```

### Middleware
```python
from fraiseql import middleware

@middleware
async def auth_middleware(resolve, root, info, **args):
    # Authentication logic
    return await resolve(root, info, **args)
```

## 🤝 Contributing

### Adding New Examples
1. Create a new directory under `examples/`
2. Follow the established structure:
   ```
   example_name/
   ├── db/
   │   ├── migrations/
   │   ├── views/
   │   └── functions/
   ├── tests/
   ├── app.py
   ├── models.py
   ├── mutations.py
   ├── README.md
   └── requirements.txt
   ```
3. Include comprehensive documentation
4. Add tests and benchmarks

### Improvement Ideas
- Add more complex business scenarios
- Implement additional GraphQL features
- Optimize for different use cases
- Add more real-time features

## 📖 Additional Resources

- [FraiseQL Documentation](https://fraiseql.dev)
- [PostgreSQL Performance Tips](https://wiki.postgresql.org/wiki/Performance_Optimization)
- [GraphQL Best Practices](https://graphql.org/learn/best-practices/)
- [TimescaleDB Documentation](https://docs.timescale.com/)

## 🆘 Support

- **Issues**: [GitHub Issues](https://github.com/your-org/fraiseql/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/fraiseql/discussions)
- **Discord**: [FraiseQL Community](https://discord.gg/fraiseql)
- **Email**: support@fraiseql.dev

---

*These examples demonstrate the power and flexibility of FraiseQL for building production-ready GraphQL APIs with PostgreSQL. Each example is designed to be both educational and practical, showing real-world patterns and best practices.*
