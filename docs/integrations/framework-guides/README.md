# Framework Integration Guides

**Status**: Production-Ready | **Updated**: 2026-02-05 | **Version**: 2.0.0+

Comprehensive guide for integrating FraiseQL GraphQL APIs with popular web frameworks across Python, TypeScript/JavaScript, Go, Java, and Ruby ecosystems.

---

## Overview

FraiseQL compiles GraphQL schemas to optimized SQL at build time. This guide shows how to integrate the compiled schemas with your application framework, handling the GraphQL endpoint, authentication, subscriptions, and error management.

**Integration Pattern**:

1. **Define schema** in Python/TypeScript (authoring only)
2. **Compile schema** with `FraiseQL-cli compile`
3. **Load compiled schema** in your framework
4. **Execute GraphQL** queries via the FraiseQL runtime
5. **Handle results** with framework-specific patterns

**No Runtime FFI**: FraiseQL is compiled to Rust. You communicate via HTTP endpoints or IPC—not language bindings.

---

## Quick Reference Table

| Framework | Language | Level | Primary Use | Deployment | Status |
|-----------|----------|-------|-------------|-----------|--------|
| **FastAPI** | Python | Full-stack | REST API with GraphQL | Docker/Kubernetes | ✅ Stable |
| **Django** | Python | Full-stack | Monolithic web app | Traditional hosting | ✅ Stable |
| **Flask** | Python | Lightweight | Microservices | Docker/Lambda | ✅ Stable |
| **NestJS** | TypeScript | Full-featured | Enterprise apps | Docker/Cloud | ✅ Stable |
| **Express** | JavaScript | Minimalist | Microservices | Node.js hosting | ✅ Stable |
| **Fastify** | JavaScript | High-performance | API backend | Edge/Serverless | ✅ Stable |
| **Gin** | Go | High-performance | API server | Kubernetes/Binary | ✅ Stable |
| **Echo** | Go | Feature-rich | REST API | Docker/Cloud native | ✅ Stable |
| **chi** | Go | Minimal | Lightweight HTTP | Microservices | ✅ Stable |
| **Spring Boot** | Java | Enterprise | Large-scale apps | Enterprise deployment | ✅ Stable |
| **Quarkus** | Java | Cloud-native | Serverless/K8s | Kubernetes/Lambda | ✅ Stable |
| **Rails** | Ruby | Full-stack | Monolithic apps | Traditional hosting | ✅ Stable |
| **Sinatra** | Ruby | Lightweight | Simple APIs | Microservices | ✅ Stable |

---

## Python Frameworks

### FastAPI - Modern API Framework

**Best For**: REST APIs with GraphQL integration, async microservices, OpenAPI documentation

**Key Integration Points**:

- Async/await support for non-blocking GraphQL execution
- Dependency injection for database connections
- Built-in OpenAPI documentation
- WebSocket support for subscriptions
- Middleware for authentication

**Example Setup**:

```python
from fastapi import FastAPI, Depends, HTTPException
from FraiseQL import FraiseQLServer
import asyncio

app = FastAPI()

# Load compiled schema at startup
@app.on_event("startup")
async def startup():
    global fraiseql_server
    fraiseql_server = FraiseQLServer.from_compiled(
        "schema.compiled.json",
        database_url="postgresql://...",
        cache_ttl=300
    )

# GraphQL endpoint
@app.post("/graphql")
async def graphql_query(query: str, variables: dict | None = None):
    try:
        result = await fraiseql_server.execute(
            query=query,
            variables=variables,
            context={"user_id": request.user.id}
        )
        return result
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))

# REST wrapper endpoint
@app.get("/api/users/{user_id}")
async def get_user(user_id: int):
    result = await fraiseql_server.execute("""
        query { user(id: $id) { id name email } }
    """, variables={"id": user_id})
    return result["data"]["user"]

# Subscriptions via WebSocket
@app.websocket("/graphql/ws")
async def websocket_endpoint(websocket):
    await fraiseql_server.handle_subscription(websocket)
```text

**Integration Checklist**:

- ✅ Environment variable configuration
- ✅ Connection pooling setup
- ✅ Error middleware for GraphQL errors
- ✅ Authentication context injection
- ✅ WebSocket upgrade headers
- ✅ Logging and observability
- ✅ CORS configuration
- ✅ Request validation

**Performance Considerations**:

