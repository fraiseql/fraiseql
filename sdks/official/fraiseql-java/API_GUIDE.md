# FraiseQL Java API Guide

Complete reference for the FraiseQL Java authoring API.

## Core Classes

### @GraphQLType Annotation

Marks a class as a GraphQL type definition.

```java
@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;
}
```

**Attributes:**

- None (applies to class level only)

### @GraphQLField Annotation

Marks a field as part of a GraphQL type.

```java
@GraphQLField
public String name;

@GraphQLField(nullable = true)
public String email;

@GraphQLField(name = "userName", type = "String")
public String username;
```

**Attributes:**

- `nullable` (boolean, default: false) - Whether the field can be null
- `name` (String) - Custom field name in the schema (defaults to Java field name)
- `type` (String) - Custom GraphQL type (defaults to auto-detected type)
- `description` (String) - Field description for schema documentation

### FraiseQL

Main API entry point for schema registration and export.

#### Type Registration

```java
// Register a single type
FraiseQL.registerType(User.class);

// Register multiple types
FraiseQL.registerTypes(User.class, Post.class, Comment.class);
```

#### Query Registration

```java
FraiseQL.query("user")
    .returnType(User.class)
    .arg("id", "Int")
    .register();

// With array return
FraiseQL.query("users")
    .returnType(User.class)
    .returnsArray(true)
    .register();

// With multiple arguments
FraiseQL.query("search")
    .returnType(Post.class)
    .returnsArray(true)
    .arg("query", "String")
    .arg("limit", "Int")
    .arg("offset", "Int")
    .register();

// With description
FraiseQL.query("post")
    .returnType(Post.class)
    .arg("id", "Int")
    .description("Get a single post by ID")
    .register();
```

#### Mutation Registration

```java
FraiseQL.mutation("createUser")
    .returnType(User.class)
    .arg("name", "String")
    .arg("email", "String")
    .register();

FraiseQL.mutation("deleteUser")
    .returnType(User.class)
    .arg("id", "Int")
    .description("Delete a user by ID")
    .register();
```

#### Schema Export

```java
// Get schema as JSON string
String schemaJson = FraiseQL.exportSchema();

// Write schema to file
FraiseQL.exportSchemaToFile("schema.json");

// Get specific schema registry
SchemaRegistry registry = SchemaRegistry.getInstance();
```

#### Schema Cleanup

```java
// Clear all registered types, queries, and mutations
FraiseQL.clear();
```

### QueryBuilder

Fluent builder for defining queries.

```java
FraiseQL.query("users")
    .returnType(User.class)           // Required
    .returnsArray(true)               // Optional
    .arg("limit", "Int")              // Optional, repeatable
    .arg("offset", "Int")
    .description("Get list of users") // Optional
    .register();                       // Required
```

**Methods:**

- `returnType(Class<?>)` - Set return type (required)
- `returnType(String)` - Set return type by string name
- `returnsArray(boolean)` - Whether result is an array
- `arg(String name, String type)` - Add argument
- `description(String)` - Add description
- `register()` - Register the query

### MutationBuilder

Fluent builder for defining mutations (same interface as QueryBuilder).

```java
FraiseQL.mutation("updateUser")
    .returnType(User.class)
    .arg("id", "Int")
    .arg("name", "String")
    .description("Update a user")
    .register();
```

### ArgumentBuilder

Builder for creating arguments with defaults and descriptions.

```java
ArgumentBuilder args = new ArgumentBuilder()
    .add("limit", "Int", 10, "Maximum items")
    .add("offset", "Int", 0, "Pagination offset")
    .add("filter", "String", null, "Search filter");

// Get simple map
Map<String, String> argMap = args.build();

// Get detailed info with descriptions
Map<String, ArgumentBuilder.ArgumentInfo> detailed = args.buildDetailed();

// Check if argument has default
if (args.hasDefault("limit")) {
    Object defaultValue = args.getDefault("limit");
}

// Get arguments with defaults only
List<ArgumentBuilder.ArgumentInfo> withDefaults = args.getArgumentsWithDefaults();
for (ArgumentBuilder.ArgumentInfo arg : withDefaults) {
    System.out.println(arg.name + ": " + arg.defaultValue);
}
```

**Methods:**

- `add(String name, String type)` - Add required argument
- `add(String name, String type, Object default)` - Add optional argument with default
- `add(String name, String type, Object default, String description)` - Add with description
- `build()` - Get `Map<String, String>` of arguments
- `buildDetailed()` - Get detailed argument info with descriptions
- `hasDefault(String name)` - Check if argument has default
- `getDefault(String name)` - Get default value
- `getArgumentsWithDefaults()` - Get list of arguments with defaults

### SchemaRegistry

Thread-safe registry managing all types, queries, and mutations.

