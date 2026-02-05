# FraiseQL Java SDK Reference

**Status**: Production-Ready | **Java Version**: 11+ (Records: 16+) | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Java SDK. This guide covers the complete Java authoring interface for building type-safe GraphQL APIs with Java annotations, builder patterns, and modern language features.

## Installation & Setup

### Maven

```xml
<dependency>
    <groupId>com.FraiseQL</groupId>
    <artifactId>FraiseQL-SDK</artifactId>
    <version>2.0.0</version>
</dependency>

<!-- Annotation processor (required for schema compilation) -->
<dependency>
    <groupId>com.FraiseQL</groupId>
    <artifactId>FraiseQL-processor</artifactId>
    <version>2.0.0</version>
    <scope>provided</scope>
</dependency>
```

Configure annotation processor in `pom.xml`:

```xml
<build>
    <plugins>
        <plugin>
            <groupId>org.apache.maven.plugins</groupId>
            <artifactId>maven-compiler-plugin</artifactId>
            <version>3.11.0</version>
            <configuration>
                <source>11</source>
                <target>11</target>
                <annotationProcessorPaths>
                    <path>
                        <groupId>com.FraiseQL</groupId>
                        <artifactId>FraiseQL-processor</artifactId>
                        <version>2.0.0</version>
                    </path>
                </annotationProcessorPaths>
            </configuration>
        </plugin>
    </plugins>
</build>
```

### Gradle

```gradle
dependencies {
    implementation 'com.FraiseQL:FraiseQL-SDK:2.0.0'
    annotationProcessor 'com.FraiseQL:FraiseQL-processor:2.0.0'
}

tasks.withType(JavaCompile) {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}
```

### Requirements

- **Java 11+** (Full support, all features)
- **Java 16+** (Records support)
- **Java 21+** (Virtual threads, pattern matching)
- Maven 3.8+ or Gradle 7.0+
- Spring Boot 3.0+ (optional, for Spring integration)

### First Schema (60 seconds)

```java
import com.FraiseQL.*;

@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField(nullable = true)
    public String email;
}

public class Main {
    public static void main(String[] args) {
        FraiseQL.registerType(User.class);

        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        FraiseQL.exportSchemaToFile("schema.json");
        System.out.println("Schema exported!");
    }
}
```

Export and deploy to your FraiseQL server:

```bash
FraiseQL-cli compile schema.json FraiseQL.toml
FraiseQL-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Annotation | Purpose | Builder Method | Returns |
|---------|-----------|---------|---|---|
| **Type Definition** | `@GraphQLType` | GraphQL object type | `registerType(Class<?>)` | Type info |
| **Field Definition** | `@GraphQLField` | Type field | N/A | Schema field |
| **Query Operation** | N/A | Read operation | `query(String name)` | QueryBuilder |
| **Mutation Operation** | N/A | Write operation | `mutation(String name)` | MutationBuilder |
| **Subscription** | N/A | Real-time stream | `subscription(String name)` | SubscriptionBuilder |
| **Fact Table** | `@FactTable` | Analytics table | `factTable(Class<?>)` | Fact table info |
| **Security/RBAC** | `@Secured` | Access control | `securedQuery(String)` | Secured builder |
| **Custom Directive** | `@Directive` | Schema directive | `directive(String name)` | Directive info |
| **Field Observer** | `@Observer` | Event webhook | `observer(String name)` | Observer info |

---

## Type System

### 1. @GraphQLType Annotation

Marks a class as a GraphQL type definition. Classes can be POJOs, records, or regular classes.

```java
// Standard POJO
@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField(nullable = true)
    public String email;
}

// Java 16+ Record (recommended for immutability)
@GraphQLType
public record Product(
    @GraphQLField int id,
    @GraphQLField String name,
    @GraphQLField double price
) {}

// With constructor
@GraphQLType
public class Order {
    private int id;
    private String status;

    public Order(int id, String status) {
        this.id = id;
        this.status = status;
    }

    @GraphQLField
    public int getId() { return id; }