- Connection pooling (min 5, max 20 workers)
- Query result caching with TTL
- Batch request processing
- Early query validation

---

### Django - Full-stack Web Framework

**Best For**: Monolithic applications, admin dashboards, multi-tenant SaaS

**Key Integration Points**:

- Django ORM integration (external views/functions)
- Middleware for authentication context
- Template rendering
- Admin integration
- Permission system compatibility

**Example Setup**:

```python
# settings.py
FRAISEQL = {
    'SCHEMA': 'schema.compiled.json',
    'DATABASE': 'postgresql://...',
    'CACHE_TTL': 300,
    'RATE_LIMIT': {
        'enabled': True,
        'requests': 100,
        'window': 60,
    }
}

# urls.py
from django.urls import path
from fraiseql_django import views

urlpatterns = [
    path('api/graphql/', views.GraphQLView.as_view()),
    path('api/graphql/ws/', views.GraphQLSubscriptionView.as_view()),
]

# views.py
from django.views import View
from FraiseQL import FraiseQLServer
from fraiseql_django.auth import get_user_context

class GraphQLView(View):
    def post(self, request):
        FraiseQL = FraiseQLServer.from_compiled(
            settings.FRAISEQL['SCHEMA']
        )
        context = get_user_context(request)

        result = FraiseQL.execute(
            query=request.json['query'],
            variables=request.json.get('variables'),
            context=context
        )
        return JsonResponse(result)
```text

**Integration Checklist**:

- ✅ Django settings integration
- ✅ User authentication context
- ✅ Permission decorators for queries/mutations
- ✅ Admin site integration
- ✅ Migration handling (schema updates)
- ✅ Logging integration
- ✅ Static file serving
- ✅ CSRF protection

**Performance Considerations**:

- Database connection reuse
- Query result caching via Django cache
- Middleware for early validation
- Async view support (Django 3.1+)

---

### Flask - Lightweight Framework

**Best For**: Microservices, REST APIs, simple integrations, prototyping

**Key Integration Points**:

- Minimal overhead, pure GraphQL endpoint
- Flask blueprints for modular endpoints
- Extension ecosystem
- Lightweight error handling

**Example Setup**:

```python
from flask import Flask, request, jsonify
from FraiseQL import FraiseQLServer

app = Flask(__name__)

# Load schema at startup
fraiseql_server = FraiseQLServer.from_compiled(
    "schema.compiled.json",
    database_url=os.getenv("DATABASE_URL")
)

@app.route('/graphql', methods=['POST'])
def graphql():
    try:
        result = fraiseql_server.execute(
            query=request.json['query'],
            variables=request.json.get('variables')
        )
        return jsonify(result)
    except Exception as e:
        return jsonify({"errors": [{"message": str(e)}]}), 400

@app.route('/graphql/ws')
def graphql_ws():
    # WebSocket handling via Flask-SocketIO
    return fraiseql_server.handle_subscription(request.environ['wsgi.input'])
```text

**Integration Checklist**:

- ✅ Blueprint organization
- ✅ Error handlers
- ✅ Request validation
- ✅ Environment configuration
- ✅ Extension usage
- ✅ CORS headers
- ✅ Rate limiting
- ✅ Logging setup

---

## TypeScript/JavaScript Frameworks

### NestJS - Full-featured Enterprise Framework

**Best For**: Large-scale applications, enterprise backend, microservices architecture

**Key Integration Points**:

- Dependency injection for FraiseQL server
- Module-based organization
- Guards for authentication
- Interceptors for logging/error handling
- Provider pattern for database connections

**Example Setup**:

```typescript
import { Module, Controller, Post, Body } from '@nestjs/common';
import { FraiseQLServer } from 'FraiseQL';

@Module({
  providers: [
    {
      provide: 'FRAISEQL_SERVER',
      useFactory: async () => {
        return FraiseQLServer.fromCompiled('schema.compiled.json', {
          databaseUrl: process.env.DATABASE_URL,
          cacheTtl: 300,
        });
      },
    },
  ],
  exports: ['FRAISEQL_SERVER'],
})
export class FraiseQLModule {}

@Controller('api')
export class GraphQLController {
  constructor(
    @Inject('FRAISEQL_SERVER') private FraiseQL: FraiseQLServer,
  ) {}

  @Post('graphql')
  async executeQuery(
    @Body() { query, variables }: GraphQLRequest,
    @Request() req: any,
  ) {
    const context = { userId: req.user?.id };
    return this.FraiseQL.execute(query, variables, context);
  }
}
```text

