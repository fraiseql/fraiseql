/**
 * Example: E-commerce schema with observers.
 *
 * This example demonstrates the observer authoring API in FraiseQL v2.
 * Observers react to database changes (INSERT, UPDATE, DELETE) with
 * configurable actions like webhooks, Slack notifications, and emails.
 *
 * Usage:
 *   npx tsx examples/ecommerce_with_observers.ts
 *
 * Note: this example does not write a schema file. `fraiseql compile` refuses a schema
 * that declares observers (#779) — observers are configured for the running server, not
 * lowered into the compiled artifact — so writing one here would produce a document the
 * very next command rejects. The registry contents are printed instead.
 */

import {
  Observer,
  SchemaRegistry,
  email,
  registerTypeFields,
  slack,
  webhook,
} from "../src/index";

// The entities the observers watch. Declared with `registerTypeFields` rather than
// `@Type()`: TypeScript erases the field types a class decorator would need, so `@Type()`
// records only the name and the export is refused (#733). `@Observer` below is a *method*
// decorator carrying its own config object, so it needs nothing erased — and it now reads
// the member name under both the legacy and the TC39 decorator protocols (#925).
registerTypeFields(
  "Order",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "customer_email", type: "String", nullable: false },
    { name: "status", type: "String", nullable: false },
    { name: "total", type: "Float", nullable: false },
    { name: "created_at", type: "DateTime", nullable: false },
  ],
  "E-commerce order",
  { sqlSource: "v_order" }
);

registerTypeFields(
  "Payment",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "order_id", type: "ID", nullable: false },
    { name: "amount", type: "Float", nullable: false },
    { name: "status", type: "String", nullable: false },
    { name: "processed_at", type: "DateTime", nullable: true },
  ],
  "Payment record",
  { sqlSource: "v_payment" }
);

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

// Show what was registered. Every observer's `name` must be a plain string here: when
// it was the decorator's context object, this printed `[object Object]` and the schema
// it produced was rejected by the compiler with `invalid type: map, expected a string`.
const observers = SchemaRegistry.getSchema().observers ?? [];
console.log(`Registered ${observers.length} observer(s):`);
for (const observer of observers) {
  console.log(`   ${observer.name}  ${observer.entity} ${observer.event}`);
}
