# Full-Stack Examples

Complete, production-ready examples demonstrating FraiseQL's end-to-end workflow from schema authoring through deployment and frontend integration.

## Available Examples

### 1. TypeScript Schema + FraiseQL Backend + Vue 3 Frontend

**File**: [`fullstack-typescript-vue.md`](./fullstack-typescript-vue.md)

**Overview**: E-commerce application showcasing the complete modern JavaScript/TypeScript stack.

**What You'll Learn**:

- ✅ Define GraphQL schemas using TypeScript decorators
- ✅ Export schemas to FraiseQL's JSON format
- ✅ Compile schemas with the FraiseQL CLI
- ✅ Deploy FraiseQL server with Docker
- ✅ Build frontend with Vue 3, Composition API, and Apollo Client
- ✅ Implement shopping cart, orders, reviews, and search

**Key Technologies**:

- **Schema Authoring**: TypeScript with decorators and `reflect-metadata`
- **Backend**: Rust (FraiseQL server), PostgreSQL
- **Frontend**: Vue 3, Apollo Client, Vite
- **Database**: PostgreSQL with views and functions
- **Deployment**: Docker and Docker Compose

**Project Structure**:

```text
ecommerce-project/
├── schema-authoring/    # TypeScript schema definition
├── backend/             # Rust FraiseQL server
├── database/            # PostgreSQL schema and seeds
├── frontend/            # Vue 3 application
└── docker-compose.yml   # Complete stack orchestration
```text

**Duration**: 2-3 hours to understand and implement

**Target Audience**: Full-stack developers, TypeScript users, Vue developers

---

### 2. Java Schema + FraiseQL Backend + Next.js Frontend

**File**: [`fullstack-java-nextjs.md`](./fullstack-java-nextjs.md)

**Overview**: Blog platform showcasing Java annotations for schema definition and modern Next.js 14+ Server Components.

**What You'll Learn**:

- ✅ Define GraphQL schemas using Java annotations (@GraphQLType, @GraphQLQuery, etc.)
- ✅ Maven plugin integration for schema export
- ✅ Type-safe relationships and field definitions
- ✅ Building Next.js 14+ with Server Components and Client Components
- ✅ Apollo Client integration with TypeScript code generation
- ✅ Database modeling for blog platforms with full-text search
- ✅ Docker and Kubernetes deployment

**Key Technologies**:

- **Schema Authoring**: Java with Maven, FraiseQL annotations
- **Backend**: Rust (FraiseQL server), PostgreSQL
- **Frontend**: Next.js 14+, React Server Components, Apollo Client, TypeScript
- **Database**: PostgreSQL with views, functions, and full-text search
- **Deployment**: Docker, Docker Compose, Vercel, Kubernetes

**Project Structure**:

```text
blog-monorepo/
├── java-schema/             # Maven project with FraiseQL annotations
├── fraiseql-server/         # Rust FraiseQL server
├── nextjs-frontend/         # Next.js 14 app with Server Components
├── sql/                     # PostgreSQL schema and seed data
└── docker-compose.yml       # Complete stack orchestration
```text

**Duration**: 2-3 hours to understand and implement

**Target Audience**: Full-stack developers, Java developers, Next.js developers

---

### 3. Go Schema + FraiseQL Backend + Flutter Frontend

**File**: [`fullstack-go-flutter.md`](./fullstack-go-flutter.md)

**Overview**: Mobile-first application with API schema defined in Go.

**What You'll Learn**:

- Schema definition in Go with struct tags
- Compiling Go schemas to FraiseQL format
- Building Flutter mobile app with GraphQL client
- iOS/Android deployment

**Key Technologies**:

- **Schema Authoring**: Go with struct tags
- **Backend**: Rust (FraiseQL), PostgreSQL
- **Frontend**: Flutter/Dart
- **Mobile**: iOS and Android

---

## How These Examples Work

### Layered Architecture

Each example follows FraiseQL's core principle: **Separation of Concerns**

```text
Authoring       →    Compilation    →    Runtime      →    Frontend
(TypeScript)   →    (Rust/CLI)      →    (Rust Srv)   →    (Vue/Flutter)

Your code      →    Smart compiler  →    Production   →    Your users
                                         GraphQL API
```text

**Key Point**: The authoring language (TypeScript, Go, Python) is **independent** from the runtime. You use what you're comfortable with for schema definition, then FraiseQL handles the efficient compilation and execution.

### Typical Workflow

