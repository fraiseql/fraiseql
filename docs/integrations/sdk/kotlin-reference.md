---
title: FraiseQL Kotlin SDK Reference
description: Complete API reference for the FraiseQL Kotlin SDK. This guide covers the complete Kotlin authoring interface for building type-safe GraphQL APIs using Kotlin's
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Kotlin SDK Reference

**Status**: Production-Ready | **Kotlin Version**: 1.9+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Kotlin SDK. This guide covers the complete Kotlin authoring interface for building type-safe GraphQL APIs using Kotlin's modern language features: data classes, nullable types, sealed classes, extension functions, and coroutines.

## Installation & Setup

### Gradle (Kotlin DSL)

Add to your `build.gradle.kts`:

```kotlin
dependencies {
    implementation("com.FraiseQL:FraiseQL-kotlin:2.0.0")

    // Optional: For annotation processing
    kapt("com.FraiseQL:FraiseQL-processor:2.0.0")
}

repositories {
    mavenCentral()
}

kotlin {
    jvmToolchain(11)
    compilerOptions {
        javaParameter = true
        freeCompilerArgs.addAll("-opt-in=kotlin.ExperimentalStdlibApi")
    }
}
```

### Requirements

- **Kotlin 1.9+** (Full support, all features)
- **Kotlin 2.0+** (Latest syntax, improved performance)
- **Java 11+** (JVM target version)
- **Gradle 7.0+** (Build system)
- **Spring Boot 3.0+** (Optional, for Spring integration)
- **Android API 26+** (For Android development)

### First Schema (60 seconds)

```kotlin
import com.FraiseQL.*

@Type
data class User(
    @Field val id: Int,
    @Field val name: String,
    @Field val email: String?
)

fun main() {
    FraiseQL.registerType<User>()

    query("user") {
        returns<User>()
        arg("id", "Int")
        description("Get a user by ID")
    }

    FraiseQL.exportSchema("schema.json")
    println("Schema exported!")
}
```

Export and deploy to your FraiseQL server:

```bash
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Annotation | Purpose | Extension/Builder | Returns |
|---------|-----------|---------|---|---|
| **Type Definition** | `@Type` | GraphQL object type | `registerType<T>()` | Type info |
| **Field Definition** | `@Field` | Type field | N/A | Schema field |
| **Query Operation** | N/A | Read operation | `query(String)` | QueryBuilder |
| **Mutation Operation** | N/A | Write operation | `mutation(String)` | MutationBuilder |
| **Subscription** | N/A | Real-time stream | `subscription(String)` | SubscriptionBuilder |
| **Fact Table** | `@FactTable` | Analytics table | `factTable<T>()` | Fact table info |
| **Security/RBAC** | `@Secured`, `@RoleRequired` | Access control | `securedQuery(String)` | Secured builder |
| **Custom Directive** | `@Directive` | Schema directive | `directive(String)` | Directive info |
| **Field Observer** | `@Observer` | Event webhook | `observer(String)` | Observer info |

---

## Type System

### 1. @Type Annotation with Data Classes

Marks a data class (or regular class) as a GraphQL type definition. Kotlin data classes provide automatic `equals()`, `hashCode()`, and `copy()` methods.

```kotlin
// Primary constructor data class (recommended)
@Type
data class User(
    @Field val id: Int,
    @Field val name: String,
    @Field val email: String?  // Nullable type built-in
)

// With custom getter (secondary constructor)
@Type
data class Product(
    @Field val id: Int,
    @Field val name: String,
    @Field val price: Double
) {
    @Field
    val discountedPrice: Double
        get() = price * 0.9
}

