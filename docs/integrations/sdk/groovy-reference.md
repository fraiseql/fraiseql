<!-- Skip to main content -->
---
title: FraiseQL Groovy SDK Reference
description: Complete API reference for the FraiseQL Groovy SDK. This guide covers the Groovy authoring interface for building type-safe GraphQL APIs with Groovy's dynamic f
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL Groovy SDK Reference

**Status**: Production-Ready | **Groovy Version**: 4.0+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Groovy SDK. This guide covers the Groovy authoring interface for building type-safe GraphQL APIs with Groovy's dynamic features, DSLs, closures, and full Java interoperability. Leverage dynamic typing, metaprogramming, and expressive syntax for rapid schema development.

## Installation & Setup

### Gradle (Recommended)

```gradle
<!-- Code example in GRADLE -->
plugins {
    id 'groovy'
    id 'java'
}

dependencies {
    implementation 'org.apache.groovy:groovy:4.0.+'
    implementation 'com.FraiseQL:FraiseQL-SDK:2.0.0'
    annotationProcessor 'com.FraiseQL:FraiseQL-processor:2.0.0'

    // Optional: Spring Boot integration
    implementation 'org.springframework.boot:spring-boot-starter:3.0.+'

    // Testing: Spock framework
    testImplementation 'org.spockframework:spock-core:2.3-groovy-4.0'
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}
```text
<!-- Code example in TEXT -->

### Maven

```xml
<!-- Code example in XML -->
<dependency>
    <groupId>com.FraiseQL</groupId>
    <artifactId>FraiseQL-SDK</artifactId>
    <version>2.0.0</version>
</dependency>

<plugin>
    <groupId>org.codehaus.groovy</groupId>
    <artifactId>groovy-eclipse-compiler</artifactId>
    <version>3.3.0-01</version>
</plugin>
```text
<!-- Code example in TEXT -->

### Requirements

- **Groovy 4.0+** (Latest: 4.0.10+)
- **Java 11+** (Full support)
- **Java 21+** (Virtual threads, pattern matching)
- Gradle 7.0+ or Maven 3.8+
- Optional: Spring Boot 3.0+ for framework integration

### First Schema (60 seconds)

```groovy
<!-- Code example in GROOVY -->
import com.FraiseQL.*

@GraphQLType
class User {
    @GraphQLField int id
    @GraphQLField String name
    @GraphQLField(nullable = true) String email
}

FraiseQL.with {
    registerType(User)

    query('user') {
        returnType User
        arg 'id', 'Int'
        description 'Get user by ID'
    }

    exportSchemaToFile('schema.json')
}

println '✓ Schema exported!'
```text
<!-- Code example in TEXT -->

Export and deploy:

```bash
<!-- Code example in BASH -->
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```text
<!-- Code example in TEXT -->

---

## Quick Reference Table

| Feature | Syntax | Purpose | Returns |
|---------|--------|---------|---------|
| **Type Definition** | `@GraphQLType class T { }` | GraphQL object type | Type registry |
| **Field Definition** | `@GraphQLField Type field` | Type field | Schema field |
| **Dynamic Properties** | `t.dynamicField = value` | Runtime property injection | Any type |
| **Query Operation** | `query('name') { }` | Read operation (SELECT) | QueryBuilder |
| **Mutation Operation** | `mutation('name') { }` | Write operation (INSERT/UPDATE) | MutationBuilder |
| **Fact Table** | `@FactTable class T { }` | Analytics table (OLAP) | Fact table info |
| **RBAC/Security** | `@Secured(roles=[...])` | Access control | Auth metadata |
| **Closures** | `{ -> code }` | Lazy code blocks, builders | Closure result |
| **DSL Builder** | `schema { /* DSL */ }` | Fluent domain-specific language | Schema object |
| **Metaprogramming** | `T.metaClass` | Runtime class modification | MetaClass |

---

## Type System

### 1. @GraphQLType Annotation

Marks a Groovy class as a GraphQL type. Classes can be plain Groovy classes, with computed properties, and dynamic fields.

