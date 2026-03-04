# FraiseQL Java Examples

This document contains complete working examples demonstrating FraiseQL Java functionality.

## Example 1: Basic Blog/CMS Schema

A simple content management system with users, posts, and comments.

```java
import com.fraiseql.core.*;

// 1. Define your types
@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField
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

@GraphQLType
public class Comment {
    @GraphQLField
    public int id;

    @GraphQLField
    public String text;

    @GraphQLField
    public int postId;

    @GraphQLField
    public int authorId;
}

// 2. Register and export
public class BlogSchema {
    public static void main(String[] args) throws Exception {
        // Register types
        FraiseQL.registerTypes(User.class, Post.class, Comment.class);

        // Define queries
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .description("Get a user by ID")
            .register();

        FraiseQL.query("posts")
            .returnType(Post.class)
            .returnsArray(true)
            .arg("authorId", "Int")
            .arg("limit", "Int")
            .description("Get posts by author")
            .register();

        FraiseQL.query("comments")
            .returnType(Comment.class)
            .returnsArray(true)
            .arg("postId", "Int")
            .description("Get comments for a post")
            .register();

        // Define mutations
        FraiseQL.mutation("createUser")
            .returnType(User.class)
            .arg("name", "String")
            .arg("email", "String")
            .register();

        FraiseQL.mutation("createPost")
            .returnType(Post.class)
            .arg("title", "String")
            .arg("content", "String")
            .arg("authorId", "Int")
            .register();

        FraiseQL.mutation("createComment")
            .returnType(Comment.class)
            .arg("text", "String")
            .arg("postId", "Int")
            .arg("authorId", "Int")
            .register();

        // Validate schema
        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(SchemaRegistry.getInstance());

        if (result.valid) {
            System.out.println("✓ Schema is valid!");
            System.out.println(SchemaValidator.getStatistics(
                SchemaRegistry.getInstance()));

            // Export to file
            FraiseQL.exportSchemaToFile("blog-schema.json");
            System.out.println("✓ Schema exported to blog-schema.json");
        } else {
            System.err.println("✗ Schema validation failed:");
            result.errors.forEach(e -> System.err.println("  - " + e));
            System.exit(1);
        }
    }
}
```

## Example 2: Advanced Ecommerce Schema

A complex ecommerce system with multiple types and relationships.