    @GraphQLField
    public String getStatus() { return status; }
}
```

### Attributes

- None (annotation applies to class level only)

### Best Practices

- Use immutable records when possible (Java 16+)
- Keep types flat (no nested `@GraphQLType` annotations)
- Use getters for computed fields
- Combine with Spring `@Component` for Bean management

### 2. @GraphQLField Annotation

Marks a field or getter as part of a GraphQL type. Supports nullability, custom names, and type overrides.

```java
@GraphQLField
public String name;

@GraphQLField(nullable = true)
public String email;

@GraphQLField(name = "userName", description = "User login name")
public String username;

@GraphQLField(type = "ID", description = "Unique user identifier")
public String userId;

@GraphQLField(name = "tags", type = "[String]!")
public List<String> getTags() { return tags; }
```

### Attributes

- `nullable` (boolean, default: `false`) - Whether field can be null
- `name` (String) - Custom field name in schema (defaults to Java name)
- `type` (String) - Custom GraphQL type (auto-detected if omitted)
- `description` (String) - Field documentation for schema

### Type Detection Rules

| Java Type | GraphQL Type | Nullable | Example |
|-----------|--------------|----------|---------|
| `int`, `long`, `short`, `byte` | `Int!` | No | `@GraphQLField public int id;` |
| `Integer`, `Long` | `Int` | Yes | `@GraphQLField Integer count;` |
| `String` | `String!` | No | `@GraphQLField String name;` |
| `boolean` | `Boolean!` | No | `@GraphQLField boolean active;` |
| `float`, `double` | `Float!` | No | `@GraphQLField double price;` |
| `LocalDate` | `String!` | No | `@GraphQLField LocalDate created;` |
| `LocalDateTime` | `String!` | No | `@GraphQLField LocalDateTime updated;` |
| `UUID` | `ID!` | No | `@GraphQLField UUID uuid;` |
| `T[]` | `[T]!` | No | `@GraphQLField String[] tags;` |
| `List<T>` | `[T]!` | No | `@GraphQLField List<String> tags;` |
| `T[]` (with `nullable=true`) | `[T]` | Yes | `@GraphQLField(nullable=true) String[] tags;` |

### 3. Generics and Complex Types

```java
@GraphQLType
public class Page<T> {
    @GraphQLField
    public List<T> items;

    @GraphQLField
    public int totalCount;

    @GraphQLField
    public int pageNumber;
}

@GraphQLType
public class UserPage {
    @GraphQLField
    public List<User> items;

    @GraphQLField
    public int totalCount;
}

// Nested types
@GraphQLType
public class Post {
    @GraphQLField
    public int id;

    @GraphQLField(type = "User!")
    public User author;

    @GraphQLField(type = "[Comment]!")
    public List<Comment> comments;
}
```

---

## Operations

### Query Operations

Queries are read-only operations that fetch data. Use the fluent builder pattern:

```java
// Simple query
FraiseQL.query("user")
    .returnType(User.class)
    .arg("id", "Int")
    .description("Get a user by ID")
    .register();

// Query returning array
FraiseQL.query("users")
    .returnType(User.class)
    .returnsArray(true)
    .arg("limit", "Int")
    .arg("offset", "Int")
    .description("Get paginated users")
    .register();

// Query with optional arguments using ArgumentBuilder
ArgumentBuilder args = new ArgumentBuilder()
    .add("limit", "Int", 10, "Maximum results")
    .add("offset", "Int", 0, "Pagination offset")
    .add("filter", "String", null, "Search filter");

FraiseQL.query("search")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("limit", "Int")
    .arg("offset", "Int")
    .arg("filter", "String")
    .description("Search posts with optional filter")
    .register();

// Query with custom return type
FraiseQL.query("userCount")
    .returnType(String.class)  // Custom scalar return
    .description("Get total user count")
    .register();
```

### QueryBuilder Methods

- `returnType(Class<?>)` - Set return type (required)
- `returnsArray(boolean)` - Whether result is array (default: false)
- `arg(String name, String type)` - Add argument (repeatable)
- `description(String)` - Add documentation
- `register()` - Register the query (required, must call last)

### Mutation Operations

Mutations are write operations that modify data (INSERT, UPDATE, DELETE).

```java
// Create mutation
FraiseQL.mutation("createUser")
    .returnType(User.class)
    .arg("email", "String")
    .arg("name", "String")
    .description("Create a new user")
    .register();