// Complex type with nesting
@Type
data class Post(
    @Field val id: Int,
    @Field val title: String,
    @Field val author: User,
    @Field val comments: List<Comment> = emptyList()
)
```

**Best Practices:**

- Use data class primary constructors for immutability
- Apply `@Field` to val properties and getters
- Leverage Kotlin's nullable types (`T?`) for optional fields
- Use default parameters for collection fields

### 2. @Field Annotation

Marks a property or getter as part of a GraphQL type. Supports nullability, custom names, and type overrides.

```kotlin
@Type
data class Account(
    @Field(description = "Unique account identifier")
    val id: Int,

    @Field
    val accountNumber: String,

    @Field(name = "currentBalance", description = "Account balance in USD")
    val balance: Double,

    @Field(type = "[String]!")
    val tags: List<String> = emptyList(),

    // Nullable field
    @Field
    val notes: String? = null
)
```

**Attributes:**

- `nullable` (Boolean, default inferred from type) - Override nullability (rarely needed)
- `name` (String) - Custom field name in schema
- `type` (String) - Custom GraphQL type (auto-detected if omitted)
- `description` (String) - Field documentation for schema

**Type Detection Rules:**

| Kotlin Type | GraphQL Type | Nullable | Example |
|-----------|--------------|----------|---------|
| `Int` | `Int!` | No | `@Field val count: Int` |
| `Int?` | `Int` | Yes | `@Field val count: Int?` |
| `String` | `String!` | No | `@Field val name: String` |
| `String?` | `String` | Yes | `@Field val email: String?` |
| `Boolean` | `Boolean!` | No | `@Field val active: Boolean` |
| `Double` | `Float!` | No | `@Field val price: Double` |
| `LocalDate` | `String!` | No | `@Field val created: LocalDate` |
| `LocalDateTime` | `String!` | No | `@Field val updated: LocalDateTime` |
| `UUID` | `ID!` | No | `@Field val uuid: UUID` |
| `List<T>` | `[T]!` | No | `@Field val tags: List<String>` |
| `List<T>?` | `[T]` | Yes | `@Field val tags: List<String>?` |

### 3. Sealed Classes for Discriminated Unions

Use sealed classes to model GraphQL union types:

```kotlin
@Type
sealed class SearchResult {
    @Type
    data class UserResult(
        @Field val user: User,
        @Field val relevance: Double
    ) : SearchResult()

    @Type
    data class PostResult(
        @Field val post: Post,
        @Field val relevance: Double
    ) : SearchResult()
}

// Register sealed class - all subtypes registered automatically
FraiseQL.registerType<SearchResult>()
```

### 4. Generics and Complex Types

```kotlin
@Type
data class Page<T>(
    @Field val items: List<T>,
    @Field val totalCount: Int,
    @Field val pageNumber: Int,
    @Field val pageSize: Int
)

@Type
data class UserPage(
    @Field val items: List<User>,
    @Field val totalCount: Int,
    @Field val pageNumber: Int
)

// With type variable bounds
@Type
data class Connection<T : Any>(
    @Field val edges: List<T>,
    @Field val cursor: String?
)
```

---

## Operations

### Query Operations

Queries are read-only operations that fetch data. Use extension functions with lambda builders:

```kotlin
// Simple query with extension function
query("user") {
    returns<User>()
    arg("id", "Int")
    description("Get a user by ID")
}

// Query returning array
query("users") {
    returns<User>()
    returnsArray(true)
    arg("limit", "Int", default = 10)
    arg("offset", "Int", default = 0)
    description("Get paginated users")
}

// Query with optional arguments using scope functions
query("search") {
    returns<Post>()
    returnsArray(true)
    arg("query", "String")
    arg("limit", "Int", default = 20)
    arg("filter", "String?")  // Nullable argument
    description("Search posts by query and optional filter")
}

// Complex query with context
query("userProfile") {
    returns<User>()
    arg("id", "Int")
    description("Get user profile with computed fields")
    securityRule("isOwnerOrAdmin(\$context, \$args.id)")
}
```

**QueryBuilder Methods (Lambda):**

```kotlin
query(name: String) {
    returns<T>()                        // Set return type (required)
    returnsArray(true/false)            // Whether result is array
    arg(name, "GraphQL Type", default?) // Add argument
    arg(name, "GraphQL Type?")          // Nullable argument
    description("documentation")        // Add documentation
    deprecationReason("reason")?        // Mark as deprecated
    securityRule("rule expression")     // Add security rule
}
```

### Mutation Operations

Mutations are write operations that modify data (INSERT, UPDATE, DELETE):

```kotlin
// Create mutation with named parameters
mutation("createUser") {
    returns<User>()
    arg("email", "String")
    arg("name", "String")
    arg("role", "String", default = "user")
    description("Create a new user")
}

// Update mutation
mutation("updateUser") {
    returns<User>()
    arg("id", "Int")
    arg("name", "String?")              // Optional update
    arg("email", "String?")
    description("Update user by ID")
    securityRule("isOwner(\$context.userId, \$args.id)")
}