```java
import com.fraiseql.core.*;
import java.math.BigDecimal;
import java.time.LocalDateTime;

@GraphQLType
public class Product {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField
    public String description;

    @GraphQLField
    public BigDecimal price;

    @GraphQLField
    public int inventory;

    @GraphQLField
    public int categoryId;
}

@GraphQLType
public class Category {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField
    public String description;
}

@GraphQLType
public class Customer {
    @GraphQLField
    public int id;

    @GraphQLField
    public String email;

    @GraphQLField
    public String name;

    @GraphQLField
    public String phone;
}

@GraphQLType
public class Order {
    @GraphQLField
    public int id;

    @GraphQLField
    public int customerId;

    @GraphQLField
    public LocalDateTime createdAt;

    @GraphQLField
    public BigDecimal total;

    @GraphQLField
    public String status;
}

@GraphQLType
public class OrderItem {
    @GraphQLField
    public int id;

    @GraphQLField
    public int orderId;

    @GraphQLField
    public int productId;

    @GraphQLField
    public int quantity;

    @GraphQLField
    public BigDecimal unitPrice;
}

public class EcommerceSchema {
    public static void main(String[] args) throws Exception {
        // Register all types
        FraiseQL.registerTypes(
            Product.class, Category.class, Customer.class,
            Order.class, OrderItem.class
        );

        // Product queries
        FraiseQL.query("product")
            .returnType(Product.class)
            .arg("id", "Int")
            .register();

        FraiseQL.query("products")
            .returnType(Product.class)
            .returnsArray(true)
            .arg("categoryId", "Int")
            .arg("limit", "Int")
            .arg("offset", "Int")
            .register();

        FraiseQL.query("searchProducts")
            .returnType(Product.class)
            .returnsArray(true)
            .arg("query", "String")
            .arg("limit", "Int")
            .register();

        // Category queries
        FraiseQL.query("categories")
            .returnType(Category.class)
            .returnsArray(true)
            .register();

        // Customer queries
        FraiseQL.query("customer")
            .returnType(Customer.class)
            .arg("id", "Int")
            .register();

        // Order queries
        FraiseQL.query("orders")
            .returnType(Order.class)
            .returnsArray(true)
            .arg("customerId", "Int")
            .arg("limit", "Int")
            .register();

        // Customer mutations
        FraiseQL.mutation("createCustomer")
            .returnType(Customer.class)
            .arg("email", "String")
            .arg("name", "String")
            .arg("phone", "String")
            .register();

        FraiseQL.mutation("updateCustomer")
            .returnType(Customer.class)
            .arg("id", "Int")
            .arg("name", "String")
            .arg("phone", "String")
            .register();

        // Order mutations
        FraiseQL.mutation("createOrder")
            .returnType(Order.class)
            .arg("customerId", "Int")
            .arg("items", "String")  // JSON array
            .register();

        FraiseQL.mutation("updateOrderStatus")
            .returnType(Order.class)
            .arg("id", "Int")
            .arg("status", "String")
            .register();

        // Validate and export
        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(registry);

        if (result.valid) {
            System.out.println("✓ Ecommerce schema is valid!");
            System.out.println(SchemaValidator.getStatistics(registry));
            FraiseQL.exportSchemaToFile("ecommerce-schema.json");
        } else {
            result.errors.forEach(System.err::println);
            System.exit(1);
        }
    }
}
```

## Example 3: Schema with Default Arguments

Using `ArgumentBuilder` for default values and descriptions.

```java
import com.fraiseql.core.*;

@GraphQLType
public class Article {
    @GraphQLField
    public int id;

    @GraphQLField
    public String title;

    @GraphQLField
    public String content;

    @GraphQLField
    public int views;
}

public class ArticleSchema {
    public static void main(String[] args) throws Exception {
        FraiseQL.registerType(Article.class);

        // Create query with default arguments
        FraiseQL.query("articles")
            .returnType(Article.class)
            .returnsArray(true)
            .arg("limit", "Int")       // Required
            .arg("offset", "Int")      // Required
            .arg("sort", "String")     // Optional, no default
            .description("List articles with pagination")
            .register();

        // You can also use ArgumentBuilder for validation
        ArgumentBuilder args = new ArgumentBuilder()
            .add("limit", "Int", 10, "Items per page")
            .add("offset", "Int", 0, "Pagination offset")
            .add("sort", "String", "created", "Sort field")
            .add("order", "String", "DESC", "Sort order");

        // Access arguments
        if (args.hasDefault("limit")) {
            System.out.println("Default limit: " + args.getDefault("limit"));
        }

        // Get detailed info
        var detailed = args.buildDetailed();
        for (String name : detailed.keySet()) {
            var info = detailed.get(name);
            System.out.println(
                info.name + ": " + info.type +
                (info.isOptional() ? " (optional)" : " (required)")
            );
        }

        // Export
        FraiseQL.exportSchemaToFile("articles-schema.json");
    }
}
```

## Example 4: Performance Monitoring

Track schema operation performance.

