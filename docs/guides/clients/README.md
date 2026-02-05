# FraiseQL Client Implementation Guides

**Status:** ✅ Production Ready
**Audience:** Frontend developers, mobile developers
**Last Updated:** 2026-02-05

Complete guides for querying FraiseQL servers from different client frameworks and platforms.

---

## Client Frameworks

FraiseQL is a GraphQL backend, so any GraphQL-compatible client library can query it. This section provides language-specific guides and best practices.

### Web Clients

**[React with Apollo Client](./react-apollo-guide.md)** — Web applications using React

- Apollo Client setup and configuration
- Query and mutation patterns with hooks
- Cache management strategies
- Real-time subscriptions with WebSockets
- Error handling and retries
- Performance optimization

**[Vue 3 with Apollo Client](./vue-apollo-guide.md)** — Vue 3 Composition API integration

- Apollo Client for Vue setup
- Composition API query hooks
- Reactive state management
- Subscription handling in Vue components
- TypeScript support
- Server-side rendering (Nuxt)

### Mobile Clients

**[Flutter with GraphQL Client](./flutter-graphql-guide.md)** — Flutter mobile applications

- GraphQL package installation and configuration
- Query and mutation patterns
- Local caching with HiveDB
- Real-time subscriptions
- Error handling for mobile networks
- State management (Provider, Riverpod)
- iOS and Android deployment

**[React Native with Apollo Client](./react-native-apollo-guide.md)** — React Native mobile applications

- Apollo Client setup for React Native
- Query and mutation patterns
- AsyncStorage for persistence
- WebSocket subscriptions
- Android and iOS specific considerations
- Offline support strategies

### Backend & CLI Clients

**[Node.js Runtime Client](./nodejs-runtime-guide.md)** — Backend server-to-server queries

- FraiseQL Node.js client library
- Query execution from backend services
- Batch query processing
- Server-side authentication
- Error handling and retries
- Performance tuning

**[CLI Query Tool](./cli-query-guide.md)** — Command-line querying

- Using `fraiseql-query` CLI tool
- Query files and variables
- Scripting FraiseQL queries
- Integration with automation tools
- Output formatting (JSON, CSV, table)

---

## Common Patterns

### State Management

- **React**: Context API vs Redux vs Zustand + Apollo caching
- **Vue**: Pinia + Apollo Client composables
- **Flutter**: Provider, Riverpod, GetX
- **React Native**: Redux with Apollo Client

### Error Handling

All clients should implement:

- Authentication errors (401)
- Authorization errors (403)
- Validation errors (422)
- Network errors (timeout, offline)
- Server errors (500)
- GraphQL errors in response

### Caching Strategies

- **InMemoryCache** (Apollo) vs server-side caching
- Cache invalidation on mutations
- Partial query caching
- Refetch policies

### Subscriptions

- WebSocket upgrade requirements
- Connection lifecycle management
- Error recovery
- Memory cleanup on unsubscribe

---

## Performance Optimization

### Client-Side

- Query memoization and deduplication
- Lazy loading with `@lazy` directive
- Pagination for large result sets
- Prefetching for anticipated queries
- Connection pooling (backend clients)

### Network

- Persisted queries to reduce payload size
- gzip compression
- HTTP/2 multiplexing (when available)
- Query batching for multiple operations

### Caching

- Cache-first strategies for static data
- Cache-and-network for semi-static data
- Network-only for frequently changing data

---

## Testing Client Code

Each guide includes testing patterns for:

**Unit Tests**

- Query building and validation
- Error handling
- Cache state management

**Integration Tests**

- Mock FraiseQL server responses
- Subscription lifecycle
- Error scenarios

**E2E Tests**

- Full user workflows
- Real backend connectivity
- Performance under load

---

## See Also

**SDK References:**

- **[All 16 Language SDKs](../../integrations/sdk/)** — How to author schemas in your language
- **[SDK Best Practices](../language-sdk-best-practices.md)** — Server-side SDK usage patterns

**Full-Stack Examples:**

- **[Python + React Example](../../examples/fullstack-python-react.md)**
- **[TypeScript + Vue Example](../../examples/fullstack-typescript-vue.md)**
- **[Go + Flutter Example](../../examples/fullstack-go-flutter.md)**
- **[Java + Next.js Example](../../examples/fullstack-java-nextjs.md)**

**Related Guides:**

- **[Real-Time Patterns](../PATTERNS.md)** — Subscription patterns
- **[Authentication & Authorization](../authorization-quick-start.md)** — Securing client access
- **[Production Deployment](../production-deployment.md)** — Hosting considerations

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