// Update mutation
FraiseQL.mutation("updateUser")
    .returnType(User.class)
    .arg("id", "Int")
    .arg("name", "String")
    .arg("email", "String")
    .description("Update user by ID")
    .register();

// Delete mutation
FraiseQL.mutation("deleteUser")
    .returnType(User.class)
    .arg("id", "Int")
    .description("Delete user by ID")
    .register();

// Batch mutation
FraiseQL.mutation("bulkDeleteUsers")
    .returnType(User.class)
    .returnsArray(true)
    .arg("ids", "Int")  // Note: Repeated for array args
    .description("Delete multiple users")
    .register();
```

### MutationBuilder Methods

Identical to QueryBuilder interface.

### Subscription Operations (Real-time)

Subscriptions enable real-time event streaming via WebSocket.

```java
FraiseQL.subscription("userCreated")
    .returnType(User.class)
    .description("Subscribe to new user creation events")
    .register();

FraiseQL.subscription("postUpdated")
    .returnType(Post.class)
    .arg("userId", "Int")
    .description("Subscribe to post updates for specific user")
    .register();

// Topic-based subscription
FraiseQL.subscription("orderStatus")
    .returnType(Order.class)
    .arg("orderId", "Int")
    .arg("topic", "String")
    .description("Subscribe to order status changes")
    .register();
```

---

## Advanced Features

### Fact Tables (Analytics/OLAP)

Define analytical tables for OLAP queries with dimensions and measures.

```java
@FactTable(name = "sales_fact", sqlSource = "fact_sales")
public class SalesFact {
    @GraphQLField(name = "dateKey")
    public int dateKey;

    @GraphQLField(name = "productKey")
    public int productKey;

    @GraphQLField(name = "storeKey")
    public int storeKey;

    @GraphQLField(name = "revenue")
    public double revenue;

    @GraphQLField(name = "quantity")
    public int quantity;

    @GraphQLField(name = "cost")
    public double cost;
}

// Register fact table
FraiseQL.registerType(SalesFact.class);

// Aggregate query on fact table
FraiseQL.query("salesByProduct")
    .returnType(SalesFact.class)
    .returnsArray(true)
    .arg("productKey", "Int")
    .arg("dateRange", "String")
    .description("Total sales by product")
    .register();
```

### RBAC & Security Annotations

Define role-based access control at query/mutation level.

```java
@Secured(roles = {"ADMIN"})
public class AdminPanel {
    @GraphQLField
    public String systemHealth;
}

// Secured query - only ADMIN role
FraiseQL.query("adminMetrics")
    .returnType(AdminPanel.class)
    .description("System metrics (admin only)")
    .register();

// Secured mutation - ADMIN or MODERATOR
FraiseQL.mutation("banUser")
    .returnType(User.class)
    .arg("userId", "Int")
    .arg("reason", "String")
    .description("Ban user from system")
    .register();

// Field-level security
@GraphQLType
public class Account {
    @GraphQLField
    public int id;

    @GraphQLField
    public String accountNumber;

    @GraphQLField(name = "balance", description = "Account balance")
    public double getBalance() { return balance; }

    // Sensitive field - only visible to OWNER or ADMIN
    @Secured(roles = {"OWNER", "ADMIN"})
    @GraphQLField
    public String accountSsn;
}
```

### Custom Directives

Define custom GraphQL directives for schema extensions.

```java
@Directive(name = "auth", description = "Requires authentication")
public class AuthDirective {
    public String roles;
}

@Directive(name = "cache", description = "Cache directive")
public class CacheDirective {
    public int ttl;
    public String scope;
}

// Use in queries
FraiseQL.query("profile")
    .returnType(User.class)
    .description("Get user profile (requires authentication)")
    .register();