**Integration Checklist**:

- ✅ Module imports and exports
- ✅ Service injection
- ✅ Guard decorators for auth
- ✅ Interceptor middleware
- ✅ Exception filters
- ✅ Async initialization
- ✅ Environment configuration
- ✅ OpenAPI documentation

---

### Express - Minimalist Framework

**Best For**: Microservices, REST APIs, lightweight deployments

**Key Integration Points**:

- Middleware stack for request/response handling
- Route handlers
- Error middleware
- Request parsing

**Example Setup**:

```typescript
import express from 'express';
import { FraiseQLServer } from 'FraiseQL';

const app = express();
app.use(express.json());

let FraiseQL: FraiseQLServer;

app.listen(3000, async () => {
  FraiseQL = await FraiseQLServer.fromCompiled('schema.compiled.json', {
    databaseUrl: process.env.DATABASE_URL,
  });
});

app.post('/graphql', async (req, res, next) => {
  try {
    const context = { userId: req.user?.id };
    const result = await FraiseQL.execute(
      req.body.query,
      req.body.variables,
      context,
    );
    res.json(result);
  } catch (error) {
    next(error);
  }
});

// Error middleware
app.use(
  (error: Error, req: express.Request, res: express.Response, next: express.NextFunction) => {
    res.status(400).json({
      errors: [{ message: error.message }],
    });
  },
);
```text

**Integration Checklist**:

- ✅ Middleware ordering
- ✅ Request body parsing
- ✅ Error handling
- ✅ Route organization
- ✅ Authentication context
- ✅ CORS middleware
- ✅ Rate limiting
- ✅ Logging

---

### Fastify - High-Performance Framework

**Best For**: High-throughput APIs, serverless, performance-critical applications

**Key Integration Points**:

- Plugin system for modularity
- Request/reply handlers
- Hooks for lifecycle events
- Built-in request validation (JSON Schema)
- Excellent performance characteristics

**Example Setup**:

```typescript
import Fastify from 'fastify';
import { FraiseQLServer } from 'FraiseQL';

const fastify = Fastify({ logger: true });

let FraiseQL: FraiseQLServer;

fastify.register(async (fastify) => {
  FraiseQL = await FraiseQLServer.fromCompiled('schema.compiled.json', {
    databaseUrl: process.env.DATABASE_URL,
    cacheTtl: 300,
  });

  fastify.post('/graphql', async (request, reply) => {
    const context = { userId: request.user?.id };
    const result = await FraiseQL.execute(
      request.body.query,
      request.body.variables,
      context,
    );
    return result;
  });
});

fastify.setErrorHandler((error, request, reply) => {
  reply.code(400).send({
    errors: [{ message: error.message }],
  });
});

fastify.listen({ port: 3000 });
```text

**Integration Checklist**:

- ✅ Plugin registration
- ✅ Request schema validation
- ✅ Error handling hooks
- ✅ Async initialization
- ✅ Authentication decorators
- ✅ Streaming responses
- ✅ HTTP/2 support
- ✅ Performance tuning

---

## Go Frameworks

### Gin - High-Performance Web Framework

**Best For**: REST APIs, microservices, high-traffic services

**Key Integration Points**:

- Router groups for endpoint organization
- Middleware for cross-cutting concerns
- Context for request-scoped data
- Gin binding for request validation

**Example Setup**:

```go
package main

import (
 "fmt"
 "github.com/gin-gonic/gin"
 "FraiseQL-go"
)

func main() {
 router := gin.Default()

 // Initialize FraiseQL server at startup
 fraiseqlServer, err := FraiseQL.NewServer(
  FraiseQL.Config{
   CompiledSchemaPath: "schema.compiled.json",
   DatabaseURL:       os.Getenv("DATABASE_URL"),
   CacheTTL:          300,
  },
 )
 if err != nil {
  panic(err)
 }

 // GraphQL endpoint
 router.POST("/api/graphql", func(c *gin.Context) {
  var query struct {
   Query     string                 `json:"query"`
   Variables map[string]interface{} `json:"variables"`
  }

  if err := c.ShouldBindJSON(&query); err != nil {
   c.JSON(400, gin.H{"error": err.Error()})
   return
  }

  ctx := c.Request.Context()
  result, err := fraiseqlServer.Execute(ctx, FraiseQL.ExecuteRequest{
   Query:     query.Query,
   Variables: query.Variables,
   Context: map[string]interface{}{
    "userID": c.GetString("user_id"),
   },
  })

  if err != nil {
   c.JSON(400, gin.H{"errors": []string{err.Error()}})
   return
  }

  c.JSON(200, result)
 })

 router.Run(":8080")
}
```text