```java
SchemaRegistry registry = SchemaRegistry.getInstance();

// Get registered type
Optional<SchemaRegistry.GraphQLTypeInfo> userType =
    registry.getType("User");

// Get registered query
Optional<SchemaRegistry.QueryInfo> userQuery =
    registry.getQuery("user");

// Get registered mutation
Optional<SchemaRegistry.MutationInfo> createUserMutation =
    registry.getMutation("createUser");

// Get all types
Map<String, SchemaRegistry.GraphQLTypeInfo> allTypes =
    registry.getAllTypes();

// Get all queries
Map<String, SchemaRegistry.QueryInfo> allQueries =
    registry.getAllQueries();

// Get all mutations
Map<String, SchemaRegistry.MutationInfo> allMutations =
    registry.getAllMutations();
```

### SchemaValidator

Validates schema correctness and completeness.

```java
SchemaRegistry registry = SchemaRegistry.getInstance();

// Validate schema
SchemaValidator.ValidationResult result =
    SchemaValidator.validate(registry);

if (result.valid) {
    System.out.println("Schema is valid!");
} else {
    System.out.println("Errors:");
    for (String error : result.errors) {
        System.err.println("  - " + error);
    }
}

// Show warnings
for (String warning : result.warnings) {
    System.out.println("  ⚠ " + warning);
}

// Get statistics
String stats = SchemaValidator.getStatistics(registry);
System.out.println(stats);
// Output: "Schema Statistics: 3 types (8 fields), 5 queries, 2 mutations"
```

**ValidationResult Properties:**

- `valid` (boolean) - Whether schema passed validation
- `errors` (List<String>) - List of validation errors
- `warnings` (List<String>) - List of non-fatal warnings

### SchemaFormatter

Converts schema registry to JSON format.

```java
SchemaRegistry registry = SchemaRegistry.getInstance();
SchemaFormatter formatter = new SchemaFormatter(registry);

// Get JSON as ObjectNode
ObjectNode schemaJson = formatter.formatSchema();

// Get pretty-printed JSON string
String jsonString = formatter.toJsonString();

// Write to file
formatter.writeToFile("schema.json");
```

### TypeConverter

Handles Java-to-GraphQL type conversions.

```java
// Convert single Java type
String graphqlType = TypeConverter.javaToGraphQL(String.class);
// Returns: "String"

String intType = TypeConverter.javaToGraphQL(int.class);
// Returns: "Int"

// Extract fields from annotated class
Map<String, TypeConverter.GraphQLFieldInfo> fields =
    TypeConverter.extractFields(User.class);

for (Map.Entry<String, TypeConverter.GraphQLFieldInfo> entry : fields.entrySet()) {
    TypeConverter.GraphQLFieldInfo field = entry.getValue();
    System.out.println(field.name + ": " + field.getGraphQLType());
}

// Get detailed type info
TypeConverter.TypeInfo typeInfo =
    TypeConverter.convertToTypeInfo(String[].class);
// Returns TypeInfo with isList=true, nullable=false, type="String"
```

### SchemaCache

High-performance caching for schema operations.

```java
SchemaCache cache = SchemaCache.getInstance();

// Cache field information
Map<String, TypeConverter.GraphQLFieldInfo> fields =
    TypeConverter.extractFields(User.class);
cache.putFieldCache(User.class, fields);

// Retrieve cached fields
Map<String, TypeConverter.GraphQLFieldInfo> cachedFields =
    cache.getFieldCache(User.class);

// Cache type conversions
cache.putTypeConversion(String.class, "String");
cache.putTypeConversion(int.class, "Int");

// Retrieve cached conversion
String graphqlType = cache.getTypeConversion(String.class);

// Cache validation results
cache.putTypeValidation("User", true);
cache.putTypeValidation("InvalidType", false);

// Retrieve validation
Boolean isValid = cache.getTypeValidation("User");

// Get cache statistics
SchemaCache.CacheStats stats = cache.getStats();
System.out.println("Field cache hits: " + stats.getFieldCacheHits());
System.out.println("Type conversion hits: " + stats.getTypeConversionHits());
System.out.println("Validation hits: " + stats.getValidationHits());
System.out.println("Total hits: " + stats.getTotalHits());

// Get cache size info
SchemaCache.CacheSizeInfo sizeInfo = cache.getSizeInfo();
System.out.println("Field cache size: " + sizeInfo.fieldCacheSize);
System.out.println("Type conversion cache size: " + sizeInfo.typeConversionCacheSize);
System.out.println("Validation cache size: " + sizeInfo.validationCacheSize);
System.out.println("Total entries: " + sizeInfo.getTotalEntries());

// Clear cache (typically for testing)
cache.clear();
```

### PerformanceMonitor

Monitors and tracks performance metrics for schema operations.