```

### Field Observers (Event Webhooks)

Trigger external webhooks when fields change.

```java
@Observer(name = "onUserCreated", webhook = "https://api.example.com/webhooks/user-created")
public class UserCreatedObserver {
    public int userId;
    public String email;
    public String name;
}

@Observer(name = "onOrderShipped", webhook = "https://api.example.com/webhooks/order-shipped")
public class OrderShippedObserver {
    public int orderId;
    public String trackingNumber;
    public LocalDateTime shipDate;
}

// Register observer
FraiseQL.registerObserver(UserCreatedObserver.class);
FraiseQL.registerObserver(OrderShippedObserver.class);
```

---

## Scalar Types Reference

FraiseQL supports 60+ scalar types mapped from Java to GraphQL:

```java
// String types
@GraphQLField public String name;              // String!
@GraphQLField public String description;       // String!
@GraphQLField public char letter;              // String!

// Numeric types
@GraphQLField public int count;                // Int!
@GraphQLField public long bigNumber;           // Int!
@GraphQLField public float rating;             // Float!
@GraphQLField public double price;             // Float!
@GraphQLField public BigInteger hugeNumber;    // BigInt!
@GraphQLField public BigDecimal money;         // Decimal!

// Boolean type
@GraphQLField public boolean active;           // Boolean!

// Date/Time types
@GraphQLField public LocalDate date;           // String! (ISO 8601)
@GraphQLField public LocalDateTime datetime;   // String! (ISO 8601)
@GraphQLField public LocalTime time;           // String! (ISO 8601)
@GraphQLField public ZonedDateTime zdt;        // String! (ISO 8601)
@GraphQLField public Instant instant;          // String! (ISO 8601)

// ID type
@GraphQLField public UUID uuid;                // ID!
@GraphQLField public String id;                // ID!

// Collections
@GraphQLField public List<String> tags;        // [String]!
@GraphQLField public String[] items;           // [String]!
@GraphQLField public Set<Integer> numbers;     // [Int]!

// Optional (Nullable)
@GraphQLField(nullable = true) public String optional;
@GraphQLField public String required;          // Non-nullable
```

---

## Schema Export & Compilation

### Export Workflow

```java
import com.FraiseQL.*;

public class SchemaBuilder {
    public static void main(String[] args) throws Exception {
        // 1. Register all types
        FraiseQL.registerTypes(
            User.class,
            Post.class,
            Comment.class
        );

        // 2. Register queries
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        FraiseQL.query("posts")
            .returnType(Post.class)
            .returnsArray(true)
            .register();

        // 3. Register mutations
        FraiseQL.mutation("createPost")
            .returnType(Post.class)
            .arg("title", "String")
            .arg("content", "String")
            .register();

        // 4. Validate schema
        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

        if (!result.valid) {
            System.err.println("Schema validation failed:");
            result.errors.forEach(System.err::println);
            System.exit(1);
        }

        // 5. Export schema
        FraiseQL.exportSchemaToFile("schema.json");
        System.out.println("Schema exported: schema.json");
        System.out.println(SchemaValidator.getStatistics(registry));
    }
}
```

Run export:

```bash
# Maven
mvn clean compile exec:java -Dexec.mainClass="com.example.SchemaBuilder"

# Gradle
gradle run

# Then compile schema
FraiseQL-cli compile schema.json FraiseQL.toml
```

### Schema Validation

```java
SchemaRegistry registry = SchemaRegistry.getInstance();
SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

