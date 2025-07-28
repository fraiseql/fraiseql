# Migration Guides

Guides for migrating existing GraphQL APIs to FraiseQL.

## [From Graphene](./from-graphene.md)

Complete guide for migrating from Graphene (Python) to FraiseQL, including:

- Database schema migration from ORM to JSONB
- Type definition conversion from class-based to dataclass decorators
- Resolver elimination through view-based architecture
- Authentication migration to built-in decorators
- Performance improvements and N+1 query elimination

## [From Apollo Server](./from-apollo-server.md)

Comprehensive guide for migrating from Apollo Server (Node.js/TypeScript) to FraiseQL, covering:

- Technology stack transition from Node.js to Python
- Schema definition migration from SDL to Python decorators
- DataLoader replacement with automatic optimization
- Database integration changes to PostgreSQL JSONB
- Authentication and error handling migration

## [From Ariadne](./from-ariadne.md)

Step-by-step guide for migrating from Ariadne (Python) to FraiseQL, including:

- Schema-first to code-first migration
- Manual resolver replacement with automatic views
- Error handling improvement with result patterns
- Performance optimization through JSONB architecture
- Testing framework updates

## Benefits of Migration

### Performance Improvements
- **Eliminated N+1 queries** through view-based architecture
- **Production mode** with direct SQL execution
- **Automatic query optimization** with field selection
- **Built-in caching** and connection pooling

### Developer Experience
- **Simpler resolvers** - one view query per resolver
- **Automatic case conversion** from snake_case to camelCase
- **Type safety** throughout the stack
- **Better error handling** with result unions

### Production Features
- **Development authentication** built-in
- **Monitoring and observability** tools
- **Security best practices** by default
- **Scalability** through PostgreSQL optimization

## Migration Strategy

1. **Assessment Phase**
   - Analyze current Strawberry implementation
   - Identify database schema requirements
   - Plan view creation strategy

2. **Database Migration**
   - Create PostgreSQL views for each type
   - Implement JSONB data structure
   - Add appropriate indexes

3. **Code Migration**
   - Convert type definitions
   - Migrate resolvers to view-based queries
   - Update mutations and inputs

4. **Testing and Validation**
   - Ensure GraphQL schema compatibility
   - Validate query performance
   - Test authentication integration

5. **Deployment**
   - Roll out in staging environment
   - Performance testing and optimization
   - Production deployment

Ready to migrate? Choose the guide that matches your current GraphQL implementation:

- **Python developers**: Start with [Graphene](./from-graphene.md) or [Ariadne](./from-ariadne.md)
- **Node.js developers**: Follow the [Apollo Server](./from-apollo-server.md) guide