**Integration Checklist**:

- ✅ Router group organization
- ✅ Middleware chain
- ✅ Context propagation
- ✅ Error handling
- ✅ Request binding
- ✅ Database connection pooling
- ✅ Logging integration
- ✅ Metrics collection

---

### Echo - Full-featured Framework

**Best For**: REST APIs, full-featured services, enterprise applications

**Key Integration Points**:

- Route groups for modular endpoints
- Middleware ecosystem
- Custom bind/renderer for GraphQL
- Built-in validation

**Example Setup**:

```go
package main

import (
 "github.com/labstack/echo/v4"
 "github.com/labstack/echo/v4/middleware"
 "FraiseQL-go"
)

func main() {
 e := echo.New()

 // Initialize FraiseQL
 fraiseqlServer := FraiseQL.MustNewServer(FraiseQL.Config{
  CompiledSchemaPath: "schema.compiled.json",
  DatabaseURL:       os.Getenv("DATABASE_URL"),
 })

 // CORS middleware
 e.Use(middleware.CORS())

 // GraphQL routes
 api := e.Group("/api")
 {
  api.POST("/graphql", func(c echo.Context) error {
   var req FraiseQL.GraphQLRequest
   if err := c.Bind(&req); err != nil {
    return c.JSON(400, map[string]interface{}{
     "error": err.Error(),
    })
   }

   result, err := fraiseqlServer.Execute(c.Request().Context(), req)
   if err != nil {
    return c.JSON(400, result)
   }

   return c.JSON(200, result)
  })

  api.WebSocket("/graphql/ws", func(c echo.Context) error {
   return fraiseqlServer.HandleSubscription(c.Request().Context(), c.Response())
  })
 }

 e.Logger.Fatal(e.Start(":8080"))
}
```text

**Integration Checklist**:

- ✅ Route group management
- ✅ Middleware chaining
- ✅ WebSocket support
- ✅ Custom validators
- ✅ Error handler setup
- ✅ Graceful shutdown
- ✅ Request context
- ✅ Health checks

---

### chi - Lightweight Router

**Best For**: Microservices, minimal overhead, composable middleware

**Key Integration Points**:

- Router groups for modular routes
- Lightweight middleware
- Standard library http patterns
- Context-based request data

**Example Setup**:

```go
package main

import (
 "net/http"
 "github.com/go-chi/chi/v5"
 "FraiseQL-go"
)

func main() {
 fraiseqlServer := FraiseQL.MustNewServer(FraiseQL.Config{
  CompiledSchemaPath: "schema.compiled.json",
  DatabaseURL:       os.Getenv("DATABASE_URL"),
 })

 r := chi.NewRouter()

 r.Post("/graphql", handleGraphQL(fraiseqlServer))
 r.HandleFunc("/graphql/ws", handleSubscription(fraiseqlServer))

 http.ListenAndServe(":8080", r)
}

func handleGraphQL(server *FraiseQL.Server) http.HandlerFunc {
 return func(w http.ResponseWriter, r *http.Request) {
  var req FraiseQL.GraphQLRequest
  json.NewDecoder(r.Body).Decode(&req)

  result, _ := server.Execute(r.Context(), req)
  w.Header().Set("Content-Type", "application/json")
  json.NewEncoder(w).Encode(result)
 }
}
```text

**Integration Checklist**:

- ✅ Composable middleware
- ✅ Router groups
- ✅ Context values
- ✅ HTTP method handlers
- ✅ Error responses
- ✅ Request validation
- ✅ Logging hooks
- ✅ Graceful shutdown

---

## Java Frameworks

### Spring Boot - Enterprise Framework

**Best For**: Large-scale applications, enterprise environments, microservices with orchestration

**Key Integration Points**:

- Spring beans for dependency injection
- MVC controllers
- Exception handlers
- Interceptors
- Security integration

**Example Setup**:

```java
@SpringBootApplication
public class FraiseQLApplication {
    public static void main(String[] args) {
        SpringApplication.run(FraiseQLApplication.class, args);
    }
}

@Configuration
public class FraiseQLConfig {
    @Bean
    public FraiseQLServer fraiseqlServer() throws Exception {
        return FraiseQLServer.fromCompiled(
            "schema.compiled.json",
            new FraiseQLConfig.Builder()
                .databaseUrl(System.getenv("DATABASE_URL"))
                .cacheTtl(300)
                .build()
        );
    }
}

@RestController
@RequestMapping("/api")
public class GraphQLController {
    @Autowired
    private FraiseQLServer fraiseqlServer;

    @PostMapping("/graphql")
    public ResponseEntity<?> executeQuery(
        @RequestBody GraphQLRequest request,
        @AuthenticationPrincipal UserDetails user
    ) {
        try {
            Map<String, Object> context = new HashMap<>();
            context.put("userId", user.getUsername());

            QueryResult result = fraiseqlServer.execute(
                request.getQuery(),
                request.getVariables(),
                context
            );
            return ResponseEntity.ok(result);
        } catch (Exception e) {
            return ResponseEntity.badRequest().body(
                new ErrorResponse(e.getMessage())
            );
        }
    }

    @ExceptionHandler(GraphQLException.class)
    public ResponseEntity<?> handleGraphQLException(GraphQLException ex) {
        return ResponseEntity.badRequest().body(
            Map.of("errors", List.of(ex.getMessage()))
        );
    }
}
```text

**Integration Checklist**:

- ✅ Spring beans
- ✅ Auto-configuration
- ✅ Security integration
- ✅ MVC controllers
- ✅ Exception handling
- ✅ Interceptors
- ✅ Environment properties
- ✅ Actuator endpoints

---

### Quarkus - Cloud-native Framework

**Best For**: Serverless, Kubernetes, cloud-native deployments, instant startup

**Key Integration Points**:

- CDI for dependency injection
- RESTEasy for HTTP
- Config for externalized configuration
- Native image compilation

**Example Setup**:

```java
@ApplicationScoped
public class FraiseQLProducer {
    @Inject
    Config config;

    @Produces
    @Singleton
    public FraiseQLServer fraiseqlServer() throws Exception {
        return FraiseQLServer.fromCompiled(
            "schema.compiled.json",
            new FraiseQLConfig.Builder()
                .databaseUrl(config.getValue("FraiseQL.database.url", String.class))
                .cacheTtl(300)
                .build()
        );
    }
}

@Path("/api")
public class GraphQLResource {
    @Inject
    FraiseQLServer fraiseqlServer;

    @POST
    @Path("/graphql")
    @Produces(MediaType.APPLICATION_JSON)
    @Consumes(MediaType.APPLICATION_JSON)
    public QueryResult graphql(GraphQLRequest request) {
        return fraiseqlServer.execute(
            request.query,
            request.variables,
            Map.of()
        );
    }
}
```text

**Integration Checklist**:

- ✅ CDI beans
- ✅ Configuration management
- ✅ Native compilation
- ✅ Fast startup
- ✅ Slim JAR size
- ✅ Kubernetes-ready
- ✅ Health checks
- ✅ Metrics export

---

## Ruby Frameworks

### Rails - Full-stack Web Framework

**Best For**: Monolithic applications, rapid development, admin dashboards

**Key Integration Points**:

- Rails controllers
- Middleware stack
- ActiveRecord integration (views/functions only)
- User authentication
- Admin interface

**Example Setup**:

```ruby
# config/routes.rb
Rails.application.routes.draw do
  namespace :api do
    post '/graphql' => 'graphql#execute'
    websocket '/graphql/ws' => 'graphql#subscribe'
  end
end

# app/controllers/api/graphql_controller.rb
module Api
  class GraphqlController < ApplicationController
    def execute
      fraiseql_server = FraiseQL::Server.from_compiled(
        'schema.compiled.json',
        database_url: ENV['DATABASE_URL']
      )

      result = fraiseql_server.execute(
        query: params[:query],
        variables: params[:variables],
        context: { user_id: current_user&.id }
      )

      render json: result
    rescue StandardError => e
      render json: { errors: [{ message: e.message }] }, status: :bad_request
    end

    def subscribe
      fraiseql_server = FraiseQL::Server.from_compiled('schema.compiled.json')
      fraiseql_server.handle_subscription(request)
    end
  end
end

# config/application.rb
config.middleware.use Rack::CORSMiddleware
```text