// Delete mutation with scope function
mutation("deleteUser") {
    returns<User>()
    arg("id", "Int")
    description("Delete user by ID")
}.apply {
    // Additional configuration with apply
}

// Batch mutation
mutation("bulkDeleteUsers") {
    returns<User>()
    returnsArray(true)
    arg("ids", "[Int]!")
    description("Delete multiple users")
}
```

**MutationBuilder Methods:**

Identical to QueryBuilder interface.

### Subscription Operations (Real-time)

Subscriptions enable real-time event streaming via WebSocket:

```kotlin
subscription("userCreated") {
    returns<User>()
    description("Subscribe to new user creation events")
}

subscription("postUpdated") {
    returns<Post>()
    arg("userId", "Int?")
    description("Subscribe to post updates for specific user")
}

// Topic-based subscription with coroutines
subscription("orderStatus") {
    returns<Order>()
    arg("orderId", "Int")
    arg("topic", "String?")
    description("Subscribe to order status changes")
}
```

---

## Advanced Features

### Fact Tables (Analytics/OLAP)

Define analytical tables for OLAP queries with dimensions and measures:

```kotlin
@FactTable(name = "sales_fact", sqlSource = "fact_sales")
data class SalesFact(
    @Field val dateKey: Int,
    @Field val productKey: Int,
    @Field val storeKey: Int,
    @Field val revenue: Double,
    @Field val quantity: Int,
    @Field val cost: Double
)

// Register fact table
FraiseQL.registerType<SalesFact>()

// Aggregate query on fact table
query("salesByProduct") {
    returns<SalesFact>()
    returnsArray(true)
    arg("productKey", "Int")
    arg("dateRange", "String?")
    description("Total sales by product")
}
```

### RBAC & Security Annotations

Define role-based access control at query/mutation/field level:

```kotlin
@Type
@RoleRequired(roles = ["ADMIN"])
data class AdminPanel(
    @Field val systemHealth: String,
    @Field val metrics: List<String>
)

// Secured query - only ADMIN role
query("adminMetrics") {
    returns<AdminPanel>()
    description("System metrics (admin only)")
}

// Secured mutation - multiple roles
mutation("banUser") {
    returns<User>()
    arg("userId", "Int")
    arg("reason", "String")
    description("Ban user from system")
    securityRule("hasRole(\$context, 'ADMIN') OR hasRole(\$context, 'MODERATOR')")
}

// Field-level security
@Type
data class Account(
    @Field val id: Int,
    @Field val accountNumber: String,
    @Field val balance: Double,

    // Sensitive field - only visible to OWNER or ADMIN
    @Field
    @Secured(roles = ["OWNER", "ADMIN"])
    val accountSsn: String?
)
```

### Custom Directives

Define custom GraphQL directives for schema extensions:

```kotlin
@Directive(name = "auth", description = "Requires authentication")
data class AuthDirective(
    val roles: List<String>? = null
)

@Directive(name = "cache", description = "Cache directive")
data class CacheDirective(
    val ttl: Int,
    val scope: String = "public"
)

// Use in queries
query("profile") {
    returns<User>()
    description("Get user profile")
    directive("auth", mapOf("roles" to listOf("user")))
}
```

### Field Observers (Event Webhooks)

Trigger external webhooks when fields change:

```kotlin
@Observer(
    name = "onUserCreated",
    webhook = "https://api.example.com/webhooks/user-created"
)
data class UserCreatedEvent(
    val userId: Int,
    val email: String,
    val name: String
)

@Observer(
    name = "onOrderShipped",
    webhook = "https://api.example.com/webhooks/order-shipped"
)
data class OrderShippedEvent(
    val orderId: Int,
    val trackingNumber: String,
    val shipDate: LocalDateTime
)

// Register observers
FraiseQL.registerObserver<UserCreatedEvent>()
FraiseQL.registerObserver<OrderShippedEvent>()
```

---

## Scalar Types Reference

FraiseQL supports 60+ scalar types mapped from Kotlin to GraphQL:

```kotlin
// String types
@Field val name: String                 // String!
@Field val description: String          // String!
@Field val letter: Char                 // String!