```groovy
<!-- Code example in GROOVY -->
// Simple type
@GraphQLType
class User {
    @GraphQLField int id
    @GraphQLField String name
    @GraphQLField(nullable = true) String email
}

// Type with computed property
@GraphQLType
class UserProfile {
    @GraphQLField int id
    @GraphQLField String firstName
    @GraphQLField String lastName

    @GraphQLField
    String getFullName() {
        "$firstName $lastName"
    }
}

// Type with dynamic properties
@GraphQLType
class DynamicEntity {
    @GraphQLField int id

    // Added at runtime via metaprogramming
}

// Mixin pattern (Groovy-specific)
@GraphQLType
class EnhancedUser extends User {
    @GraphQLField LocalDateTime createdAt
}
```text
<!-- Code example in TEXT -->

**Attributes:**

- None (annotation applies to class level only)

**Best Practices:**

- Use dynamic properties via metaprogramming for flexible schemas
- Leverage Groovy's duck typing for flexibility
- Use computed properties (getters) for derived fields
- Combine with Spring `@Component` for framework integration
- Avoid excessive dynamic metaprogramming in performance-critical paths

### 2. @GraphQLField Annotation

Marks a field or getter as part of GraphQL type. Supports nullability, custom names, and descriptions.

```groovy
<!-- Code example in GROOVY -->
// Simple field
@GraphQLField int id

// Nullable field
@GraphQLField(nullable = true) String bio

// Computed field (getter method)
@GraphQLField
String getDisplayName() {
    "${firstName} ${lastName}"
}

// Field with custom name and description
@GraphQLField(name = 'userName', description = 'Login name')
String username

// Closure-based computed field (Groovy DSL pattern)
@GraphQLField
Closure getMetadata() {
    { -> ['created': createdAt, 'modified': updatedAt] }
}
```text
<!-- Code example in TEXT -->

**Attributes:**

- `nullable` (boolean, default: `false`) - Whether field can be null
- `name` (String) - Custom field name in schema (defaults to field name)
- `type` (String) - Custom GraphQL type (auto-detected if omitted)
- `description` (String) - Field documentation

**Groovy Type Detection:**

| Groovy Type | GraphQL Type | Example |
|-------------|--------------|---------|
| `int`, `long`, `short`, `byte` | `Int!` | `@GraphQLField int id` |
| `Integer`, `Long` | `Int` | `@GraphQLField Integer count` |
| `String` | `String!` | `@GraphQLField String name` |
| `boolean` | `Boolean!` | `@GraphQLField boolean active` |
| `float`, `double` | `Float!` | `@GraphQLField double price` |
| `BigDecimal` | `Decimal!` | `@GraphQLField BigDecimal money` |
| `LocalDate` | `String!` | `@GraphQLField LocalDate created` |
| `LocalDateTime` | `String!` | `@GraphQLField LocalDateTime updated` |
| `List<T>` | `[T]!` | `@GraphQLField List<String> tags` |
| `T[]` | `[T]!` | `@GraphQLField String[] items` |
| `Map` | `JSON!` | `@GraphQLField Map metadata` |
| `Closure` | Varies | `@GraphQLField Closure handler` |

### 3. Dynamic Properties & Metaprogramming

Groovy's metaprogramming allows runtime type enhancement:

```groovy
<!-- Code example in GROOVY -->
// Add properties dynamically at compile time
@GraphQLType
class Article {
    @GraphQLField int id
    @GraphQLField String title
}

// Add properties at runtime
Article.metaClass.authorId = null
Article.metaClass.getAuthorName = {
    "Author ${delegate.authorId}"
}

// Groovy Expandos (dynamic objects)
@GraphQLType
class DynamicRecord {
    @GraphQLField int id

    // Properties can be added at runtime
}

def record = new DynamicRecord(id: 1)
record.dynamicField = 'value'
record.metadata = [key: 'val']

// Using propertyMissing for computed fields
class SmartEntity {
    def propertyMissing(String name, value) {
        if (name.startsWith('get')) {
            return computeProperty(name)
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## Operations

### Query Operations (Closures & DSL)

Queries are read-only operations. Groovy's closure syntax makes builders fluent and expressive:

```groovy
<!-- Code example in GROOVY -->
// Simple query (closure-based DSL)
FraiseQL.query('user') {
    returnType User
    arg 'id', 'Int'
    description 'Get user by ID'
}