if (result.valid) {
    System.out.println("✓ Schema is valid!");
    System.out.println(SchemaValidator.getStatistics(registry));
} else {
    System.out.println("✗ Schema has errors:");
    result.errors.forEach(e -> System.err.println("  ERROR: " + e));

    if (!result.warnings.isEmpty()) {
        System.out.println("And warnings:");
        result.warnings.forEach(w -> System.out.println("  WARN: " + w));
    }
}
```

---

## Type Mapping Reference

Complete Java ↔ GraphQL type mappings:

| Category | Java Type | GraphQL Type | Nullable | Example |
|----------|-----------|--------------|----------|---------|
| **String** | `String` | `String!` | No | `"Hello"` |
| **Integer** | `int` | `Int!` | No | `42` |
| **Integer** | `Integer` | `Int` | Yes | `null` |
| **Long** | `long` | `Int!` | No | `999999L` |
| **Long** | `Long` | `Int` | Yes | `null` |
| **Float** | `float` | `Float!` | No | `3.14f` |
| **Float** | `double` | `Float!` | No | `3.14159` |
| **Decimal** | `BigDecimal` | `Decimal!` | No | `new BigDecimal("123.45")` |
| **Boolean** | `boolean` | `Boolean!` | No | `true` |
| **Boolean** | `Boolean` | `Boolean` | Yes | `null` |
| **List** | `List<T>` | `[T]!` | No | `List.of(1, 2, 3)` |
| **Array** | `T[]` | `[T]!` | No | `new int[]{1, 2}` |
| **UUID** | `UUID` | `ID!` | No | `UUID.randomUUID()` |
| **Date** | `LocalDate` | `String!` | No | `LocalDate.now()` |
| **DateTime** | `LocalDateTime` | `String!` | No | `LocalDateTime.now()` |
| **Instant** | `Instant` | `String!` | No | `Instant.now()` |

---

## Common Patterns

### CRUD with Builder Pattern

```java
@GraphQLType
public class Article {
    @GraphQLField public int id;
    @GraphQLField public String title;
    @GraphQLField public String content;
    @GraphQLField public LocalDateTime createdAt;
    @GraphQLField public LocalDateTime updatedAt;
}

public class ArticleSchema {
    public static void registerSchema() {
        FraiseQL.registerType(Article.class);

        // Create
        FraiseQL.mutation("createArticle")
            .returnType(Article.class)
            .arg("title", "String")
            .arg("content", "String")
            .register();

        // Read one
        FraiseQL.query("article")
            .returnType(Article.class)
            .arg("id", "Int")
            .register();

        // Read many
        FraiseQL.query("articles")
            .returnType(Article.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .arg("offset", "Int")
            .register();

        // Update
        FraiseQL.mutation("updateArticle")
            .returnType(Article.class)
            .arg("id", "Int")
            .arg("title", "String")
            .arg("content", "String")
            .register();

        // Delete
        FraiseQL.mutation("deleteArticle")
            .returnType(Article.class)
            .arg("id", "Int")
            .register();
    }
}
```

### Pagination Pattern

```java
@GraphQLType
public class UserConnection {
    @GraphQLField public List<User> edges;
    @GraphQLField public int totalCount;
    @GraphQLField public boolean hasNextPage;
    @GraphQLField public int pageInfo;
}

FraiseQL.query("users")
    .returnType(UserConnection.class)
    .arg("first", "Int")          // Limit
    .arg("after", "String")       // Cursor
    .arg("sort", "String")        // Sort field
    .description("Paginated user list")
    .register();
```

### Filtering Pattern

```java
FraiseQL.query("searchUsers")
    .returnType(User.class)
    .returnsArray(true)
    .arg("filter", "String")      // e.g. "name:John"
    .arg("status", "String")      // e.g. "active"
    .arg("createdAfter", "String")
    .arg("limit", "Int")
    .register();
```

### Spring Boot Integration

```java
@Configuration
public class FraiseQLConfig {

    @Bean
    public CommandLineRunner schemaBuilder() {
        return args -> {
            FraiseQL.registerTypes(
                User.class, Post.class, Comment.class
            );

            // Register operations...

            SchemaValidator.ValidationResult result =
                SchemaValidator.validate(SchemaRegistry.getInstance());

            if (result.valid) {
                FraiseQL.exportSchemaToFile("schema.json");
            }
        };
    }
}

@RestController
@RequestMapping("/api")
public class GraphQLController {

