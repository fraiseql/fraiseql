# Migration Guides

Guides for migrating existing GraphQL APIs to FraiseQL.

## [From Strawberry](./from-strawberry.md)

Complete guide for migrating from Strawberry GraphQL to FraiseQL, including:

- Type definition conversion
- Resolver migration strategies
- Database integration changes
- Authentication migration
- Testing updates
- Deployment considerations

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

Ready to migrate? Start with the [Strawberry migration guide](./from-strawberry.md).
