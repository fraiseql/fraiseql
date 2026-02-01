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

        // Export schema
        FraiseQL.exportSchema("ecommerce_observers_schema.json");

        // Print summary
        System.out.println("\n✅ Schema exported to ecommerce_observers_schema.json");
        System.out.println("   Types: " + FraiseQL.getRegistry().getAllTypes().size());
        System.out.println("   Observers: " + FraiseQL.getRegistry().getAllObservers().size());

        System.out.println("\n🎯 Observer Summary:");
        System.out.println("   1. onHighValueOrder → Webhooks, Slack, Email for total > 1000");
        System.out.println("   2. onOrderShipped → Webhook + customer email when status='shipped'");
        System.out.println("   3. onPaymentFailure → Slack + webhook with retry on payment failures");
        System.out.println("   4. onOrderDeleted → Archive deleted orders via webhook");
        System.out.println("   5. onOrderCreated → Slack notification for all new orders");

        System.out.println("\n✨ Next steps:");
        System.out.println("   1. fraiseql-cli compile ecommerce_observers_schema.json");
        System.out.println("   2. fraiseql-server --schema ecommerce_observers_schema.compiled.json");
        System.out.println("   3. Observers will execute automatically on database changes!");
    }
}