// Query returning array
FraiseQL.query('users') {
    returnType User
    returnsArray true
    arg 'limit', 'Int'
    arg 'offset', 'Int'
}

// Query with optional arguments using Groovy's named parameters
FraiseQL.query('search') {
    returnType Post
    returnsArray true

    // Groovy spread syntax for multiple args
    [
        'query': 'String',
        'limit': 'Int',
        'offset': 'Int',
        'filter': 'String'
    ].each { name, type ->
        arg name, type
    }
}

// Query with GString interpolation
String queryName = 'getUserById'
FraiseQL.query(queryName) {
    returnType User
    arg 'id', 'Int'
    description "Query to ${queryName.camelCase()}"
}

// Chaining multiple queries with Groovy closures
['user', 'users', 'userCount'].each { name ->
    FraiseQL.query(name) {
        returnType User
        description "Query for $name"
    }
}
```text
<!-- Code example in TEXT -->

**QueryBuilder Closure Parameters:**

- `returnType(Class)` - Set return type (required)
- `returnsArray(boolean)` - Whether result is array (default: false)
- `arg(String name, String type)` - Add argument (repeatable)
- `description(String)` - Add documentation

### Mutation Operations (Write Operations)

Mutations modify data using the same closure DSL:

```groovy
<!-- Code example in GROOVY -->
// Create mutation
FraiseQL.mutation('createUser') {
    returnType User
    arg 'email', 'String'
    arg 'name', 'String'
    description 'Create new user'
}

// Update mutation
FraiseQL.mutation('updateUser') {
    returnType User
    arg 'id', 'Int'
    arg 'name', 'String'
    arg 'email', 'String'
}

// Delete mutation
FraiseQL.mutation('deleteUser') {
    returnType User
    arg 'id', 'Int'
}

// Batch mutation with Groovy iteration
['create', 'update', 'delete'].each { operation ->
    FraiseQL.mutation("${operation}Article") {
        returnType Article
        description "Batch ${operation} operation"
    }
}

// Conditional registration with Groovy control flow
if (environment == 'production') {
    FraiseQL.mutation('dangerousDelete') {
        returnType User
        arg 'id', 'Int'
        description 'Admin only'
    }
}
```text
<!-- Code example in TEXT -->

### Subscription Operations (Real-time)

Subscriptions enable real-time event streaming via WebSocket:

```groovy
<!-- Code example in GROOVY -->
// Topic-based subscription
FraiseQL.subscription('userCreated') {
    returnType User
    description 'Subscribe to user creation events'
}

// Parameterized subscription with Groovy closures
FraiseQL.subscription('postUpdated') {
    returnType Post
    arg 'userId', 'Int'

    // Closure for dynamic subscription logic
    def handler = {
        println "Post updated for user: $userId"
    }
}

// Multi-topic subscription
FraiseQL.subscription('eventStream') {
    returnType Event
    arg 'topic', 'String'
    arg 'filter', 'String'
}
```text
<!-- Code example in TEXT -->

---

## Advanced Features

### Fact Tables (OLAP/Analytics)

Define analytical tables using closures for configuration:

```groovy
<!-- Code example in GROOVY -->
@FactTable(name = 'sales_fact', sqlSource = 'fact_sales')
class SalesFact {
    @GraphQLField int dateKey
    @GraphQLField int productKey
    @GraphQLField int storeKey
    @GraphQLField double revenue
    @GraphQLField int quantity
    @GraphQLField double cost
}

FraiseQL.factTable(SalesFact) {
    dimension 'dateKey', 'Date Dimension'
    dimension 'productKey', 'Product Dimension'
    dimension 'storeKey', 'Store Dimension'

    measure 'revenue', 'SUM', 'Total Revenue'
    measure 'quantity', 'SUM', 'Total Quantity'
    measure 'cost', 'SUM', 'Total Cost'
}

