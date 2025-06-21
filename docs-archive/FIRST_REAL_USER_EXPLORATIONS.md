# First Real User Explorations of FraiseQL

This document captures the misunderstandings, confusion points, and challenges encountered during the first real-world attempt to migrate a production application from Strawberry GraphQL to FraiseQL.

## Context

- **User**: Migrating PrintOptim backend from Strawberry GraphQL to FraiseQL
- **Date**: December 2024
- **Starting Point**: Existing Strawberry GraphQL API with complex types, queries, and mutations
- **Goal**: Complete migration to FraiseQL while maintaining functionality

## Major Misunderstandings and Pain Points

### 1. Query Registration Pattern Confusion

**What I Expected**: Based on the blog example, I expected to pass query functions directly:
```python
async def get_user(info, id: UUID) -> User:
    return User(...)

app = create_fraiseql_app(
    queries=[get_user, get_posts],  # Just functions
)
```

**What Actually Happened**:
- The schema builder threw "Type Query must define one or more fields"
- The `build_fraiseql_schema` function calls `register_type()` on query functions
- This suggests queries should be types, not functions

**Attempted Solutions**:
1. Created a `QueryRoot` type with `@fraiseql.field` decorators (failed - no such decorator)
2. Tried passing empty queries list (failed - same error)
3. Tried using types in queries parameter (still unclear what the correct pattern is)

**Confusion**: The blog example clearly shows passing functions, but the code expects types. This fundamental mismatch made it impossible to get even a basic schema working.

### 2. Context and Database Access Pattern

**Initial Assumption**: I thought I needed to provide a `context_getter` function like in Strawberry:
```python
async def get_context(request: Request) -> dict:
    return {
        "db": connection,
        "user": current_user,
    }
```

**Reality**:
- `create_fraiseql_app()` doesn't accept a `context_getter` parameter
- The CQRS pattern seems to be injected differently
- Blog example shows dependency injection but it's not clear how it connects

### 3. Authentication Integration

**Challenge**: The Auth0 integration in the existing code caused hanging because:
- Module-level Auth0 initialization tries to fetch JWKS immediately
- This blocks the import process
- Had to create a lazy loading pattern to work around it

**What Would Help**: Clear documentation on how to integrate existing auth systems without using FraiseQL's built-in auth.

### 4. Type System Limitations

**Unexpected Restrictions**:
- No support for `dict[str, Any]` or JSON types in GraphQL schema
- Had to remove `details` field from Error type
- Unclear what the alternative is for flexible JSON data

**Missing Guidance**: How to handle dynamic/flexible data structures that were previously JSON fields.

### 5. Environment Variable Conflicts

**Problem**: FraiseQL's `FraiseQLConfig` uses pydantic settings which auto-loads from environment:
```
pydantic_core._pydantic_core.ValidationError: 5 validation errors for FraiseQLConfig
env
  Extra inputs are not permitted [type=extra_forbidden, input_value='local', input_type=str]
```

**Issue**: Common env var names like `ENV`, `DEBUG`, `DB_USER` conflict with FraiseQL's config.

**Workaround Needed**: Had to rename .env file to prevent auto-loading.

### 6. Decorator Confusion

**What I Tried**:
- `@fraiseql.query` - Doesn't exist (though seemed logical)
- `@fraiseql.field` - Doesn't exist (tried for QueryRoot)
- `@fraiseql.type` - Works
- `@fraiseql.input` - Works
- `@fraiseql.mutation` - Exists but wasn't working as expected
- `@fraiseql.success`/`@fraiseql.failure` - Work for result types

**Confusion**: Inconsistent decorator API - why do types and inputs have decorators but queries don't?

### 7. Mutation Pattern Mismatch

**Initial Attempt**: Tried using class-based mutations like Strawberry:
```python
@fraiseql.mutation
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError

    async def resolve(self, info, input):
        ...
```

**Error**: "Mutation CreateMachine must define 'error' type"

**Correct Pattern**: Mutations should be functions returning Union types:
```python
async def create_machine(info, input: CreateMachineInput) -> CreateMachineSuccess | CreateMachineError:
    ...
```

### 8. Database URL Format

**Issue**: Settings provided psycopg connection string format:
```
"dbname='printoptim_db_local' user='lionel' host='localhost'..."
```

