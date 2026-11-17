/**
 * Example: E-commerce schema with observers.
 *
 * This example demonstrates the observer authoring API in FraiseQL v2.
 * Observers react to database changes (INSERT, UPDATE, DELETE) with
 * configurable actions like webhooks, Slack notifications, and emails.
 *
 * Usage:
 *   npx tsx examples/ecommerce_with_observers.ts
 *   fraiseql-cli compile ecommerce_with_observers_schema.json
 *
 * The project's `fraiseql.toml` must enable the observer runtime, or the compiler
 * refuses the declarations (no runtime would start, and none of them would fire):
 *
 *   [observers]
 *   enabled = true
 *
 * Templates use `{{ field }}` placeholders that name fields of the observed entity by
 * their stored key. The compiler refuses a webhook `body_template` naming a field the
 * entity does not hold.
 */

import {
  Observer,
  SchemaRegistry,
  email,
  exportSchema,
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
      slack("#sales", "🎉 High-value order {{ id }}: ${{ total }}"),
      email(
        "sales@example.com",
        "High-value order",
        "Order {{ id }} for ${{ total }} was created by {{ customer_email }}"
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
    condition: "field_changed_to('status', 'shipped')",
    actions: [
      webhook(undefined, {
        url_env: "SHIPPING_WEBHOOK_URL",
        signing_secret_env: "SHIPPING_WEBHOOK_SECRET",
      }),
      email(
        "fulfilment@example.com",
        "An order has shipped",
        "Order {{ id }} for {{ customer_email }} is on its way: https://example.com/track/{{ id }}"
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
    actions: [slack("#orders", "New order {{ id }} by {{ customer_email }}")],
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
        body_template: '{"type": "order", "id": "{{ id }}", "status": "{{ status }}"}',
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
      slack("#payments", "⚠️ Payment failed for order {{ order_id }}: {{ amount }}"),
      webhook("https://api.example.com/payment-failures", {
        headers: { "X-Source": "fraiseql" },
        signing_secret_env: "PAYMENT_WEBHOOK_SECRET",
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

exportSchema("ecommerce_with_observers_schema.json");

const observers = SchemaRegistry.getSchema().observers ?? [];
console.log(`\nRegistered ${observers.length} observer(s):`);
for (const observer of observers) {
  console.log(`   ${observer.name}  ${observer.entity} ${observer.event}`);
}