// Numeric types
@Field val count: Int                   // Int!
@Field val bigNumber: Long              // Int!
@Field val rating: Float                // Float!
@Field val price: Double                // Float!
@Field val hugeNumber: BigInteger       // BigInt!
@Field val money: BigDecimal            // Decimal!

// Boolean type
@Field val active: Boolean              // Boolean!
@Field val isActive: Boolean?           // Boolean

// Date/Time types (java.time)
@Field val date: LocalDate              // String! (ISO 8601)
@Field val datetime: LocalDateTime      // String! (ISO 8601)
@Field val time: LocalTime              // String! (ISO 8601)
@Field val zdt: ZonedDateTime           // String! (ISO 8601)
@Field val instant: Instant             // String! (ISO 8601)

// ID type
@Field val uuid: UUID                   // ID!
@Field val id: String                   // ID!

// Collections
@Field val tags: List<String>           // [String]!
@Field val items: Set<Int>              // [Int]!
@Field val flags: BooleanArray          // [Boolean]!

// Optional (Nullable) - Kotlin native syntax
@Field val optional: String?            // String
@Field val required: String             // String!
```

---

## Schema Export & Compilation

### Export Workflow

```kotlin
import com.FraiseQL.*

object SchemaBuilder {
    @JvmStatic
    fun main(args: Array<String>) {
        // 1. Register all types
        registerTypes()

        // 2. Register queries
        registerQueries()

        // 3. Register mutations
        registerMutations()

        // 4. Validate schema
        val result = SchemaValidator.validate()

        if (!result.valid) {
            System.err.println("Schema validation failed:")
            result.errors.forEach { System.err.println("  ERROR: $it") }
            System.exit(1)
        }

        // 5. Export schema
        FraiseQL.exportSchema("schema.json")
        println("✓ Schema exported: schema.json")
        println(SchemaValidator.getStatistics())
    }

    private fun registerTypes() {
        FraiseQL.registerType<User>()
        FraiseQL.registerType<Post>()
        FraiseQL.registerType<Comment>()
    }

    private fun registerQueries() {
        query("user") {
            returns<User>()
            arg("id", "Int")
        }

        query("posts") {
            returns<Post>()
            returnsArray(true)
        }
    }

    private fun registerMutations() {
        mutation("createPost") {
            returns<Post>()
            arg("title", "String")
            arg("content", "String")
        }
    }
}
```

Run export:

```bash
# Gradle
./gradlew run

# Then compile schema
FraiseQL-cli compile schema.json FraiseQL.toml
```

---

## Type Mapping Reference

Complete Kotlin ↔ GraphQL type mappings:

| Category | Kotlin Type | GraphQL Type | Nullable | Example |
|----------|-----------|--------------|----------|---------|
| **String** | `String` | `String!` | No | `"Hello"` |
| **String** | `String?` | `String` | Yes | `null` |
| **Integer** | `Int` | `Int!` | No | `42` |
| **Integer** | `Int?` | `Int` | Yes | `null` |
| **Long** | `Long` | `Int!` | No | `999999L` |
| **Float** | `Float` | `Float!` | No | `3.14f` |
| **Double** | `Double` | `Float!` | No | `3.14159` |
| **Decimal** | `BigDecimal` | `Decimal!` | No | `BigDecimal("123.45")` |
| **Boolean** | `Boolean` | `Boolean!` | No | `true` |
| **List** | `List<T>` | `[T]!` | No | `listOf(1, 2, 3)` |
| **Array** | `IntArray` | `[Int]!` | No | `intArrayOf(1, 2)` |
| **Set** | `Set<T>` | `[T]!` | No | `setOf(1, 2, 3)` |
| **UUID** | `UUID` | `ID!` | No | `UUID.randomUUID()` |
| **Date** | `LocalDate` | `String!` | No | `LocalDate.now()` |
| **DateTime** | `LocalDateTime` | `String!` | No | `LocalDateTime.now()` |
| **Instant** | `Instant` | `String!` | No | `Instant.now()` |

---

## Common Patterns

### CRUD with Extension Functions

```kotlin
@Type
data class Article(
    @Field val id: Int,
    @Field val title: String,
    @Field val content: String,
    @Field val createdAt: LocalDateTime,
    @Field val updatedAt: LocalDateTime
)

