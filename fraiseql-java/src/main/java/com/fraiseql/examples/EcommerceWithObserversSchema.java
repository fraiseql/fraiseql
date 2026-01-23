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

        // Observer 1: Notify when high-value orders are created
        new ObserverBuilder("onHighValueOrder")
            .entity("Order")
            .event("INSERT")
            .condition("total > 1000")
            .addAction(Webhook.create("https://api.example.com/high-value-orders"))
            .addAction(SlackAction.create("#sales", "🎉 High-value order {id}: ${total}"))
            .addAction(EmailAction.create(
                "sales@example.com",
                "High-value order {id}",
                "Order {id} for ${total} was created by {customer_email}"
            ))
            .register();

        // Observer 2: Notify when orders are shipped
        Map<String, Object> webhookOpts = new HashMap<>();
        webhookOpts.put("url_env", "SHIPPING_WEBHOOK_URL");

        new ObserverBuilder("onOrderShipped")
            .entity("Order")
            .event("UPDATE")
            .condition("status.changed() and status == 'shipped'")
            .addAction(Webhook.withEnv("SHIPPING_WEBHOOK_URL"))
            .addAction(EmailAction.withFrom(
                "{customer_email}",
                "Your order {id} has shipped!",
                "Your order is on its way. Track it here: https://example.com/track/{id}",
                "noreply@example.com"
            ))
            .register();

        // Observer 3: Alert on payment failures with aggressive retry
        Map<String, Object> headers = new HashMap<>();
        headers.put("Authorization", "Bearer {PAYMENT_API_TOKEN}");

        Map<String, Object> webhookWithHeaders = Webhook.create("https://api.example.com/payment-failures");
        webhookWithHeaders.put("headers", headers);

        new ObserverBuilder("onPaymentFailure")
            .entity("Payment")
            .event("UPDATE")
            .condition("status == 'failed'")
            .addAction(SlackAction.create("#payments", "⚠️ Payment failed for order {order_id}: {amount}"))
            .addAction(webhookWithHeaders)
            .retry(RetryConfig.exponential(5, 100, 60000))
            .register();

        // Observer 4: Archive deleted orders
        Map<String, Object> bodyTemplate = new HashMap<>();
        bodyTemplate.put("body_template", "{\"type\": \"order\", \"id\": \"{{id}}\", \"data\": {{_json}}}");

        Map<String, Object> archiveWebhook = Webhook.create("https://api.example.com/archive");
        archiveWebhook.putAll(bodyTemplate);

        new ObserverBuilder("onOrderDeleted")
            .entity("Order")
            .event("DELETE")
            .addAction(archiveWebhook)
            .register();

        // Observer 5: Simple notification for all new orders
        new ObserverBuilder("onOrderCreated")
            .entity("Order")
            .event("INSERT")
            .addAction(SlackAction.create("#orders", "New order {id} by {customer_email}"))
            .register();

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