**Expected**: PostgreSQL URL format:
```
"postgresql://user:pass@host:port/dbname"
```

Had to add a new property to convert between formats.

### 9. Missing Import Location

**Confusion**: Where to import from?
- `fraiseql` - Types and decorators
- `fraiseql.fastapi` - `create_fraiseql_app`
- `fraiseql.cqrs` - `CQRSRepository`

The import structure wasn't immediately clear from docs.

### 10. Async Context Manager Lifecycle

**Problem**: The existing app used FastAPI's lifespan for async connection pool:
```python
@asynccontextmanager
async def lifespan(app: FastAPI):
    pool = AsyncConnectionPool(...)
    await pool.open()
    ...
```

**Unknown**: How FraiseQL handles async resource lifecycle management.

## What Would Have Helped

1. **Migration Guide**: A specific guide for migrating from Strawberry would be invaluable
2. **Query Examples**: Clear examples of how queries should be structured (not just mutations)
3. **Type Restrictions**: Document unsupported types upfront (like arbitrary JSON)
4. **Config Documentation**: List all environment variables that FraiseQL uses
5. **Error Messages**: More helpful error messages (e.g., "queries parameter expects X, got Y")
6. **Complete Working Example**: A full example with queries, mutations, and custom types
7. **Decorator Reference**: Complete list of available decorators and their purposes

## Positive Observations

Despite the challenges:
- The CQRS pattern is interesting and could simplify data access
- Type decorators are clean and familiar
- The migration from Strawberry types was mostly straightforward
- Error handling patterns with success/failure types are well thought out

## Conclusion

The migration attempt revealed that while FraiseQL has promise, the documentation and examples don't fully match the actual implementation. The query registration pattern in particular seems to be a fundamental blocker that prevented completing even a basic migration.

The confusion between what the examples show (passing query functions) and what the code expects (types for queries) suggests either:
1. The examples are outdated
2. There's a missing piece of configuration
3. The API has changed recently

This real-world migration attempt highlights the need for more comprehensive documentation and migration guides for users coming from other GraphQL frameworks.

## WebSocket Subscription Implementation (January 2025)

### Completed Implementation

Following Test-Driven Development approach, successfully implemented full WebSocket subscription support:

**Test Coverage**: All 28 subscription tests passing
- 4 subscription integration tests
- 8 core subscription tests
- 16 WebSocket subscription tests

**Implementation Details**:
1. **Protocol Support**: Both `graphql-ws` (legacy Apollo) and `graphql-transport-ws` (new standard) protocols
2. **Connection Management**: Complete lifecycle with states (CONNECTING → READY → CLOSING → CLOSED)
3. **Message Handling**: Full support for all message types (CONNECTION_INIT, SUBSCRIBE, COMPLETE, PING/PONG, etc.)
4. **Subscription Execution**: Async generator support with proper cleanup
5. **Keep-Alive**: Configurable ping/pong mechanism to detect disconnected clients
6. **Error Handling**: Comprehensive error handling with proper WebSocket close codes
7. **Broadcasting**: Built-in support for broadcasting to multiple connections
8. **FastAPI Integration**: Working example with HTML/JavaScript client

**Key Files Created/Modified**:
- `/src/fraiseql/subscriptions/websocket.py` - Complete WebSocket implementation
- `/tests/test_websocket_subscriptions.py` - Comprehensive test suite
- `/examples/websocket_fastapi.py` - Working FastAPI example
- `/src/fraiseql/core/exceptions.py` - Added WebSocketError
- `/src/fraiseql/subscriptions/__init__.py` - Updated exports

### Grumpy's Assessment Needed

*Viktor the Grumpy Investor enters, adjusting his glasses while reviewing the implementation*

"So you finally got WebSockets working, eh? Let me run through this implementation with my checklist..."

*Opens laptop and starts running tests*

```bash
$ uv run pytest tests/test_websocket_subscriptions.py -v
============================= test session starts ==============================
collected 16 items

tests/test_websocket_subscriptions.py ................                   [100%]
============================== 16 passed in 0.98s ==============================
```

"Hmm, 16 tests passing. Not bad. Let me check the broader subscription tests..."

```bash
$ uv run pytest tests/test_subscription*.py tests/test_subscriptions.py -v
============================= test session starts ==============================
collected 28 items
tests/test_subscription_integration.py ....                              [ 14%]
tests/test_subscriptions.py ........                                     [ 42%]
tests/test_websocket_subscriptions.py ................                   [100%]
============================== 28 passed in 1.69s ==============================
```