```java
PerformanceMonitor monitor = PerformanceMonitor.getInstance();

// Record operation timing (in milliseconds)
monitor.recordOperation("typeConversion", 10);
monitor.recordOperation("typeConversion", 15);
monitor.recordOperation("typeConversion", 8);

// Get metrics for specific operation
PerformanceMonitor.OperationMetrics metrics =
    monitor.getMetrics("typeConversion");

System.out.println("Operation count: " + metrics.getOperationCount());     // 3
System.out.println("Min latency: " + metrics.getMinLatency() + " ms");     // 8
System.out.println("Max latency: " + metrics.getMaxLatency() + " ms");     // 15
System.out.println("Average latency: " + metrics.getAverageLatency() + " ms"); // 11

// Get all metrics
Map<String, PerformanceMonitor.OperationMetrics> allMetrics =
    monitor.getAllMetrics();

// Get system-wide metrics
PerformanceMonitor.SystemMetrics systemMetrics = monitor.getSystemMetrics();
System.out.println("Total operations: " + systemMetrics.getTotalOperations());
System.out.println("Tracked operation types: " + systemMetrics.getTrackedOperations());
System.out.println("Average latency (all): " + systemMetrics.getAverageLatency() + " ms");
System.out.println("Operations/sec: " + systemMetrics.getOperationsPerSecond());
System.out.println("Uptime: " + systemMetrics.getUptimeMillis() + " ms");

// Generate formatted report
String report = monitor.generateReport();
System.out.println(report);

// Reset all metrics
monitor.reset();
```

## Type Conversion Reference

### Automatic Type Detection

When using `@GraphQLField`, FraiseQL automatically converts Java types:

```java
@GraphQLType
public class Example {
    @GraphQLField
    public int id;                    // → Int!

    @GraphQLField
    public String name;               // → String!

    @GraphQLField
    public boolean active;            // → Boolean!

    @GraphQLField
    public float rating;              // → Float!

    @GraphQLField
    public LocalDate createdDate;     // → String!

    @GraphQLField
    public UUID uuid;                 // → ID!

    @GraphQLField
    public String[] tags;             // → [String]!

    @GraphQLField(nullable = true)
    public String description;        // → String

    @GraphQLField(nullable = true)
    public String[] optionalTags;     // → [String]
}
```

### Explicit Type Definition

Use the `type` attribute for custom types:

```java
@GraphQLType
public class CustomExample {
    @GraphQLField(type = "User")
    public Object userData;

    @GraphQLField(type = "PostConnection")
    public Object posts;

    @GraphQLField(type = "[String]", nullable = true)
    public Object customList;
}
```

## Common Patterns

### Basic CRUD Schema

```java
@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String email;

    @GraphQLField
    public String name;
}

// Setup
FraiseQL.registerType(User.class);

// Queries
FraiseQL.query("user")
    .returnType(User.class)
    .arg("id", "Int")
    .register();

FraiseQL.query("users")
    .returnType(User.class)
    .returnsArray(true)
    .arg("limit", "Int")
    .arg("offset", "Int")
    .register();

// Mutations
FraiseQL.mutation("createUser")
    .returnType(User.class)
    .arg("email", "String")
    .arg("name", "String")
    .register();

FraiseQL.mutation("updateUser")
    .returnType(User.class)
    .arg("id", "Int")
    .arg("name", "String")
    .register();

FraiseQL.mutation("deleteUser")
    .returnType(User.class)
    .arg("id", "Int")
    .register();
```

### Validation Before Export

```java
FraiseQL.registerTypes(User.class, Post.class);
// ... register queries and mutations

SchemaRegistry registry = SchemaRegistry.getInstance();
SchemaValidator.ValidationResult result = SchemaValidator.validate(registry);

if (result.valid) {
    FraiseQL.exportSchemaToFile("schema.json");
    System.out.println(SchemaValidator.getStatistics(registry));
} else {
    result.errors.forEach(System.err::println);
    System.exit(1);
}
```

### Performance Tracking

```java
PerformanceMonitor monitor = PerformanceMonitor.getInstance();

long startTime = System.currentTimeMillis();

// ... perform schema operations

long duration = System.currentTimeMillis() - startTime;
monitor.recordOperation("schemaSetup", duration);

System.out.println(monitor.generateReport());
```

## Error Handling

All validation is done before schema registration completes. If there are issues:

```java
SchemaValidator.ValidationResult result =
    SchemaValidator.validate(SchemaRegistry.getInstance());

if (!result.valid) {
    System.out.println("Schema has " + result.errors.size() + " errors:");
    for (String error : result.errors) {
        System.err.println("  ERROR: " + error);
    }

    if (!result.warnings.isEmpty()) {
        System.out.println("And " + result.warnings.size() + " warnings:");
        for (String warning : result.warnings) {
            System.out.println("  WARNING: " + warning);
        }
    }
}
```

## Testing

FraiseQL works well with JUnit 5. Example test setup:

```java
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

public class MySchemaTest {
    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
        SchemaCache.getInstance().clear();
    }

    @Test
    public void testSchema() {
        FraiseQL.registerType(User.class);
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(SchemaRegistry.getInstance());

        assertTrue(result.valid);
    }
}
```

## Next Steps

1. Review [INSTALL.md](INSTALL.md) for setup instructions
2. Check [EXAMPLES.md](EXAMPLES.md) for complete working examples
3. See [CONTRIBUTING.md](CONTRIBUTING.md) to contribute
