package com.fraiseql.examples;

import com.fraiseql.core.*;

import java.io.IOException;
import java.util.HashMap;
import java.util.Map;

/**
 * E-commerce schema example demonstrating FraiseQL observer authoring.
 *
 * <p>This example shows how to define observers that react to database changes
 * with webhooks, Slack notifications, and emails.</p>
 *
 * <p>Run this to generate ecommerce_observers_schema.json:</p>
 * <pre>
 * java com.fraiseql.examples.EcommerceWithObserversSchema
 * </pre>
 */
public class EcommerceWithObserversSchema {
    public static void main(String[] args) throws IOException {
        // Define types
        @GraphQLType(name = "Order", description = "E-commerce order")
        class Order {
            @GraphQLField(type = "ID")
            public String id;

            @GraphQLField(type = "String")
            public String customerEmail;

            @GraphQLField(type = "String")
            public String status;

            @GraphQLField(type = "Float")
            public double total;

            @GraphQLField(type = "DateTime")
            public String createdAt;
        }

        @GraphQLType(name = "Payment", description = "Payment record")
        class Payment {
            @GraphQLField(type = "ID")
            public String id;

            @GraphQLField(type = "ID")
            public String orderId;

            @GraphQLField(type = "Float")
            public double amount;

            @GraphQLField(type = "String")
            public String status;

            @GraphQLField(type = "DateTime", nullable = true)
            public String processedAt;
        }

        // Register types
        FraiseQL.registerType(Order.class);
        FraiseQL.registerType(Payment.class);

        // NOTE: Observers are now configured in fraiseql.toml instead of Java code
        // See Phase 2 refactoring: TOML-based configuration reduces per-language scope
        //
        // Example fraiseql.toml configuration:
        //   [fraiseql.observers.onHighValueOrder]
        //   entity = "Order"
        //   event = "INSERT"
        //   condition = "total > 1000"
        //   actions = [
        //     { type = "webhook", url = "https://api.example.com/high-value-orders" },
        //     { type = "slack", channel = "#sales", message = "🎉 High-value order {id}: ${total}" }
        //   ]

        // Export minimal types (observers now configured in fraiseql.toml)
        FraiseQL.exportTypes("ecommerce_types.json");

        // Print summary
        System.out.println("\n✅ Types exported to ecommerce_types.json");
        System.out.println("   Types: " + FraiseQL.getRegistry().getAllTypes().size());

        System.out.println("\n🎯 TOML-based Workflow:");
        System.out.println("   1. Java generates: ecommerce_types.json (types only)");
        System.out.println("   2. Define observers in: fraiseql.toml [fraiseql.observers]");
        System.out.println("   3. Compile: fraiseql-cli compile fraiseql.toml --types ecommerce_types.json");
        System.out.println("   4. Result: schema.compiled.json with types + observer config");

        System.out.println("\n✨ Example fraiseql.toml configuration:");
        System.out.println("   [fraiseql.observers.onHighValueOrder]");
        System.out.println("   entity = \"Order\"");
        System.out.println("   event = \"INSERT\"");
        System.out.println("   condition = \"total > 1000\"");
        System.out.println("   actions = [");
        System.out.println("     { type = \"webhook\", url = \"https://api.example.com/high-value\" },");
        System.out.println("     { type = \"slack\", channel = \"#sales\", message = \"High-value order {id}\" }");
        System.out.println("   ]");
    }
}