*Grumpy nods approvingly*

"Alright, you've got basic WebSocket functionality. But looking at this implementation and the user feedback above, we still have MAJOR issues to address before this is production-ready:

### 🚨 CRITICAL BLOCKERS

1. **Query Registration is STILL BROKEN**
   - Users can't even create a basic schema!
   - The blog shows passing functions, code expects types
   - This is embarrassing - fix it NOW

2. **Documentation is Completely Wrong**
   - Examples don't match implementation
   - Missing decorator reference
   - No migration guide from Strawberry

3. **DataLoader Not Implemented**
   - You've got WebSockets but no N+1 query prevention?
   - This will kill performance in production

### 📋 Grumpy's Priority List

**IMMEDIATE (Block everything else):**
1. Fix query registration - make it work like the blog example
2. Add @fraiseql.query decorator
3. Update ALL examples to actually work

**HIGH PRIORITY (This week):**
1. Implement DataLoader with automatic batching
2. Add comprehensive migration guide from Strawberry
3. Fix type system to support JSON/dict[str, Any]
4. Add context_getter parameter to create_fraiseql_app

**MEDIUM PRIORITY (Before beta):**
1. Performance benchmarks vs Strawberry
2. Production deployment guide
3. Security audit of WebSocket implementation
4. Rate limiting for subscriptions

**NICE TO HAVE:**
1. GraphQL playground with subscription support
2. Subscription filtering and authorization
3. Horizontal scaling guide for WebSockets

### 🔍 Code Review Notes

Your WebSocket implementation looks solid:
- ✅ Proper connection lifecycle
- ✅ Both protocol support
- ✅ Good error handling
- ✅ Clean async patterns

But it's useless if people can't even create a basic schema!

### 📊 Market Reality Check

Looking at the user feedback, we're not ready for users yet:
- Can't do basic query registration
- Environment variable conflicts
- Missing critical features (DataLoader, JSON support)
- Documentation actively misleads users

**Verdict**: Fix the fundamentals before adding more features. A working basic GraphQL server beats a broken one with WebSockets.

Now stop patting yourself on the back for WebSockets and FIX THE QUERY REGISTRATION!"

*Grumpy slams laptop closed*

"And update the version to 0.1.0a3 since you added WebSockets. At least version numbers should reflect reality."

## DataLoader Implementation Analysis (June 2025)

### Current Status Analysis

*Viktor checks his notes and starts reviewing the DataLoader implementation*

"Alright, I see you've got WebSockets working. Good. Now let's talk about this DataLoader situation because N+1 queries will kill any production application..."

*Opens the optimization module*

```bash
$ find src -name "*dataloader*" -o -name "*optimization*"
src/fraiseql/optimization/dataloader.py
src/fraiseql/optimization/registry.py
src/fraiseql/optimization/loaders.py
```

"Wait, what? You already HAVE DataLoader implementation? Let me check this..."

*Reviews the code*

```python
# From dataloader.py - Core implementation exists
class DataLoader(ABC, Generic[KeyType, ValueType]):
    async def load(self, key: KeyType) -> Optional[ValueType]:
        # Batching and caching logic...

    async def batch_load(self, keys: List[KeyType]) -> List[Optional[ValueType]]:
        # Abstract method for user implementation
```

"The core DataLoader is implemented with proper batching, caching, and generics. Good foundation. But looking at the examples..."

*Checks blog_api/queries.py*

```python
# Current resolver pattern - NO DataLoader usage!
async def resolve_post_author(post: Post, info) -> Optional[User]:
    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(UUID(post.author_id))  # ← N+1 QUERY!
    return User.from_dict(user_data) if user_data else None
```

"THERE'S THE PROBLEM! You have DataLoader implemented but NOBODY IS USING IT!"

### 🚨 Critical Assessment: Integration Gap

**The Issue**: DataLoader exists but isn't integrated with the GraphQL system.

**Evidence**:
1. ✅ Core DataLoader implementation is solid (batching, caching, typing)
2. ❌ FastAPI context doesn't include LoaderRegistry
3. ❌ Field resolvers don't use DataLoader
4. ❌ No automatic integration with @fraiseql.field decorator
5. ❌ Examples show N+1 query patterns instead of DataLoader usage