1. **Define** your schema using your preferred language
2. **Export** to `schema.json` (language-specific)
3. **Compile** with `fraiseql-cli compile` (generates SQL templates, validates)
4. **Deploy** the compiled schema with FraiseQL server (pure Rust, no FFI)
5. **Consume** via GraphQL from any client (Vue, Flutter, React, etc.)

### Why This Matters

- ✅ **No vendor lock-in**: Export schema → use any GraphQL client
- ✅ **Zero runtime overhead**: Schema compiled to SQL at build time
- ✅ **Type safety everywhere**: From schema definition through frontend components
- ✅ **Framework agnostic**: Use the frontend framework you love
- ✅ **Language agnostic**: Define schemas in TypeScript, Go, Python—doesn't matter

---

## Running the Examples

### Quick Start (TypeScript + Vue Example)

```bash
# 1. Clone the repository
git clone <repo> my-fraiseql-app
cd my-fraiseql-app

# 2. Follow the guide
cat docs/examples/fullstack-typescript-vue.md

# 3. Build and run (Docker Compose handles everything)
docker-compose up

# 4. Open browser
open http://localhost:5173
```text

### Without Docker

Each example includes detailed manual setup instructions if you prefer to run services individually.

---

## Learning Path

### Beginner

1. Read the **Architecture Overview** in your chosen example
2. Review the **TypeScript/Go Schema** section
3. Understand **Database Design** patterns
4. Run the **complete stack** locally

### Intermediate

1. Modify the schema (add a new type or field)
2. Re-export and re-compile
3. See how frontend automatically adapts (via GraphQL introspection)
4. Add new API endpoints

### Advanced

1. Implement custom resolver functions
2. Add authentication and authorization
3. Set up production deployment (Kubernetes, cloud)
4. Optimize query performance
5. Implement caching strategies

---

## Common Questions

### Q: Can I use TypeScript but deploy with Go?

**A**: No. The authoring language is independent, but the runtime is always Rust (FraiseQL server). You choose TypeScript/Go for schema definition, not for runtime.

### Q: How do I add authentication?

**A**: See the "Troubleshooting" section of each example. Authentication is configured in `fraiseql.toml` and flows from compile-time config to runtime enforcement.

### Q: Can I use a different database?

**A**: Yes. FraiseQL supports PostgreSQL (primary), MySQL, SQLite, and SQL Server. Examples use PostgreSQL as it's the most feature-complete.

### Q: How do I call this from React/Svelte/Solid instead of Vue/Flutter?

**A**: Any GraphQL client works. Apollo Client works with all frameworks. Just point your client to the FraiseQL GraphQL endpoint.

### Q: Can I deploy this without Docker?

**A**: Yes. Docker Compose is for convenience. You can run:

- PostgreSQL traditionally or managed cloud (RDS, CloudSQL)
- FraiseQL server as a Rust binary
- Vue/Flutter on any frontend hosting

---

## Best Practices from Examples

1. **Type Your Database**: Use `NOT NULL`, `CHECK`, `UNIQUE` constraints
2. **Use Views for Complex Queries**: FraiseQL compiles views as first-class GraphQL types
3. **Index Aggressively**: Add indexes for commonly filtered fields
4. **Version Your Schema**: Track schema changes in git, use migrations
5. **Test End-to-End**: Examples include integration test patterns
6. **Monitor Production**: Enable logging and metrics (see Deployment section)

---

## What's Next After Examples?

Once you understand the end-to-end flow:

1. **Advanced Schema Design**: See [`../DESIGNING_FOR_FRAISEQL.md`](../DESIGNING_FOR_FRAISEQL.md)
2. **Enterprise Features**: Authentication, authorization, audit logging
3. **Performance Optimization**: Caching, APQ, connection pooling
4. **Operations**: Monitoring, scaling, upgrades
5. **Patterns**: Common solutions for real-world problems

---

## Getting Help

- **Questions about examples?** Check the troubleshooting section
- **Issues running locally?** See Docker logs: `docker-compose logs`
- **GraphQL not working?** Test endpoint: `curl http://localhost:8080/graphql`
- **Frontend can't connect?** Check CORS headers and API URL in frontend config
- **General FraiseQL questions?** See main [documentation](../README.md)

---

## Contributing

Have a great example? Missing a language? Found an issue?

- Report issues on GitHub
- Submit example PRs
- Suggest improvements

This documentation is living—examples evolve with real-world usage.

---

**Remember**: FraiseQL is about writing less code and making it safer. These examples show how.

Happy building! 🚀