```java
import com.fraiseql.core.*;

public class PerformanceExample {
    public static void main(String[] args) throws Exception {
        @GraphQLType
        class User {
            @GraphQLField public int id;
            @GraphQLField public String name;
        }

        PerformanceMonitor monitor = PerformanceMonitor.getInstance();

        // Measure schema setup time
        long startTime = System.currentTimeMillis();

        // Register types
        FraiseQL.registerType(User.class);

        // Register queries
        for (int i = 0; i < 10; i++) {
            FraiseQL.query("query" + i)
                .returnType(User.class)
                .arg("id", "Int")
                .register();
        }

        long setupTime = System.currentTimeMillis() - startTime;
        monitor.recordOperation("schemaSetup", setupTime);

        // Export schema
        startTime = System.currentTimeMillis();
        FraiseQL.exportSchemaToFile("perf-schema.json");
        long exportTime = System.currentTimeMillis() - startTime;
        monitor.recordOperation("schemaExport", exportTime);

        // Print metrics
        System.out.println("Schema Setup Metrics:");
        PerformanceMonitor.OperationMetrics setupMetrics =
            monitor.getMetrics("schemaSetup");
        System.out.println("  Duration: " + setupMetrics.getAverageLatency() + " ms");

        System.out.println("\nSchema Export Metrics:");
        PerformanceMonitor.OperationMetrics exportMetrics =
            monitor.getMetrics("schemaExport");
        System.out.println("  Duration: " + exportMetrics.getAverageLatency() + " ms");

        // System-wide metrics
        System.out.println("\nSystem Metrics:");
        System.out.println(monitor.generateReport());
    }
}
```

## Example 5: Type Conversion and Caching

Demonstrating type conversion and schema caching.

```java
import com.fraiseql.core.*;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.util.UUID;

@GraphQLType
public class Person {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField
    public LocalDate birthDate;

    @GraphQLField
    public LocalDateTime createdAt;

    @GraphQLField
    public UUID uuid;

    @GraphQLField
    public String[] tags;

    @GraphQLField(nullable = true)
    public String bio;
}

public class TypeConversionExample {
    public static void main(String[] args) throws Exception {
        FraiseQL.registerType(Person.class);

        // Use SchemaCache
        SchemaCache cache = SchemaCache.getInstance();

        // Extract and cache fields
        var fields = TypeConverter.extractFields(Person.class);
        System.out.println("Person fields:");
        for (var entry : fields.entrySet()) {
            System.out.println(
                "  " + entry.getKey() + ": " + entry.getValue().getGraphQLType()
            );
        }

        // Cache the fields
        cache.putFieldCache(Person.class, fields);

        // Retrieve from cache
        var cachedFields = cache.getFieldCache(Person.class);
        System.out.println("\nCached field count: " + cachedFields.size());

        // Show cache statistics
        var stats = cache.getStats();
        System.out.println("Cache hits: " + stats.getTotalHits());

        // Type conversion examples
        System.out.println("\nType Conversions:");
        System.out.println("String → " + TypeConverter.javaToGraphQL(String.class));
        System.out.println("int → " + TypeConverter.javaToGraphQL(int.class));
        System.out.println("LocalDate → " +
            TypeConverter.javaToGraphQL(LocalDate.class));
        System.out.println("UUID → " + TypeConverter.javaToGraphQL(UUID.class));
    }
}
```

## Example 6: Validation and Testing

Comprehensive schema validation in a test context.

```java
import com.fraiseql.core.*;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class SchemaValidationTest {
    @BeforeEach
    public void setUp() {
        FraiseQL.clear();
        SchemaCache.getInstance().clear();
    }

    @Test
    public void testValidSchema() {
        @GraphQLType
        class User {
            @GraphQLField public int id;
            @GraphQLField public String name;
        }

        FraiseQL.registerType(User.class);
        FraiseQL.query("user")
            .returnType(User.class)
            .arg("id", "Int")
            .register();

        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(SchemaRegistry.getInstance());

        assertTrue(result.valid);
        assertTrue(result.errors.isEmpty());
    }

    @Test
    public void testInvalidReturnType() {
        FraiseQL.query("user")
            .returnType("UndefinedType")
            .arg("id", "Int")
            .register();

        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(SchemaRegistry.getInstance());

        assertFalse(result.valid);
        assertTrue(result.errors.stream()
            .anyMatch(e -> e.contains("undefined return type")));
    }

    @Test
    public void testEmptySchema() {
        SchemaValidator.ValidationResult result =
            SchemaValidator.validate(SchemaRegistry.getInstance());

        assertTrue(result.valid);
        assertTrue(result.warnings.stream()
            .anyMatch(w -> w.contains("No types")));
    }

    @Test
    public void testSchemaStatistics() {
        @GraphQLType
        class User { @GraphQLField public int id; }
        @GraphQLType
        class Post { @GraphQLField public int id; }

        FraiseQL.registerTypes(User.class, Post.class);
        FraiseQL.query("user").returnType(User.class).register();
        FraiseQL.query("post").returnType(Post.class).register();

        String stats = SchemaValidator.getStatistics(
            SchemaRegistry.getInstance());

        assertTrue(stats.contains("2 types"));
        assertTrue(stats.contains("2 queries"));
    }
}
```

