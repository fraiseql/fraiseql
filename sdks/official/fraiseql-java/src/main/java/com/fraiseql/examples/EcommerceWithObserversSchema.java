package com.fraiseql.examples;

import com.fraiseql.core.EmailAction;
import com.fraiseql.core.FraiseQL;
import com.fraiseql.core.GraphQLField;
import com.fraiseql.core.GraphQLType;
import com.fraiseql.core.ObserverBuilder;
import com.fraiseql.core.RetryConfig;
import com.fraiseql.core.SlackAction;
import com.fraiseql.core.Webhook;
import java.io.IOException;

/**
 * E-commerce schema example demonstrating FraiseQL observer authoring.
 *
 * <p>Observers react to database changes with webhooks, Slack notifications, and emails.
 * Templates use {@code {{ field }}} placeholders that name fields of the observed entity
 * by their stored key; the compiler refuses a webhook body template naming a field the
 * entity does not hold.</p>
 *
 * <p>The project's {@code fraiseql.toml} must enable the observer runtime, or the compiler
 * refuses the declarations:</p>
 * <pre>
 * [observers]
 * enabled = true
 * </pre>
 *
 * <p>Run this to generate ecommerce_observers_schema.json (or pass another path):</p>
 * <pre>
 * java com.fraiseql.examples.EcommerceWithObserversSchema
 * </pre>
 */
public class EcommerceWithObserversSchema {
    /**
     * Writes this example's schema.json, so the example can be run as a program.
     *
     * @param args optionally, the path to write the schema to
     */
    public static void main(String[] args) throws IOException {
        @GraphQLType(name = "Order", description = "E-commerce order", sqlSource = "v_order")
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

        @GraphQLType(name = "Payment", description = "Payment record", sqlSource = "v_payment")
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

        FraiseQL.registerType(Order.class);
        FraiseQL.registerType(Payment.class);

        // Observer 1: notify when high-value orders are created
        new ObserverBuilder("onHighValueOrder")
            .entity("Order")
            .event("INSERT")
            .condition("total > 1000")
            .addAction(Webhook.create("https://api.example.com/high-value-orders"))
            .addAction(SlackAction.create("#sales", "🎉 High-value order {{ id }}: ${{ total }}"))
            .addAction(EmailAction.create(
                "sales@example.com",
                "High-value order",
                "Order {{ id }} for ${{ total }} was created by {{ customer_email }}"))
            .register();

        // Observer 2: notify when orders are shipped
        new ObserverBuilder("onOrderShipped")
            .entity("Order")
            .event("UPDATE")
            .condition("field_changed_to('status', 'shipped')")
            .addAction(Webhook.signedWith(
                Webhook.withEnv("SHIPPING_WEBHOOK_URL"), "SHIPPING_WEBHOOK_SECRET"))
            .addAction(EmailAction.create(
                "fulfilment@example.com",
                "An order has shipped",
                "Order {{ id }} for {{ customer_email }} is on its way: https://example.com/track/{{ id }}"))
            .register();

        // Observer 3: alert on payment failures with aggressive retry
        new ObserverBuilder("onPaymentFailure")
            .entity("Payment")
            .event("UPDATE")
            .condition("status == 'failed'")
            .addAction(SlackAction.create(
                "#payments", "⚠️ Payment failed for order {{ order_id }}: {{ amount }}"))
            .addAction(Webhook.signedWith(
                Webhook.create("https://api.example.com/payment-failures"), "PAYMENT_WEBHOOK_SECRET"))
            .retry(RetryConfig.exponential(5, 100, 60_000))
            .register();

        // Observer 4: archive deleted orders
        new ObserverBuilder("onOrderDeleted")
            .entity("Order")
            .event("DELETE")
            .addAction(Webhook.bodyTemplate(
                Webhook.create("https://api.example.com/archive"),
                "{\"type\": \"order\", \"id\": \"{{ id }}\", \"status\": \"{{ status }}\"}"))
            .register();

        // Observer 5: simple notification for all new orders
        new ObserverBuilder("onOrderCreated")
            .entity("Order")
            .event("INSERT")
            .addAction(SlackAction.create("#orders", "New order {{ id }} by {{ customer_email }}"))
            .register();

        String output = args.length > 0 ? args[0] : "ecommerce_observers_schema.json";
        FraiseQL.exportSchema(output);

        System.out.println("\n✅ Schema exported to " + output);
        System.out.println("   Types: " + FraiseQL.getRegistry().getAllTypes().size());
        System.out.println("   Observers: " + FraiseQL.getRegistry().getAllObservers().size());

        System.out.println("\n✨ Next steps:");
        System.out.println("   1. Enable the observer runtime in fraiseql.toml:");
        System.out.println("        [observers]");
        System.out.println("        enabled = true");
        System.out.println("   2. fraiseql-cli compile " + output);
    }
}