**Integration Checklist**:

- ✅ Controller organization
- ✅ Middleware stack
- ✅ Devise integration
- ✅ Admin gems
- ✅ Database migrations
- ✅ Error handling
- ✅ Logging setup
- ✅ Asset pipeline (for frontend)

---

### Sinatra - Lightweight DSL

**Best For**: Simple APIs, microservices, quick prototypes

**Key Integration Points**:

- Route handlers
- Middleware
- Error handling
- Simple configuration

**Example Setup**:

```ruby
require 'sinatra'
require 'FraiseQL'
require 'json'

fraiseql_server = FraiseQL::Server.from_compiled(
  'schema.compiled.json',
  database_url: ENV['DATABASE_URL']
)

post '/graphql' do
  content_type :json

  request.body.rewind
  data = JSON.parse(request.body.read)

  result = fraiseql_server.execute(
    query: data['query'],
    variables: data['variables']
  )

  result.to_json
rescue StandardError => e
  status 400
  { errors: [{ message: e.message }] }.to_json
end

get '/health' do
  content_type :json
  { status: 'ok' }.to_json
end
```text

**Integration Checklist**:

- ✅ Route definition
- ✅ Request parsing
- ✅ Error handlers
- ✅ CORS headers
- ✅ Logging
- ✅ Environment config
- ✅ Database connection
- ✅ Rate limiting

---

## Integration Patterns

### 1. GraphQL Endpoint Setup

All frameworks share a common pattern for the core GraphQL endpoint:

```text
POST /graphql
Content-Type: application/json

{
  "query": "query { user(id: 1) { id name email } }",
  "variables": { "id": 1 }
}

Response:
{
  "data": { "user": { "id": 1, "name": "Alice", "email": "alice@example.com" } }
}
```text

**Best Practices**:

- Parse query and variables from request body
- Extract user context from authentication headers/session
- Return errors in GraphQL format (not HTTP error codes)
- Support both GET and POST (GET for introspection, POST for queries)
- Implement request size limits
- Cache compiled schema at server startup

### 2. REST API Wrapper

Wrap specific GraphQL operations as REST endpoints:

```text
GET /api/users/123

Internally executes:
query { user(id: 123) { id name email } }

Response:
{ "id": 123, "name": "Alice", "email": "alice@example.com" }
```text

**Adapter Pattern**:

```python
class RestAdapter:
    def get_user(self, user_id):
        query = f'query {{ user(id: {user_id}) {{ id name email }} }}'
        result = self.FraiseQL.execute(query)
        return result['data']['user']
```text

### 3. WebSocket Subscriptions

Set up real-time subscriptions:

```javascript
// Client
const ws = new WebSocket('ws://localhost:3000/graphql/ws');
ws.send(JSON.stringify({
  type: 'START',
  payload: {
    query: 'subscription { onUserCreated { id name email } }'
  }
}));

ws.on('message', (event) => {
  const message = JSON.parse(event.data);
  if (message.type === 'DATA') {
    console.log('User created:', message.payload.data);
  }
});
```text

**Server Implementation**:

- Accept WebSocket upgrade
- Parse GraphQL subscription from connection message
- Stream results as JSON objects
- Handle client disconnection
- Implement heartbeat/keepalive

### 4. Error Handling Integration

Standardize error responses across all frameworks:

```json
{
  "errors": [
    {
      "message": "User not found",
      "extensions": {
        "code": "NOT_FOUND",
        "path": ["user"]
      }
    }
  ]
}
```text

**Common Error Types**:

- `VALIDATION_ERROR` - Invalid query/variables
- `AUTHENTICATION_ERROR` - Not authenticated
- `AUTHORIZATION_ERROR` - Not authorized (RBAC)
- `NOT_FOUND` - Resource doesn't exist
- `INTERNAL_ERROR` - Server error
- `DATABASE_ERROR` - Database connectivity/query error
- `RATE_LIMIT_ERROR` - Rate limit exceeded

---

## Framework-Specific Considerations

### Type Safety Per Framework

**Python**:

- Use type hints for query results
- Validate with Pydantic models
- Enable mypy/pyright checking

**TypeScript/JavaScript**:

- Generate TypeScript types from schema
- Use `graphql-codegen` for type generation
- Enable strict mode in tsconfig.json