## Example 7: Complete Integration

A complete example combining all features.

```java
import com.fraiseql.core.*;

public class CompleteExample {
    @GraphQLType
    public static class Product {
        @GraphQLField public int id;
        @GraphQLField public String name;
        @GraphQLField public String sku;
    }

    @GraphQLType
    public static class Order {
        @GraphQLField public int id;
        @GraphQLField public int customerId;
        @GraphQLField public String status;
    }

    public static void main(String[] args) throws Exception {
        // 1. Register types
        FraiseQL.registerTypes(Product.class, Order.class);

        // 2. Register queries with ArgumentBuilder
        ArgumentBuilder productArgs = new ArgumentBuilder()
            .add("id", "Int", null, "Product ID")
            .add("limit", "Int", 10, "Results limit")
            .add("offset", "Int", 0, "Pagination offset");

        FraiseQL.query("products")
            .returnType(Product.class)
            .returnsArray(true)
            .arg("limit", "Int")
            .arg("offset", "Int")
            .description("List all products")
            .register();

        FraiseQL.query("orders")
            .returnType(Order.class)
            .returnsArray(true)
            .arg("customerId", "Int")
            .description("Get customer orders")
            .register();

        // 3. Register mutations
        FraiseQL.mutation("createOrder")
            .returnType(Order.class)
            .arg("customerId", "Int")
            .arg("productIds", "String")
            .register();

        FraiseQL.mutation("updateOrderStatus")
            .returnType(Order.class)
            .arg("orderId", "Int")
            .arg("status", "String")
            .register();

        // 4. Validate schema
        SchemaRegistry registry = SchemaRegistry.getInstance();
        SchemaValidator.ValidationResult validation =
            SchemaValidator.validate(registry);

        if (!validation.valid) {
            validation.errors.forEach(System.err::println);
            System.exit(1);
        }

        // 5. Monitor performance
        PerformanceMonitor monitor = PerformanceMonitor.getInstance();
        long exportStart = System.currentTimeMillis();

        // 6. Export schema
        FraiseQL.exportSchemaToFile("complete-schema.json");

        long exportTime = System.currentTimeMillis() - exportStart;
        monitor.recordOperation("schemaExport", exportTime);

        // 7. Display results
        System.out.println("✓ Schema validation successful!");
        System.out.println(SchemaValidator.getStatistics(registry));
        System.out.println("\n" + monitor.generateReport());
        System.out.println("\n✓ Schema exported to complete-schema.json");
    }
}
```

## Running Examples

To run an example:

```bash
# 1. Add to your project
cp Example*.java src/main/java/com/example/

# 2. Compile
mvn clean compile

# 3. Run
mvn exec:java -Dexec.mainClass="com.example.BlogSchema"
```

## Next Steps

1. Study how types are defined with `@GraphQLType` and `@GraphQLField`
2. Learn query and mutation registration with `FraiseQL`
3. Explore validation with `SchemaValidator`
4. Monitor performance with `PerformanceMonitor`
5. Export schemas and compile with `fraiseql-cli`

For more details, see [API_GUIDE.md](API_GUIDE.md).