    @PostMapping("/graphql")
    public ResponseEntity<?> executeQuery(@RequestBody Map<String, Object> request) {
        // Execute GraphQL query using FraiseQL runtime
        return ResponseEntity.ok("{}");
    }
}
```

---

## Error Handling

### Validation Error Handling

```java
try {
    SchemaRegistry registry = SchemaRegistry.getInstance();
    SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

    if (!result.valid) {
        throw new SchemaValidationException(
            "Schema validation failed with " + result.errors.size() + " errors"
        );
    }

    FraiseQL.exportSchemaToFile("schema.json");

} catch (SchemaValidationException e) {
    System.err.println("Validation error: " + e.getMessage());
    System.exit(1);
} catch (IOException e) {
    System.err.println("File I/O error: " + e.getMessage());
    System.exit(1);
}
```

### Custom Exception Handling

```java
public class FraiseQLException extends RuntimeException {
    public FraiseQLException(String message) {
        super(message);
    }

    public FraiseQLException(String message, Throwable cause) {
        super(message, cause);
    }
}

public class SchemaValidationException extends FraiseQLException {
    public SchemaValidationException(String message) {
        super(message);
    }
}
```

---

## Testing

### JUnit 5 Test Pattern

```java
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class SchemaTest {

    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
        SchemaCache.getInstance().clear();
    }

    @Test
    public void testUserTypeRegistration() {
        FraiseQL.registerType(User.class);

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.GraphQLTypeInfo> userType =
            registry.getType("User");

        assertTrue(userType.isPresent());
    }

    @Test
    public void testQueryRegistration() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        Optional<SchemaRegistry.QueryInfo> userQuery =
            registry.getQuery("user");

        assertTrue(userQuery.isPresent());
    }

    @Test
    public void testSchemaValidation() {
        FraiseQL.registerTypes(User.class, Post.class);
        FraiseQL.query("user").returnType(User.class).arg("id", "Int").register();

        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(registry);

        assertTrue(result.valid);
        assertTrue(result.errors.isEmpty());
    }

    @Test
    public void testSchemaExport() throws IOException {
        FraiseQL.registerType(User.class);
        String schemaJson = FraiseQL.exportSchema();

        assertNotNull(schemaJson);
        assertTrue(schemaJson.contains("User"));
    }
}
```

### Mock Pattern for Testing

```java
public class MockDatabaseAdapter implements DatabaseAdapter {
    private Map<String, Object> data = new HashMap<>();

    @Override
    public Object query(String sql) {
        return data.get(sql);
    }

    public void setMockData(String sql, Object result) {
        data.put(sql, result);
    }
}

@Test
public void testWithMockAdapter() {
    MockDatabaseAdapter adapter = new MockDatabaseAdapter();
    adapter.setMockData("SELECT * FROM users", List.of(
        new User(1, "Alice"),
        new User(2, "Bob")
    ));

    // Execute test with mock
}
```

---

## Troubleshooting

### Common Setup Issues

#### Dependency Resolution

**Issue**: `Could not resolve dependency: FraiseQL:FraiseQL-java:2.0.0`

**Solution - Check repository**:

```xml
<repository>
  <id>central</id>
  <url>https://repo.maven.apache.org/maven2</url>
</repository>
```

Or Maven Central directly:

```bash
mvn clean install -U  # Update snapshots
```

#### Compilation Issues

**Issue**: `Cannot find symbol class FraiseQLServer`

**Verify dependency**:

```xml
<dependency>
  <groupId>com.FraiseQL</groupId>
  <artifactId>FraiseQL-java</artifactId>
  <version>2.0.0</version>
</dependency>
```

```bash
mvn dependency:tree | grep FraiseQL
```

#### Classpath Issues

**Issue**: `ClassNotFoundException: com.FraiseQL.FraiseQLServer`

**Check classpath**:

```bash
# Maven - ensure correct target directory
mvn clean compile

# Gradle - check build output
./gradlew build

# Java - add to classpath explicitly
java -cp ".:lib/*" MyApp
```

#### Java Version Mismatch

**Issue**: `Unsupported major.minor version`

**Check Java version** (11+ required):

```bash
java -version
```

**Set in build**:

```xml
<properties>
  <maven.compiler.source>11</maven.compiler.source>
  <maven.compiler.target>11</maven.compiler.target>