// Aggregate query using DSL
FraiseQL.query('salesByProduct') {
    returnType SalesFact
    returnsArray true
    arg 'productKey', 'Int'
    arg 'dateRange', 'String'
    description 'Sales aggregation by product'
}
```text
<!-- Code example in TEXT -->

### RBAC & Security Annotations

Define security with Groovy's expressive syntax:

```groovy
<!-- Code example in GROOVY -->
@Secured(roles = ['ADMIN'])
@GraphQLType
class AdminPanel {
    @GraphQLField String systemHealth
    @GraphQLField int totalUsers
}

// Conditional security with Groovy control flow
def securityRoles = environment == 'prod' ? ['ADMIN'] : ['ADMIN', 'DEV']

FraiseQL.query('metrics') {
    returnType AdminPanel
    description 'System metrics (restricted)'
}

// Field-level security
@GraphQLType
class Account {
    @GraphQLField int id
    @GraphQLField String accountNumber

    @GraphQLField
    double getBalance() { balance }

    @Secured(roles = ['OWNER', 'ADMIN'])
    @GraphQLField
    String getAccountSsn() { ssn }
}

// Groovy closure for custom authorization logic
FraiseQL.query('sensitiveData') {
    returnType Account
    authorize { user ->
        user.hasRole('ADMIN') || user.isOwner()
    }
}
```text
<!-- Code example in TEXT -->

### Custom Directives

Define schema directives using Groovy classes:

```groovy
<!-- Code example in GROOVY -->
@Directive(name = 'auth', description = 'Requires authentication')
class AuthDirective {
    String roles
}

@Directive(name = 'cache', description = 'Cache directive')
class CacheDirective {
    int ttl
    String scope
}

// Register directives via closure
FraiseQL.directive('auth') {
    description 'Requires authentication'
    arg 'roles', '[String]'
}

FraiseQL.directive('rateLimit') {
    description 'Rate limiting directive'
    arg 'requests', 'Int'
    arg 'window', 'Int'
}
```text
<!-- Code example in TEXT -->

### Field Observers & Event Webhooks

Define webhook observers using Groovy classes:

```groovy
<!-- Code example in GROOVY -->
@Observer(
    name = 'onUserCreated',
    webhook = 'https://api.example.com/webhooks/user-created'
)
class UserCreatedObserver {
    int userId
    String email
    String name
}

// Groovy builder pattern for observer registration
FraiseQL.observer('userDeleted') {
    webhook 'https://api.example.com/webhooks/user-deleted'

    // Closure for event mapping
    transform { event ->
        [userId: event.id, timestamp: System.currentTimeMillis()]
    }

    retry {
        attempts 3
        backoff 'exponential'
    }
}

// Multi-observer with Groovy iteration
['created', 'updated', 'deleted'].each { action ->
    FraiseQL.observer("user${action.capitalize()}") {
        webhook "https://api.example.com/webhooks/user-${action}"
    }
}
```text
<!-- Code example in TEXT -->

---

## Scalar Types Reference

Groovy's dynamic typing combined with Java scalar support:

```groovy
<!-- Code example in GROOVY -->
// String types
@GraphQLField String name              // String!
@GraphQLField String description       // String!
@GraphQLField char letter              // String!

// Groovy GString with interpolation
@GraphQLField
String getLabel() {
    "User: $name (ID: $id)"
}

// Numeric types
@GraphQLField int count                // Int!
@GraphQLField long bigNumber           // Int!
@GraphQLField float rating             // Float!
@GraphQLField double price             // Float!
@GraphQLField BigDecimal money          // Decimal!
@GraphQLField BigInteger huge          // BigInt!

// Boolean type
@GraphQLField boolean active           // Boolean!

// Date/Time types
@GraphQLField LocalDate date           // String! (ISO 8601)
@GraphQLField LocalDateTime datetime   // String! (ISO 8601)
@GraphQLField LocalTime time           // String! (ISO 8601)
@GraphQLField ZonedDateTime zdt        // String! (ISO 8601)
@GraphQLField Instant instant          // String! (ISO 8601)

// ID type
@GraphQLField UUID uuid                // ID!
@GraphQLField String id                // ID!

