# FraiseQL Java - Installation Guide

FraiseQL Java is a GraphQL schema authoring library that allows you to define GraphQL schemas using simple Java annotations. Your schema definitions are converted to a JSON schema that can be compiled by the FraiseQL CLI into optimized SQL.

## Prerequisites

- **Java 17 or higher**
- **Maven 3.8.1 or higher**

## Installation

### Via Maven Central (Coming Soon)

Add the following dependency to your `pom.xml`:

```xml
<dependency>
    <groupId>com.fraiseql</groupId>
    <artifactId>fraiseql-java</artifactId>
    <version>2.0.0</version>
</dependency>
```

### Via Local Installation

For development or testing, build and install locally:

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql/fraiseql-java

# Build and install
mvn clean install
```

Then add to your project's `pom.xml`:

```xml
<dependency>
    <groupId>com.fraiseql</groupId>
    <artifactId>fraiseql-java</artifactId>
    <version>2.0.0-SNAPSHOT</version>
</dependency>
```

## Quick Start

### 1. Define Your Types

Use the `@GraphQLType` annotation to mark classes as GraphQL types:

```java
import com.fraiseql.core.*;

@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField(nullable = true)
    public String email;
}

@GraphQLType
public class Post {
    @GraphQLField
    public int id;

    @GraphQLField
    public String title;

    @GraphQLField
    public String content;

    @GraphQLField
    public int authorId;
}
```

### 2. Register Types and Define Queries

```java
// Register your types
FraiseQL.registerTypes(User.class, Post.class);

// Define queries
FraiseQL.query("user")
    .returnType(User.class)
    .arg("id", "Int")
    .register();

FraiseQL.query("posts")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("limit", "Int")
    .arg("offset", "Int")
    .register();

// Define mutations
FraiseQL.mutation("createUser")
    .returnType(User.class)
    .arg("name", "String")
    .arg("email", "String")
    .register();
```

### 3. Export Your Schema

```java
// Export to JSON
String schemaJson = FraiseQL.exportSchema();

// Or write to file
FraiseQL.exportSchemaToFile("schema.json");
```

## Field Annotations

The `@GraphQLField` annotation supports several options:

```java
@GraphQLType
public class Example {
    // Required field with auto-detected type
    @GraphQLField
    public String name;

    // Nullable field
    @GraphQLField(nullable = true)
    public String description;

    // Custom GraphQL type (overrides Java type detection)
    @GraphQLField(type = "Custom")
    public Object customData;

    // Custom field name in schema
    @GraphQLField(name = "userName")
    public String name;

    // With description
    @GraphQLField(description = "The user's email address")
    public String email;

    // List of items
    @GraphQLField
    public String[] tags;

    // Nullable list
    @GraphQLField(nullable = true)
    public String[] optionalTags;

    // All options combined
    @GraphQLField(
        nullable = true,
        name = "customName",
        description = "A custom field",
        type = "CustomType"
    )
    public Object field;
}
```

## Query and Mutation Arguments

Use `ArgumentBuilder` to create queries with default values and descriptions:

```java
ArgumentBuilder args = new ArgumentBuilder()
    .add("limit", "Int", 10, "Maximum items to return")
    .add("offset", "Int", 0, "Pagination offset")
    .add("filter", "String", null, "Optional search filter");

FraiseQL.query("items")
    .returnType(Item.class)
    .returnsArray(true)
    .arg("limit", "Int")
    .arg("offset", "Int")
    .arg("filter", "String")
    .register();
```

## Schema Validation

Validate your schema before deployment:

```java
SchemaRegistry registry = SchemaRegistry.getInstance();
SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

if (result.valid) {
    System.out.println("Schema is valid!");
    System.out.println(SchemaValidator.getStatistics(registry));
} else {
    System.out.println("Validation errors:");
    result.errors.forEach(System.err::println);
}
```

## Performance Monitoring

Monitor schema operations and cache performance:

```java
// Record operation timing
PerformanceMonitor monitor = PerformanceMonitor.getInstance();
monitor.recordOperation("typeConversion", 10);
monitor.recordOperation("typeConversion", 15);

// Get metrics
PerformanceMonitor.OperationMetrics metrics =
    monitor.getMetrics("typeConversion");
System.out.println("Average latency: " + metrics.getAverageLatency() + " ms");

// Get overall system metrics
PerformanceMonitor.SystemMetrics systemMetrics =
    monitor.getSystemMetrics();
System.out.println("Operations per second: " +
    systemMetrics.getOperationsPerSecond());

// Generate performance report
System.out.println(monitor.generateReport());
```

## Caching

The schema cache automatically caches field information and type conversions:

```java
SchemaCache cache = SchemaCache.getInstance();

// Check cache statistics
SchemaCache.CacheStats stats = cache.getStats();
System.out.println("Cache hits: " + stats.getTotalHits());

// Get cache size info
SchemaCache.CacheSizeInfo sizeInfo = cache.getSizeInfo();
System.out.println("Cached types: " + sizeInfo.typeConversionCacheSize);
```

## Supported Types

FraiseQL automatically converts Java types to GraphQL types:

| Java Type | GraphQL Type | Notes |
|-----------|--------------|-------|
| `int`, `Integer` | `Int` | 32-bit integer |
| `long`, `Long` | `Int` | Mapped to GraphQL Int |
| `float`, `Float` | `Float` | Single precision |
| `double`, `Double` | `Float` | Double precision |
| `boolean`, `Boolean` | `Boolean` | True/false |
| `String` | `String` | Text |
| `UUID` | `ID` | Unique identifier |
| `LocalDate` | `String` | ISO 8601 format |
| `LocalDateTime` | `String` | ISO 8601 format |
| `BigDecimal` | `Float` | Arbitrary precision |
| `Type[]` | `[Type]` | Array/List |
| Custom `@GraphQLType` | Type name | User-defined type |

## Next Steps

1. **Define your schema** - Create `@GraphQLType` classes for your data
2. **Export to JSON** - Generate the `schema.json` file
3. **Compile with FraiseQL CLI** - Run `fraiseql-cli compile schema.json`
4. **Use the compiled schema** - Load `schema.compiled.json` in your Rust GraphQL server

## Examples

See the `examples/` directory for complete examples:

- **BasicSchema** - Simple blog/CMS application
- **EcommerceSchema** - Complex ecommerce system with multiple types

## Troubleshooting

### "Cannot find @GraphQLType annotation"

Ensure the JAR is properly on your classpath. Check:
- `mvn dependency:tree` includes fraiseql-java
- Java version is 17+

### "No types registered in schema"

You must register at least one type:

```java
@GraphQLType
public class MyType { ... }

FraiseQL.registerType(MyType.class);
```

### "Field has no type"

All `@GraphQLField` fields must have a detectable type. Check:
- Primitive types: `int`, `String`, `boolean`, etc.
- Annotated types: `@GraphQLType` classes
- Supported types: See "Supported Types" section

### "Undefined return type" in validation

The query's return type must be a registered `@GraphQLType`:

```java
FraiseQL.registerType(User.class);
FraiseQL.query("user").returnType(User.class).register();  // ✅ Good
FraiseQL.query("user").returnType("User").register();      // ❌ Bad - string won't work
```

## Support

For issues, questions, or contributions:

- **GitHub Issues**: https://github.com/fraiseql/fraiseql/issues
- **Documentation**: https://docs.fraiseql.com
- **Discord**: https://discord.gg/fraiseql

## License

FraiseQL Java is licensed under the Apache License 2.0. See LICENSE file for details.