object ArticleSchema {
    fun registerSchema() {
        FraiseQL.registerType<Article>()

        // Create
        mutation("createArticle") {
            returns<Article>()
            arg("title", "String")
            arg("content", "String")
        }

        // Read one
        query("article") {
            returns<Article>()
            arg("id", "Int")
        }

        // Read many
        query("articles") {
            returns<Article>()
            returnsArray(true)
            arg("limit", "Int", default = 20)
            arg("offset", "Int", default = 0)
        }

        // Update
        mutation("updateArticle") {
            returns<Article>()
            arg("id", "Int")
            arg("title", "String?")
            arg("content", "String?")
        }

        // Delete
        mutation("deleteArticle") {
            returns<Article>()
            arg("id", "Int")
        }
    }
}
```

### Pagination with Coroutines

```kotlin
@Type
data class UserConnection(
    @Field val edges: List<User>,
    @Field val totalCount: Int,
    @Field val hasNextPage: Boolean,
    @Field val pageInfo: PageInfo
)

@Type
data class PageInfo(
    @Field val startCursor: String?,
    @Field val endCursor: String?,
    @Field val hasNextPage: Boolean,
    @Field val hasPreviousPage: Boolean
)

query("usersPaginated") {
    returns<UserConnection>()
    arg("first", "Int", default = 10)
    arg("after", "String?")
    arg("sort", "String", default = "name")
    description("Paginated user list with cursor support")
}

// Async with coroutines
suspend fun fetchUsers(first: Int, after: String?): UserConnection = coroutineScope {
    val usersDeferred = async { /* fetch users */ }
    val countDeferred = async { /* fetch count */ }

    val users = usersDeferred.await()
    val count = countDeferred.await()

    UserConnection(users, count, count > users.size, PageInfo(null, null, true, false))
}
```

### Android Development

```kotlin
// In Android ViewModel
@HiltViewModel
class UserViewModel @Inject constructor(
    private val fraiseqlClient: FraiseQLClient
) : ViewModel() {

    private val _users = MutableStateFlow<List<User>>(emptyList())
    val users: StateFlow<List<User>> = _users.asStateFlow()

    fun loadUsers(limit: Int = 20) {
        viewModelScope.launch {
            try {
                val result = fraiseqlClient.query<List<User>>(
                    "users",
                    mapOf("limit" to limit)
                )
                _users.value = result
            } catch (e: Exception) {
                // Handle error
            }
        }
    }
}

// In Android Fragment
Fragment {
    val viewModel: UserViewModel = viewModel()
    val users by viewModel.users.collectAsState()

    LazyColumn {
        items(users) { user ->
            UserCard(user)
        }
    }
}
```

---

## Error Handling

### Result Sealed Class Pattern

```kotlin
sealed class Result<T> {
    data class Success<T>(val data: T) : Result<T>()
    data class Error<T>(val exception: FraiseQLException) : Result<T>()

    inline fun <R> map(transform: (T) -> R): Result<R> = when (this) {
        is Success -> Success(transform(data))
        is Error -> Error(exception)
    }
}

// Usage with when expression
when (val result = FraiseQL.query<User>("user", mapOf("id" to 1))) {
    is Result.Success -> println("User: ${result.data}")
    is Result.Error -> println("Error: ${result.exception.message}")
}
```

### Custom Exception Handling

```kotlin
sealed class FraiseQLException(message: String) : Exception(message) {
    data class ValidationError(val message: String) : FraiseQLException(message)
    data class DatabaseError(val code: String, val message: String) : FraiseQLException(message)
    data class AuthorizationError(val message: String) : FraiseQLException(message)
}

// Usage with try-catch
try {
    val schema = SchemaValidator.validate()
    if (!schema.valid) {
        throw FraiseQLException.ValidationError(schema.errors.joinToString())
    }
} catch (e: FraiseQLException.ValidationError) {
    System.err.println("Validation error: ${e.message}")
} catch (e: FraiseQLException.AuthorizationError) {
    System.err.println("Access denied: ${e.message}")
}
```

---

## Testing

### Kotest Patterns

```kotlin
import io.kotest.core.spec.style.FunSpec
import io.kotest.matchers.shouldBe
import io.kotest.matchers.shouldNotBeNull