// Collections
@GraphQLField List<String> tags        // [String]!
@GraphQLField String[] items           // [String]!
@GraphQLField Set<Integer> numbers     // [Int]!

// Groovy closures as first-class values
@GraphQLField Closure<String> formatter // Function type
@GraphQLField Map metadata             // JSON!
```text
<!-- Code example in TEXT -->

---

## Schema Export

### Export Workflow

```groovy
<!-- Code example in GROOVY -->
import com.FraiseQL.*

// 1. Define types
@GraphQLType
class User {
    @GraphQLField int id
    @GraphQLField String name
}

@GraphQLType
class Post {
    @GraphQLField int id
    @GraphQLField String title
    @GraphQLField User author
}

// 2. Register types and operations
FraiseQL.with {
    registerType(User)
    registerType(Post)

    // Queries
    query('user') {
        returnType User
        arg 'id', 'Int'
    }

    query('posts') {
        returnType Post
        returnsArray true
    }

    // Mutations
    mutation('createPost') {
        returnType Post
        arg 'title', 'String'
        arg 'content', 'String'
    }

    // 3. Export schema
    exportSchemaToFile('schema.json')
}

println '✓ Schema exported successfully'
```text
<!-- Code example in TEXT -->

### Schema Validation

```groovy
<!-- Code example in GROOVY -->
// Validate schema after registration
SchemaRegistry registry = SchemaRegistry.getInstance()
SchemaValidator.ValidationResult result = SchemaValidator.validate(registry)