### 🎯 Grumpy's Priority Assessment

**CRITICAL (Fix immediately):**
1. **FastAPI Integration**: LoaderRegistry must be in GraphQL context automatically
2. **Field Decorator Integration**: `@fraiseql.field` should work seamlessly with DataLoader
3. **Update Examples**: Blog API must demonstrate DataLoader usage, not N+1 queries

**HIGH PRIORITY (This week):**
4. **Development Mode N+1 Detection**: Warn developers about potential N+1 queries
5. **DataLoader Testing Utilities**: Make it easy to test DataLoader integration
6. **Documentation**: Show real-world DataLoader usage patterns

**MEDIUM PRIORITY (Before production):**
7. **Performance Monitoring**: Metrics for DataLoader effectiveness
8. **Auto-generation**: Generate DataLoaders from schema relationships
9. **Cache Management**: Advanced caching strategies

### 🔧 Required Implementation Plan

**Phase 1: Make DataLoader Actually Usable**
```python
# Target: This should work out of the box
@fraiseql.type
class Post:
    author_id: UUID

    @fraiseql.field
    async def author(self, info) -> Optional[User]:
        # Should automatically use DataLoader if available
        loader = get_loader(UserLoader, info.context)
        return await loader.load(self.author_id)
```

**Phase 2: Make It Automatic**
```python
# Target: Even simpler integration
@fraiseql.type
class Post:
    author_id: UUID

    @dataloader_field(UserLoader)  # ← New decorator
    async def author(self, info) -> Optional[User]:
        return await self.load_related(self.author_id)
```

### 💡 What Users Actually Need

Based on the real user feedback, the biggest issues are:

1. **"I don't know DataLoader exists"** - No integration examples
2. **"I can't figure out how to use it"** - Missing FastAPI integration
3. **"My queries are slow"** - N+1 queries everywhere in examples
4. **"No guidance on setup"** - LoaderRegistry not in default context

### 📊 Market Reality Check

**Current State**: DataLoader is implemented but hidden/unusable
**User Expectation**: DataLoader should work automatically like in Strawberry/GraphQL-Core
**Production Requirement**: Sub-100ms response times require proper DataLoader usage

**Verdict**: Fix the integration gap NOW. The implementation is good, but if users can't easily use it, it's worthless.

*Grumpy adjusts his glasses and points at the examples*

"Look at your own blog example - it's doing N+1 queries! How can you expect users to avoid this if your own examples don't use DataLoader? Fix the integration, update the examples, and make DataLoader the default pattern, not an advanced feature."

## DataLoader Integration Implementation (June 2025 - Update)

### What Got Fixed

*Viktor runs a quick test of the implementation*

```bash
$ uv run pytest tests/fastapi/test_dataloader_integration.py tests/examples/test_blog_dataloader.py -v
============================= test session starts ==============================
collected 12 items

tests/fastapi/test_dataloader_integration.py ......                      [ 50%]
tests/examples/test_blog_dataloader.py ......                            [100%]

============================== 12 passed in 0.24s ===============================
```

"Hmm, 12 tests passing. Let me check what actually got implemented..."

*Reviews the changes*

**✅ FIXED: FastAPI Integration**
```python
# In dependencies.py - LoaderRegistry automatically added to context
async def build_graphql_context(
    db: Annotated[FraiseQLRepository, Depends(get_db)],
    user: Annotated[UserContext | None, Depends(get_current_user_optional)],
) -> dict[str, Any]:
    # Create a new LoaderRegistry for this request
    loader_registry = LoaderRegistry(db=db)
    LoaderRegistry.set_current(loader_registry)

    return {
        "db": db,
        "user": user,
        "authenticated": user is not None,
        "loader_registry": loader_registry,
    }
```

**✅ FIXED: Blog Example N+1 Queries**
```python
# Old pattern (N+1 queries)
async def resolve_post_author(post: Post, info) -> Optional[User]:
    db: BlogRepository = info.context["db"]
    user_data = await db.get_user_by_id(UUID(post.author_id))  # ← N+1!
    return User.from_dict(user_data) if user_data else None

# New pattern (DataLoader batching)
async def resolve_post_author(post: Post, info) -> Optional[User]:
    user_loader = get_loader(UserDataLoader)
    user_data = await user_loader.load(UUID(post.author_id))  # ← Batched!
    return User.from_dict(user_data) if user_data else None
```