class SchemaTest : FunSpec({
    beforeTest {
        FraiseQL.clear()
        SchemaCache.getInstance().clear()
    }

    test("register user type") {
        FraiseQL.registerType<User>()
        val registry = SchemaRegistry.getInstance()
        val userType = registry.getType("User")

        userType.shouldNotBeNull()
        userType.name shouldBe "User"
    }

    test("register query operation") {
        FraiseQL.registerType<User>()
        query("user") {
            returns<User>()
            arg("id", "Int")
        }

        val registry = SchemaRegistry.getInstance()
        val query = registry.getQuery("user")

        query.shouldNotBeNull()
    }

    test("validate schema") {
        FraiseQL.registerType<User>()
        query("user") {
            returns<User>()
            arg("id", "Int")
        }

        val result = SchemaValidator.validate()
        result.valid shouldBe true
        result.errors.shouldBe(emptyList())
    }
})
```

### JUnit 5 Patterns

```kotlin
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import kotlin.test.assertTrue
import kotlin.test.assertNotNull

class SchemaValidationTest {

    @BeforeEach
    fun setUp() {
        FraiseQL.clear()
    }

    @Test
    fun `should export schema to JSON` () {
        FraiseQL.registerType<User>()
        val schemaJson = FraiseQL.exportSchema()

        assertNotNull(schemaJson)
        assertTrue(schemaJson.contains("User"))
    }
}
```

---

## See Also

- [Kotlin Documentation](https://kotlinlang.org/) - Official Kotlin language docs
- [FraiseQL Architecture Guide](../../architecture/README.md) - System design
- [Java SDK Reference](./java-reference.md) - Comparison with Java SDK
- [Python SDK Reference](./python-reference.md) - Python SDK documentation
- [TypeScript SDK Reference](./typescript-reference.md) - TypeScript SDK documentation
- [Security & RBAC Guide](../../guides/authorization-quick-start.md) - Authorization patterns
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md) - Fact tables and aggregations
- [GraphQL Scalar Types](../../reference/scalars.md) - Complete scalar type reference

---

## Troubleshooting

### Common Setup Issues

#### Gradle Dependency Resolution

**Issue**: `Could not find com.FraiseQL:FraiseQL-kotlin:2.0.0`

**Solution**:

```gradle
repositories {
    mavenCentral()
}

dependencies {
    implementation 'com.FraiseQL:FraiseQL-kotlin:2.0.0'
}
```

```bash
./gradlew clean build --refresh-dependencies
```

#### Kotlin Compiler Issues

**Issue**: `Unresolved reference: FraiseQL`

**Check Kotlin version** (1.8+ required):

```gradle
plugins {
    kotlin("jvm") version "1.9.0"
}
```

#### Interop Issues

**Issue**: `Cannot find Java class in Kotlin`

**Solution - Configure interop**:

```gradle
kotlin {
    jvmTarget = "11"
}

sourceSets {
    main.kotlin.srcDirs += 'src/main/java'
}
```

#### Coroutine Setup

**Issue**: `Unresolved reference: async` or `launch`

**Add coroutines dependency**:

```gradle
dependencies {
    implementation 'org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.0'
    implementation 'org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.7.0'
}
```

---

### Type System Issues

#### Nullable Type Issues

**Issue**: `Type mismatch: inferred type is User? but User was expected`

**Solution - Handle nullability explicitly**:

```kotlin
// ❌ Wrong - nullable but treated as non-null
@FraiseQLType
data class User(val name: String)  // Should be String?

// ✅ Correct
@FraiseQLType
data class User(val name: String?)  // Nullable

// Or non-null with init check
@FraiseQLType
data class User(val name: String) {
    init {
        require(name.isNotBlank()) { "Name required" }
    }
}
```

#### Data Class Limitations

**Issue**: `Cannot use type with parameter`

**Solution - Use concrete data classes**:

```kotlin
// ❌ Won't work - generics
@FraiseQLType
data class Box<T>(val value: T)

// ✅ Use concrete types
@FraiseQLType
data class UserBox(val value: User)
```

#### Property Delegation Issues

**Issue**: `No primary constructor found`

**Solution - Use simple data classes**:

```kotlin
// ✅ Simple properties work
@FraiseQLType
data class User(
    val id: Int,
    val email: String,
    val middleName: String? = null
)
```

#### Sealed Class Issues

**Issue**: `Cannot instantiate sealed class`

**Solution - Use regular classes or objects**:

```kotlin
// ✅ Use normal inheritance or composition
@FraiseQLType
data class User(val id: Int, val status: String)