</properties>
```

---

### Type System Issues

#### Annotation Processing Failures

**Issue**: `No processor claimed annotation @FraiseQLType`

**Solution - Enable annotation processing**:

```xml
<plugin>
  <groupId>org.apache.maven.plugins</groupId>
  <artifactId>maven-compiler-plugin</artifactId>
  <configuration>
    <annotationProcessors>
      <annotationProcessor>com.FraiseQL.processor.FraiseQLProcessor</annotationProcessor>
    </annotationProcessors>
  </configuration>
</plugin>
```

#### Type Mapping Errors

**Issue**: `Cannot assign Integer to type String field`

**Solution - Use correct types**:

```java
// ❌ Wrong - type mismatch
@FraiseQLType
public class User {
    public String id;  // Should be int or UUID
}

// ✅ Correct
@FraiseQLType
public class User {
    public int id;
    public String email;
}
```

#### Null Safety Issues

**Issue**: `NullPointerException on User.email`

**Solution - Use Optional**:

```java
// ❌ Can throw NPE
@FraiseQLType
public class User {
    public String email;  // Could be null
}

// ✅ Explicit nullability
@FraiseQLType
public class User {
    @Nullable
    public String middleName;

    @NonNull
    public String email;
}
```

#### Generics Issues

**Issue**: `Type erasure prevents generic type resolution`

**Solution - Use concrete types**:

```java
// ❌ Won't work - generics erased at runtime
@FraiseQLType
public class Box<T> {
    public T value;
}

// ✅ Use concrete types
@FraiseQLType
public class UserBox {
    public User value;
}
```

---

### Runtime Errors

#### Thread Safety Issues

**Issue**: `ConcurrentModificationException in schema execution`

**Solution - Use thread-safe patterns**:

```java
// Ensure server instance is thread-safe
private static final FraiseQLServer server = FraiseQLServer.fromCompiled(
    "schema.compiled.json"
);

// Each request can reuse same server
@PostMapping("/graphql")
public ResponseEntity<?> graphql(@RequestBody GraphQLRequest request) {
    // Server is thread-safe
    QueryResult result = server.execute(request.getQuery());
    return ResponseEntity.ok(result);
}
```

#### Connection Pool Exhaustion

**Issue**: `HikariPool - Connection is not available`

**Check pool configuration**:

```java
HikariConfig config = new HikariConfig();
config.setMaximumPoolSize(20);
config.setMinimumIdle(5);
config.setConnectionTimeout(30000);
```

**Or via properties**:

```properties
spring.datasource.hikari.maximum-pool-size=20
spring.datasource.hikari.minimum-idle=5
```

#### Reflection Issues

**Issue**: `Cannot access field X: class does not have declared field`

**Solution - Check field visibility and names**:

```java
// Ensure fields are accessible
@FraiseQLType
public class User {
    public int id;      // public, not private
    public String name;
}

// Use @JsonProperty for name mapping if needed
@FraiseQLType
public class User {
    @JsonProperty("user_id")
    public int userId;
}
```

#### Async/CompletableFuture Issues

**Issue**: `Future never completes`

**Solution - Properly handle async**:

```java
// ❌ Wrong - not handling completion
FraiseQLServer.fromCompiledAsync("schema.json").thenApply(server -> {
    // Doesn't wait for this
    return server;
});

// ✅ Correct - chain operations
FraiseQLServer.fromCompiledAsync("schema.json")
    .thenApply(server -> {
        QueryResult result = server.execute(query);
        return result;
    })
    .thenAccept(result -> {
        // Handle result
    })
    .exceptionally(error -> {
        error.printStackTrace();
        return null;
    });
```

---

### Performance Issues

#### Slow Query Compilation

**Issue**: Schema compilation takes >10 seconds on startup

**Pre-compile**:

```bash
# Use FraiseQL-cli to pre-compile
FraiseQL-cli compile schema.json FraiseQL.toml

# Load pre-compiled schema (faster)
FraiseQLServer server = FraiseQLServer.fromCompiled("schema.compiled.json");
```

#### Large Heap Size

#### Issue**: Application uses >1GB memory

**Profile with jmap**:

```bash
jmap -heap <pid>  # Check heap usage
jmap -dump:live,format=b,file=heap.bin <pid>  # Dump heap
jhat heap.bin  # Analyze dump
```

**Solutions**:

```java
// Paginate large result sets
@Query(sql_source = "v_users")
public List<User> users(
    @GraphQLArgument(name = "limit", defaultValue = "20") int limit,
    @GraphQLArgument(name = "offset", defaultValue = "0") int offset
) {
    // Limit results
    return new ArrayList<>();
}