**✅ FIXED: Repository Batch Methods**
```python
# Added to BlogRepository
async def get_users_by_ids(self, user_ids: list[str]) -> list[dict[str, Any]]:
    return await self.select_from_json_view("v_users", where={"id": {"$in": user_ids}})

async def get_comments_by_post_ids(self, post_ids: list[str]) -> list[dict[str, Any]]:
    return await self.select_from_json_view("v_comments", where={"postId": {"$in": post_ids}})
```

**✅ FIXED: Complete DataLoader Suite**
- `UserDataLoader` - Batch user lookups
- `CommentsByPostDataLoader` - Batch comment lookups by post
- `PostDataLoader` - Batch post lookups

### 🎯 Assessment: MAJOR IMPROVEMENT

**Before**: DataLoader existed but was completely unusable
**After**: DataLoader works out of the box with FastAPI integration

### 📊 What This Means for Users

**Old user experience**:
1. "How do I use DataLoader?" - No idea, no integration
2. Examples showed N+1 query anti-patterns
3. Manual setup required, no documentation

**New user experience**:
1. LoaderRegistry automatically available in GraphQL context
2. Blog example demonstrates proper DataLoader usage
3. `get_loader(MyDataLoader)` just works
4. Clear performance benefits shown in tests

### 🚨 What's Still Missing

**CRITICAL (Still needed for production)**:
1. **@dataloader_field decorator** - Make it even easier
2. **N+1 detection in dev mode** - Warn when DataLoader should be used
3. **Performance metrics** - Show actual performance improvements

**HIGH PRIORITY**:
4. **Auto-generation from schema** - Generate DataLoaders from relationships
5. **Documentation examples** - More real-world patterns

### 🔧 Current Developer Experience

**Level 1: Manual (Now Works)**
```python
# Field resolver with explicit DataLoader
async def resolve_post_author(post: Post, info) -> Optional[User]:
    loader = get_loader(UserDataLoader)
    return await loader.load(post.author_id)
```

**Level 2: Decorator (Not implemented)**
```python
# Target: @dataloader_field decorator
@fraiseql.type
class Post:
    @dataloader_field(UserDataLoader)
    async def author(self, info) -> Optional[User]:
        return await self.load_related(self.author_id)
```

**Level 3: Auto-generation (Not implemented)**
```python
# Target: Automatic DataLoader generation
@fraiseql.type
class Post:
    author: User = dataloader_relation('author_id')  # Auto-generates DataLoader
```

### 💡 Grumpy's Updated Verdict

**Progress**: ✅ **GOOD** - The integration gap is fixed
**Status**: 🟡 **WORKABLE** - Users can now actually use DataLoader
**Production Ready**: 🔴 **NOT YET** - Still missing developer experience features

**What users can do now**:
- ✅ Use DataLoader without manual setup
- ✅ Follow blog example patterns
- ✅ Get automatic batching and caching
- ✅ Use get_loader() in any resolver

**What users still can't do easily**:
- ❌ Get warned about N+1 queries
- ❌ Use declarative field decorators
- ❌ Auto-generate DataLoaders from schema

### 📈 Performance Impact

The blog example now demonstrates:
- **N+1 queries eliminated** in all field resolvers
- **Batch database queries** for users, posts, comments
- **Request-scoped caching** preventing duplicate lookups
- **Proper async concurrency** with DataLoader

**Real-world impact**: For a query loading 100 posts with authors and comments:
- **Before**: 1 + 100 + 100 = 201 database queries
- **After**: 1 + 1 + 1 = 3 database queries (99% reduction)

### 🎯 Next Priority

**The @dataloader_field decorator is now the highest priority.** The manual pattern works, but users expect declarative patterns like:

```python
@fraiseql.type
class Post:
    @dataloader_field(UserDataLoader, 'author_id')
    async def author(self, info) -> User:
        pass  # Auto-implemented
```

This would make DataLoader usage as simple as Strawberry or other frameworks.

*Grumpy nods approvingly*

"Finally! Users can actually use DataLoader now. The blog example is no longer embarrassing. But don't get comfortable - we still need that decorator and N+1 detection to make this truly production-ready."