if (result.valid) {
    println '✓ Schema is valid!'
    println SchemaValidator.getStatistics(registry)
} else {
    println '✗ Schema has errors:'
    result.errors.each { error ->
        System.err.println "  ERROR: $error"
    }

    if (!result.warnings.isEmpty()) {
        println 'Warnings:'
        result.warnings.each { warning ->
            println "  WARN: $warning"
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## Type Mapping Reference

Complete Groovy ↔ GraphQL type mappings:

| Category | Groovy Type | GraphQL Type | Example |
|----------|-------------|--------------|---------|
| **String** | `String` | `String!` | `"Hello"` |
| **Integer** | `int` | `Int!` | `42` |
| **Integer** | `Integer` | `Int` | `null` |
| **Long** | `long` | `Int!` | `999999L` |
| **Float** | `float` | `Float!` | `3.14f` |
| **Float** | `double` | `Float!` | `3.14159` |
| **Decimal** | `BigDecimal` | `Decimal!` | `123.45.bd` |
| **Boolean** | `boolean` | `Boolean!` | `true` |
| **List** | `List<T>` | `[T]!` | `[1, 2, 3]` |
| **Array** | `T[]` | `[T]!` | `[1, 2, 3] as int[]` |
| **UUID** | `UUID` | `ID!` | `UUID.randomUUID()` |
| **Date** | `LocalDate` | `String!` | `LocalDate.now()` |
| **DateTime** | `LocalDateTime` | `String!` | `LocalDateTime.now()` |
| **Map** | `Map` | `JSON!` | `[key: 'value']` |

---

## Common Patterns

### CRUD with Closure DSL

```groovy
<!-- Code example in GROOVY -->
@GraphQLType
class Article {
    @GraphQLField int id
    @GraphQLField String title
    @GraphQLField String content
    @GraphQLField LocalDateTime createdAt
    @GraphQLField LocalDateTime updatedAt
}

FraiseQL.with {
    registerType(Article)

    // CREATE
    mutation('createArticle') {
        returnType Article
        arg 'title', 'String'
        arg 'content', 'String'
    }

    // READ one
    query('article') {
        returnType Article
        arg 'id', 'Int'
    }

    // READ many
    query('articles') {
        returnType Article
        returnsArray true
        arg 'limit', 'Int'
        arg 'offset', 'Int'
    }

    // UPDATE
    mutation('updateArticle') {
        returnType Article
        arg 'id', 'Int'
        arg 'title', 'String'
        arg 'content', 'String'
    }

    // DELETE
    mutation('deleteArticle') {
        returnType Article
        arg 'id', 'Int'
    }
}
```text
<!-- Code example in TEXT -->

### Pagination Pattern

```groovy
<!-- Code example in GROOVY -->
@GraphQLType
class UserConnection {
    @GraphQLField List<User> edges
    @GraphQLField int totalCount
    @GraphQLField boolean hasNextPage
    @GraphQLField String endCursor
}

FraiseQL.query('users') {
    returnType UserConnection
    arg 'first', 'Int'          // Limit
    arg 'after', 'String'       // Cursor
    arg 'sort', 'String'        // Sort field
    description 'Paginated user list'
}
```text
<!-- Code example in TEXT -->

### Filtering & Search with Groovy

```groovy
<!-- Code example in GROOVY -->
// Build search query dynamically
def buildSearchQuery(String filter, List<String> fields) {
    def query = FraiseQL.query('search') {
        returnType User
        returnsArray true

        fields.each { field ->
            arg field, 'String'
        }
    }

    return query
}

// Use with Groovy closures
def filters = ['name', 'email', 'status']
buildSearchQuery('users', filters)

// String interpolation in descriptions
FraiseQL.query('searchUsers') {
    returnType User
    returnsArray true
    arg 'filter', 'String'
    description "Search users by filter (supports: ${filters.join(', ')})"
}
```text
<!-- Code example in TEXT -->

### Spring Boot Integration

```groovy
<!-- Code example in GROOVY -->
import org.springframework.context.annotation.Configuration
import org.springframework.context.annotation.Bean
import org.springframework.boot.CommandLineRunner

@Configuration
class FraiseQLConfig {

    @Bean
    CommandLineRunner schemaBuilder() {
        return { args ->
            FraiseQL.with {
                registerTypes(User, Post, Comment)

                // Queries
                query('user') {
                    returnType User
                    arg 'id', 'Int'
                }

                // Mutations
                mutation('createPost') {
                    returnType Post
                    arg 'title', 'String'
                    arg 'content', 'String'
                }

                // Validate and export
                SchemaRegistry registry = SchemaRegistry.getInstance()
                SchemaValidator.ValidationResult result =
                    SchemaValidator.validate(registry)

                if (result.valid) {
                    exportSchemaToFile('schema.json')
                    println '✓ Schema validated and exported'
                } else {
                    throw new RuntimeException(
                        "Schema validation failed: ${result.errors.join(', ')}"
                    )
                }
            }
        }
    }
}
```text
<!-- Code example in TEXT -->

---

## Error Handling

### Validation Error Handling

```groovy
<!-- Code example in GROOVY -->
import com.FraiseQL.error.*

try {
    SchemaRegistry registry = SchemaRegistry.getInstance()
    SchemaValidator.ValidationResult result =
        SchemaValidator.validate(registry)

    if (!result.valid) {
        throw new SchemaValidationException(
            "Schema validation failed: ${result.errors.size()} error(s)"
        )
    }

    FraiseQL.exportSchemaToFile('schema.json')

} catch (SchemaValidationException e) {
    System.err.println "Validation error: ${e.message}"
    result.errors.each { System.err.println "  - $it" }
    System.exit(1)

} catch (IOException e) {
    System.err.println "File I/O error: ${e.message}"
    System.exit(1)
}
```text
<!-- Code example in TEXT -->

### Custom Exception Handling

```groovy
<!-- Code example in GROOVY -->
class FraiseQLException extends RuntimeException {
    FraiseQLException(String message) {
        super(message)
    }

    FraiseQLException(String message, Throwable cause) {
        super(message, cause)
    }
}

class SchemaValidationException extends FraiseQLException {
    List<String> validationErrors = []

    SchemaValidationException(String message, List<String> errors) {
        super(message)
        this.validationErrors = errors
    }
}
```text
<!-- Code example in TEXT -->

---

## Testing

### Spock Framework Pattern

Groovy's Spock framework provides BDD-style testing:

```groovy
<!-- Code example in GROOVY -->
import spock.lang.Specification

class SchemaTest extends Specification {

    def setup() {
        FraiseQL.clear()
        SchemaCache.getInstance().clear()
    }

    def 'User type should be registered'() {
        when:
        FraiseQL.registerType(User)

        then:
        SchemaRegistry.getInstance().getType('User').isPresent()
    }

    def 'Query registration should work'() {
        when:
        FraiseQL.registerType(User)
        FraiseQL.query('user') {
            returnType User
            arg 'id', 'Int'
        }

        then:
        SchemaRegistry.getInstance().getQuery('user').isPresent()
    }

    def 'Schema validation should pass for valid schema'() {
        when:
        FraiseQL.registerTypes(User, Post)
        FraiseQL.query('user').returnType(User).arg('id', 'Int')

        then:
        SchemaValidator.validate(SchemaRegistry.getInstance()).valid
    }

    def 'Multiple types can be registered via closure'() {
        when:
        FraiseQL.with {
            registerTypes(User, Post, Comment)

            query('users').returnType(User).returnsArray(true)
            query('posts').returnType(Post).returnsArray(true)
        }

        then:
        SchemaRegistry.getInstance().getQuery('users').isPresent()
        SchemaRegistry.getInstance().getQuery('posts').isPresent()
    }
}
```text
<!-- Code example in TEXT -->

### JUnit 5 with Groovy

```groovy
<!-- Code example in GROOVY -->
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import static org.junit.jupiter.api.Assertions.*

class SchemaJunitTest {

    @BeforeEach
    void setUp() {
        FraiseQL.clear()
    }

    @Test
    void testBasicSchemaExport() {
        FraiseQL.registerType(User)
        String schema = FraiseQL.exportSchema()

        assertNotNull(schema)
        assertTrue(schema.contains('User'))
    }

    @Test
    void testDynamicPropertyRegistration() {
        // Groovy allows runtime property addition
        User.metaClass.dynamicField = 'test'

        User user = new User(id: 1, name: 'Alice')
        assertEquals('test', user.dynamicField)
    }
}
```text
<!-- Code example in TEXT -->

---

## See Also

- [API Guide](../../reference/README.md) - Detailed API reference
- [Java SDK Reference](./java-reference.md) - Java SDK documentation
- [Python SDK Reference](./python-reference.md) - Python SDK documentation
- [TypeScript SDK Reference](./typescript-reference.md) - TypeScript SDK documentation
- [Security & RBAC Guide](../../guides/authorization-quick-start.md) - Authorization patterns
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md) - Fact tables and aggregations
- [GraphQL Scalar Types](../../reference/scalars.md) - Complete scalar type reference
- [Architecture Principles](../../architecture/README.md) - System design

---

## Troubleshooting

### Common Setup Issues

#### Gradle Issues

**Issue**: `Could not find com.FraiseQL:FraiseQL-groovy:2.0.0`

**Solution**:

```gradle
<!-- Code example in GRADLE -->
repositories {
    mavenCentral()
}

dependencies {
    implementation 'com.FraiseQL:FraiseQL-groovy:2.0.0'
}
```text
<!-- Code example in TEXT -->

#### Groovy Compilation

**Issue**: `Cannot find groovy-all jar`

**Solution - Add Groovy dependency**:

```gradle
<!-- Code example in GRADLE -->
dependencies {
    implementation 'org.apache.groovy:groovy-all:4.0.0'
    implementation 'com.FraiseQL:FraiseQL-groovy:2.0.0'
}
```text
<!-- Code example in TEXT -->

#### Java Interop Issues

**Issue**: `Cannot find Java class in Groovy`

**Solution - Set up classpath**:

```gradle
<!-- Code example in GRADLE -->
sourceSets {
    main {
        java {
            srcDir 'src/main/java'
        }
        groovy {
            srcDir 'src/main/groovy'
        }
    }
}
```text
<!-- Code example in TEXT -->

#### Dynamic Method Issues

**Issue**: `MissingMethodException`

**Solution - Define methods properly**:

```groovy
<!-- Code example in GROOVY -->
// ✅ Correct
class MyClass {
    def execute(String query) {
        return FraiseQL.execute(query)
    }
}

// ✅ Or with type
String execute(String query) {
    return FraiseQL.execute(query)
}
```text
<!-- Code example in TEXT -->

---

### Type System Issues

#### Closure Issues

**Issue**: `Invalid variable reference`

**Solution - Proper closure scope**:

```groovy
<!-- Code example in GROOVY -->
// ✅ Correct
def process = { result ->
    println result
}

FraiseQL.execute(query).each(process)
```text
<!-- Code example in TEXT -->

#### Metaclass Issues

**Issue**: `GroovyRuntimeException: Could not find matching method`

**Solution - Check metaclass**:

```groovy
<!-- Code example in GROOVY -->
// ✅ Add method dynamically
String.metaClass.queryify = { ->
    "query { $delegate { id } }"
}

def query = 'user'.queryify()
```text
<!-- Code example in TEXT -->

#### Type Coercion Issues

**Issue**: `ClassCastException` at runtime

**Solution - Explicit casting**:

```groovy
<!-- Code example in GROOVY -->
// ✅ Explicit cast
def id = (int) request.params.id
def result = FraiseQL.execute(query, [id: id])
```text
<!-- Code example in TEXT -->

---

### Runtime Errors

#### Dynamic Method Errors

**Issue**: `MissingMethodException: No signature of method`

**Solution - Define method first**:

```groovy
<!-- Code example in GROOVY -->
// ✅ Define before using
FraiseQL.metaClass.executeWithRetry = { String query ->
    FraiseQL.execute(query)
}

FraiseQL.executeWithRetry(query)
```text
<!-- Code example in TEXT -->

#### Closure Context Issues

**Issue**: `NullPointerException in closure`

**Solution - Capture variables**:

```groovy
<!-- Code example in GROOVY -->
// ❌ Wrong
def users = []
queries.each { q ->
    users << FraiseQL.execute(q)  // Context lost
}

// ✅ Correct
def users = []
def f = FraiseQL  // Capture reference
queries.each { q ->
    users << f.execute(q)
}
```text
<!-- Code example in TEXT -->

#### Thread Issues in Groovy

**Issue**: `ConcurrentModificationException`

**Solution - Synchronize if needed**:

```groovy
<!-- Code example in GROOVY -->
def results = Collections.synchronizedList([])

queries.each { q ->
    results << FraiseQL.execute(q)
}
```text
<!-- Code example in TEXT -->

---

### Performance Issues

#### Build Time

**Issue**: Build takes >60 seconds

**Parallel compilation**:

```gradle
<!-- Code example in GRADLE -->
tasks.withType(JavaCompile).all { task ->
    task.options.fork = true
    task.options.forkOptions.memoryInitialSize = '512m'
    task.options.forkOptions.memoryMaximumSize = '1g'
}
```text
<!-- Code example in TEXT -->

#### Dynamic Dispatch Overhead

**Issue**: Slow method calls due to dynamic dispatch

**Use static type hints**:

```groovy
<!-- Code example in GROOVY -->
// ❌ Slower - dynamic
def result = FraiseQL.execute(query)

// ✅ Faster - static
FraiseQLResult result = FraiseQL.execute(query)
```text
<!-- Code example in TEXT -->

---

### Debugging Techniques

#### Groovy Console

```bash
<!-- Code example in BASH -->
groovysh

groovy> import com.FraiseQL.*
groovy> def server = Server.fromCompiled('schema.json')
groovy> server.execute('{ user(id: 1) { id } }')
```text
<!-- Code example in TEXT -->

#### Print Debugging

```groovy
<!-- Code example in GROOVY -->
def result = FraiseQL.execute(query)
println "Result: ${result}"
println "Result: ${result.dump()}"  // Detailed dump
```text
<!-- Code example in TEXT -->

#### Property Access

```groovy
<!-- Code example in GROOVY -->
// ✅ Check properties
def user = result.data.user
println user?.id  // Safe navigation
println user.properties  // All properties
```text
<!-- Code example in TEXT -->

---

### Getting Help

Provide:

1. Java version: `java -version`
2. Groovy version: `groovy --version`
3. Gradle version: `gradle --version`
4. FraiseQL version
5. Error message

---

**Status**: Production Ready ✅ | **Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community
