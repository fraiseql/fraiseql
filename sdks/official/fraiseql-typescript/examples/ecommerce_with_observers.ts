/**
 * Example: E-commerce schema with observers.
 *
 * This example demonstrates the observer authoring API in FraiseQL v2.
 * Observers react to database changes (INSERT, UPDATE, DELETE) with
 * configurable actions like webhooks, Slack notifications, and emails.
 *
 * Usage:
 *   ts-node examples/ecommerce_with_observers.ts
 *
 * Output:
 *   ecommerce_schema.json (ready for fraiseql-cli compilation)
 */

import {
  Type,
  Observer,
  webhook,
  slack,
  email,
  exportSchema,
  RetryConfig,
} from "../src/index";
import type { ID, DateTime } from "../src/index";

// Define types
@Type()
class Order {
  /** E-commerce order */
  id!: ID;
  customer_email!: string;
  status!: string;
  total!: number;
  created_at!: DateTime;
}

@Type()
class Payment {
  /** Payment record */
  id!: ID;
  order_id!: ID;
  amount!: number;
  status!: string;
  processed_at?: DateTime;
}

// Observers defined as class methods (required for TypeScript decorators)
class OrderObservers {
  // Observer 1: Notify when high-value orders are created
  @Observer({
    entity: "Order",
    event: "INSERT",
    condition: "total > 1000",
    actions: [
      webhook("https://api.example.com/high-value-orders"),
      slack("#sales", "🎉 High-value order {id}: ${total}"),
      email(
        "sales@example.com",
        "High-value order {id}",
        "Order {id} for ${total} was created by {customer_email}"
      ),
    ],
  })
  onHighValueOrder() {
    /** Triggered when a high-value order is created */
  }

  // Observer 2: Notify when orders are shipped
  @Observer({
    entity: "Order",
    event: "UPDATE",
    condition: "status.changed() and status == 'shipped'",
    actions: [
      webhook(undefined, { url_env: "SHIPPING_WEBHOOK_URL" }),
      email(
        "{customer_email}",
        "Your order {id} has shipped!",
        "Your order is on its way. Track it here: https://example.com/track/{id}",
        { from_email: "noreply@example.com" }
      ),
    ],
  })
  onOrderShipped() {
    /** Triggered when an order status changes to 'shipped' */
  }

  // Observer 5: Simple notification for all new orders
  @Observer({
    entity: "Order",
    event: "INSERT",
    actions: [slack("#orders", "New order {id} by {customer_email}")],
  })
  onOrderCreated() {
    /** Triggered when any order is created */
  }

  // Observer 4: Archive deleted orders
  @Observer({
    entity: "Order",
    event: "DELETE",
    actions: [
      webhook("https://api.example.com/archive", {
        body_template: '{"type": "order", "id": "{{id}}", "data": {{_json}}}',
      }),
    ],
  })
  onOrderDeleted() {
    /** Triggered when an order is deleted */
  }
}

class PaymentObservers {
  // Observer 3: Alert on payment failures with aggressive retry
  @Observer({
    entity: "Payment",
    event: "UPDATE",
    condition: "status == 'failed'",
    actions: [
      slack("#payments", "⚠️ Payment failed for order {order_id}: {amount}"),
      webhook("https://api.example.com/payment-failures", {
        headers: { Authorization: "Bearer {PAYMENT_API_TOKEN}" },
      }),
    ],
    retry: {
      max_attempts: 5,
      backoff_strategy: "exponential",
      initial_delay_ms: 100,
      max_delay_ms: 60000,
    },
  })
  onPaymentFailure() {
    /** Triggered when a payment fails */
  }
}

// Reference classes to trigger decorator registration
void OrderObservers;
void PaymentObservers;

// Export schema with observers
if (require.main === module) {
  exportSchema("ecommerce_schema.json");

  console.log("\n🎯 Observer Summary:");
  console.log("   1. onHighValueOrder → Webhooks, Slack, Email for total > 1000");
  console.log("   2. onOrderShipped → Webhook + customer email when status='shipped'");
  console.log("   3. onPaymentFailure → Slack + webhook with retry on payment failures");
  console.log("   4. onOrderDeleted → Archive deleted orders via webhook");
  console.log("   5. onOrderCreated → Slack notification for all new orders");
  console.log("\n✨ Next steps:");
  console.log("   1. fraiseql-cli compile ecommerce_schema.json");
  console.log("   2. fraiseql-server --schema ecommerce_schema.compiled.json");
  console.log("   3. Observers will execute automatically on database changes!");
}