**Go**:

- Use struct tags for JSON marshaling
- Generate types with `gqlgen` or `gomodule-gqlgen`
- Compile-time type checking

**Java**:

- Generate POJOs from schema
- Use Jackson for JSON mapping
- Enable compile-time verification

**Ruby**:

- Use ActiveSupport for type coercion
- Document with YARD
- Test with RSpec

### Async Patterns

**Python (async/await)**:

```python
async def graphql(query, variables):
    result = await fraiseql_server.execute(query, variables)
    return result
```text

**TypeScript (Promises)**:

```typescript
async executeQuery(query: string): Promise<Result> {
  const result = await FraiseQL.execute(query);
  return result;
}
```text

**Go (goroutines)**:

```go
go fraiseqlServer.Execute(ctx, request)
```text

### Testing Setup

**Python (pytest)**:

```python
@pytest.fixture
def fraiseql_server():
    return FraiseQLServer.from_compiled('schema.compiled.json')

def test_query(fraiseql_server):
    result = fraiseql_server.execute('query { user(id: 1) { id } }')
    assert result['data'] is not None
```text

**TypeScript (Jest)**:

```typescript
describe('GraphQL', () => {
  it('should execute query', async () => {
    const result = await FraiseQL.execute('query { user(id: 1) { id } }');
    expect(result.data).toBeDefined();
  });
});
```text

### Deployment Considerations

**Development**:

- Hot reload compiled schema changes
- Enable query logging for debugging
- Use local SQLite for testing

**Production**:

- Pre-compile schema at build time
- Load from immutable artifact
- Environment variable overrides
- Connection pooling (min 10, max 50)
- Query result caching
- Rate limiting enabled
- Error sanitization enabled
- Audit logging enabled

---

## Getting Started

### Quick Start Checklist

1. **Define Schema** (in Python/TypeScript)
   - Create `schema.py` or `schema.ts`
   - Use FraiseQL decorators for types, queries, mutations
   - Export to `schema.json`

2. **Compile Schema**

   ```bash
   FraiseQL-cli compile schema.json FraiseQL.toml
   ```text

3. **Setup Framework**
   - Choose framework from above
   - Initialize FraiseQL server at startup
   - Create GraphQL endpoint

4. **Test Integration**
   - POST to `/graphql` endpoint
   - Verify results
   - Check error handling

5. **Deploy**
   - Bundle compiled schema with application
   - Configure environment variables
   - Enable observability
   - Monitor performance

### Common Setup Patterns

**Development Workflow**:

```bash
# Watch for schema changes
watchmedo shell-command \
  --patterns="*.py" \
  --recursive \
  --command='python schema.py && FraiseQL-cli compile schema.json' \
  .

# Start server in another terminal
python main.py
```text

**Production Deployment**:

```dockerfile
FROM python:3.12
COPY schema.compiled.json /app/
COPY FraiseQL.toml /app/
COPY app.py /app/
CMD ["python", "/app/app.py"]
```text

### Performance Tips

- **Connection pooling**: min 5, max 20 (adjust based on load)
- **Query caching**: TTL 300s for stable queries
- **Batch operations**: Use mutations with list parameters
- **Index denormalized columns**: For fast filtering in analytics
- **Enable compression**: gzip for large responses
- **Use subscriptions**: Only for real-time updates (not polling)

---

## See Also

- **SDK References**: [Python](../sdk/python-reference.md) | [TypeScript](../sdk/typescript-reference.md) | [Go](../sdk/go-reference.md) | [Java](../sdk/java-reference.md) | [Ruby](../sdk/ruby-reference.md)
- **Architecture**: [FraiseQL Architecture Principles](../../architecture/README.md)
- **Authentication**: [Auth Integration Guide](../authentication/README.md)
- **Deployment**: [Deployment Guide](../../guides/production-deployment.md)
- **API Federation**: [Federation Guide](../federation/README.md)
- **Performance**: [Performance Optimization](../../guides/performance-optimization.md)

---

## Community Resources

- **Issues**: [GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues)
- **Discussions**: [GitHub Discussions](https://github.com/FraiseQL/FraiseQL/discussions)
- **Stack Overflow**: Tag questions with `FraiseQL`
- **Community Chat**: [Discord](https://discord.gg/FraiseQL)

---

**Status**: ✅ Production Ready
**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**License**: MIT