// For union types
@FraiseQLType
data class Result(
    val user: User? = null,
    val error: String? = null
)
```

---

### Runtime Errors

#### Coroutine Context Issues

**Issue**: `Exception in thread "main" java.lang.IllegalStateException: No CoroutineScope`

**Solution - Provide scope**:

```kotlin
// ❌ Wrong - no scope
val result = FraiseQL.execute(query)

// ✅ With scope
runBlocking {
    val result = FraiseQL.executeAsync(query)
}

// Or in controller
@PostMapping("/graphql")
suspend fun graphql(@RequestBody request: GraphQLRequest): QueryResult {
    return FraiseQL.executeAsync(request.query)
}
```

#### Java Interop Issues

**Issue**: `NullPointerException in Java code called from Kotlin`

**Solution - Null safety**:

```kotlin
// ✅ Use let or !! carefully
val result = javaMethod()?.let { process(it) }

// Or assert non-null
val result = javaMethod()!!  // Only if sure it's not null
```

#### Extension Function Issues

**Issue**: `Cannot extend FraiseQL classes`

**Solution - Create extension functions instead**:

```kotlin
// ✅ Add functionality via extension
fun Server.executeWithTimeout(query: String, timeoutMs: Long = 30000): QueryResult {
    return withTimeoutOrNull(timeoutMs) {
        this.execute(query)
    } ?: throw TimeoutException()
}

// Usage
server.executeWithTimeout(query)
```

#### Scope Function Misuse

**Issue**: `Returned value is lost`

**Solution - Use correct scope function**:

```kotlin
// ✅ let for transforming result
val ids = users.let { it.map { u -> u.id } }

// ✅ apply for configuration
val server = Server.from_compiled("schema.json").apply {
    logger.level = Level.DEBUG
}

// ✅ run for executing in context
server.run {
    execute(query)
}
```

---

### Performance Issues

#### Build Time

**Issue**: Build takes >30 seconds**

**Parallel compilation**:

```gradle
org.gradle.parallel=true
org.gradle.workers.max=4
```

**Or command line**:

```bash
./gradlew build --parallel --max-workers=4
```

#### Coroutine Overhead

**Issue**: Many coroutines slow down execution**

**Limit concurrency**:

```kotlin
val dispatcher = Dispatchers.Default.limitedParallelism(4)

launch(dispatcher) {
    server.executeAsync(query)
}
```

#### Memory Usage

**Issue**: Application uses >500MB**

**Profile with Kotlin**:

```kotlin
val runtime = Runtime.getRuntime()
val memory = runtime.totalMemory() - runtime.freeMemory()
println("Memory: ${memory / 1024 / 1024}MB")
```

---

### Debugging Techniques

#### Logging Setup

**Add logging**:

```gradle
dependencies {
    implementation 'io.github.microutils:kotlin-logging:3.0.0'
    implementation 'ch.qos.logback:logback-classic:1.4.0'
}
```

```kotlin
import mu.KotlinLogging

val logger = KotlinLogging.logger {}

fun main() {
    logger.debug { "Starting" }
    server.execute(query)
}
```

**Run with debug**:

```bash
java -Dorg.slf4j.simpleLogger.defaultLogLevel=debug -jar app.jar
```

#### IDE Debugging

**IntelliJ IDEA**:

1. Set breakpoint
2. Debug Run (Shift+F9)
3. Step through (F10)
4. Inspect with Alt+Q

#### Testing

```kotlin
@Test
fun testQuery() {
    val server = Server.from_compiled("schema.json")
    val result = server.execute("{ user(id: 1) { id } }")
    assertEquals(1, (result["data"]["user"]["id"] as Int))
}
```

---

### Getting Help

#### GitHub Issues

Provide:

1. Kotlin version: `kotlinc -version`
2. Java version: `java -version`
3. FraiseQL version
4. Minimal reproducible example
5. Full error trace

**Environment**:

```markdown
- Kotlin: 1.9.0
- Java: 11
- FraiseQL: 2.0.0
```

#### Community Channels

- **GitHub Discussions**: Q&A
- **Kotlin Slack**: Kotlin community
- **Stack Overflow**: Tag with `kotlin` and `FraiseQL`

---

**Status**: Production Ready ✅ | **Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community