// Close resources explicitly
server.close();  // Or use try-with-resources
```

#### GC Pressure

#### Issue**: Frequent garbage collection pauses

**Enable GC logging**:

```bash
java -XX:+PrintGCDetails -XX:+PrintGCDateStamps -Xloggc:gc.log MyApp
```

**Optimize**:

- Use connection pooling
- Cache compiled schema
- Batch mutations
- Use pagination

#### Build Time Issues

#### Issue**: Maven build takes >2 minutes

**Parallel compilation**:

```bash
mvn clean compile -T 1C  # 1 thread per core
```

**Skip tests during development**:

```bash
mvn install -DskipTests
```

---

### Debugging Techniques

#### Enable Logging

**Setup SLF4J/Logback**:

```xml
<dependency>
  <groupId>org.slf4j</groupId>
  <artifactId>slf4j-api</artifactId>
  <version>2.0.0</version>
</dependency>
<dependency>
  <groupId>ch.qos.logback</groupId>
  <artifactId>logback-classic</artifactId>
  <version>1.4.0</version>
</dependency>
```

**In code**:

```java
private static final Logger logger = LoggerFactory.getLogger(GraphQLController.class);

@PostMapping("/graphql")
public ResponseEntity<?> graphql(@RequestBody GraphQLRequest request) {
    logger.debug("Executing query: {}", request.getQuery());
    try {
        QueryResult result = server.execute(request.getQuery());
        return ResponseEntity.ok(result);
    } catch (Exception e) {
        logger.error("Query failed", e);
        throw e;
    }
}
```

#### Use IDE Debugger

**IntelliJ IDEA**:

1. Set breakpoint (click line number)
2. Run in debug mode (Shift+F9)
3. Step through code (F10)
4. Inspect variables in Variables panel

#### Inspect Generated Classes

**Check bytecode**:

```bash
javap -c -private com.example.User
```

### Or use javap UI in IDE

#### Network Debugging

**Monitor SQL traffic**:

```bash
# PostgreSQL slow query log
ALTER SYSTEM SET log_min_duration_statement = 1000;  # Log slow queries >1s
```

**Monitor GraphQL traffic**:

```bash
curl -X POST http://localhost:8080/api/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ user(id: 1) { id } }"}' \
  -v
```

---

### Getting Help

#### GitHub Issues

Provide:

1. Java version: `java -version`
2. Build tool: Maven/Gradle version
3. FraiseQL version
4. Minimal reproducible example
5. Full stack trace
6. Relevant logs

**Issue template**:

```markdown
**Environment**:
- Java: 11.0.15
- Maven: 3.8.1
- FraiseQL: 2.0.0

**Issue**:
[Describe problem]

**Reproduce**:
[Minimal code]

**Error**:
[Full stack trace]
```

#### Community Channels

- **GitHub Discussions**: Q&A
- **Stack Overflow**: Tag with `FraiseQL` and `java`
- **Discord**: Real-time help

#### Profiling Tools

**JProfiler**:

```bash
jprofiletask -config=config.xml MyApp
```

**YourKit**:

```bash
java -agentpath:/path/to/libyjpagent.so MyApp
```

---

## See Also

- [API Guide](../../reference/README.md) - Detailed API reference
- [Python SDK Reference](./python-reference.md) - Python SDK documentation
- [TypeScript SDK Reference](./typescript-reference.md) - TypeScript SDK documentation
- [Security & RBAC Guide](../../guides/authorization-quick-start.md) - Authorization patterns
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md) - Fact tables and aggregations
- [GraphQL Scalar Types](../../reference/scalars.md) - Complete scalar type reference
- [Architecture Principles](../../architecture/README.md) - System design

---

**Status**: Production Ready ✅ | **Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community
