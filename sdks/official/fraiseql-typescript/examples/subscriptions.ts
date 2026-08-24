/**
 * FraiseQL Subscriptions Example
 *
 * This example demonstrates subscription support for real-time event streaming:
 * - Basic subscriptions on a return type
 * - Topic-based subscriptions for event channels
 * - Filter arguments mapped onto paths in the event payload
 * - Field projection from the event
 * - Common patterns: CDC, real-time notifications, alerts
 *
 * Subscriptions in FraiseQL are compiled database event projections,
 * sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * Two concepts this example used to demonstrate are gone (#1024). There is no `nullable`
 * and no `operation`/`operations`: the runtime subscription model has neither, and the
 * SDK emitted both into a struct that denies unknown fields, so a document declaring any
 * subscription was refused at compile — the whole document, not the subscription. Where
 * a CREATE/UPDATE/DELETE filter was wanted, the event's own payload carries the verb and
 * a `filter` condition selects on it.
 *
 * Usage:
 *   npx tsx examples/subscriptions.ts
 */

import * as fraiseql from "../src/index";

// ============================================================================
// TYPE DEFINITIONS
// ============================================================================

fraiseql.registerTypeFields(
  "User",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "email", type: "Email", nullable: false },
    { name: "name", type: "String", nullable: false },
    { name: "status", type: "String", nullable: false },
  ],
  undefined,
  { sqlSource: "v_user" }
);

fraiseql.registerTypeFields(
  "Order",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "customerId", type: "ID", nullable: false },
    { name: "status", type: "String", nullable: false },
    { name: "totalAmount", type: "Decimal", nullable: false },
    { name: "createdAt", type: "DateTime", nullable: false },
  ],
  undefined,
  { sqlSource: "v_order" }
);

fraiseql.registerTypeFields(
  "Payment",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "orderId", type: "ID", nullable: false },
    { name: "amount", type: "Decimal", nullable: false },
    { name: "status", type: "String", nullable: false },
    { name: "processedAt", type: "DateTime", nullable: false },
  ],
  undefined,
  { sqlSource: "v_payment" }
);

// ============================================================================
// EXAMPLE 1: Basic Subscriptions
// ============================================================================

// Subscribe to all user changes
fraiseql.registerSubscription(
  "userChanged",
  "User",
  [],
  "Subscribe to any user changes"
);

// The event payload carries the DML verb; a filter condition selects on it, which is
// what replaces the `operation: "CREATE"` this example used to pass.
fraiseql.registerSubscription(
  "userEventsByVerb",
  "User",
  [{ name: "verb", type: "String", nullable: true }],
  "Subscribe to user events of one kind",
  { filter: { conditions: [{ argument: "verb", path: "$.op" }] } }
);

// ============================================================================
// EXAMPLE 2: Topic-Based Subscriptions
// ============================================================================

fraiseql.registerSubscription(
  "orderEvents",
  "Order",
  [],
  "Subscribe to order events on the order_events topic",
  { topic: "order_events" }
);

fraiseql.registerSubscription(
  "orderLifecycle",
  "Order",
  [{ name: "status", type: "String", nullable: true }],
  "Track order lifecycle on a topic, narrowed by status",
  {
    topic: "orders",
    filter: { conditions: [{ argument: "status", path: "$.status" }] },
  }
);

// ============================================================================
// EXAMPLE 3: Filtered Subscriptions with Arguments
// ============================================================================

fraiseql.registerSubscription(
  "userUpdatesForId",
  "User",
  [{ name: "userId", type: "ID", nullable: false }],
  "Subscribe to updates for a specific user",
  { filter: { conditions: [{ argument: "userId", path: "$.id" }] } }
);

fraiseql.registerSubscription(
  "customerOrders",
  "Order",
  [{ name: "customerId", type: "ID", nullable: false }],
  "Subscribe to order changes for a specific customer",
  { filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] } }
);

// ============================================================================
// EXAMPLE 4: Projecting a Subset of the Event
// ============================================================================

// `fields` keeps the stream narrow: only these keys of the event are delivered.
fraiseql.registerSubscription(
  "orderTotals",
  "Order",
  [{ name: "customerId", type: "ID", nullable: true }],
  "Just the amounts, for a running total",
  {
    topic: "orders",
    filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] },
    fields: ["id", "totalAmount"],
  }
);

// ============================================================================
// EXAMPLE 5: Real-Time Notification Patterns
// ============================================================================

fraiseql.registerSubscription(
  "paymentProcessed",
  "Payment",
  [{ name: "status", type: "String", nullable: false }],
  "Real-time payment processing notifications",
  {
    topic: "payments",
    filter: { conditions: [{ argument: "status", path: "$.status" }] },
  }
);

fraiseql.registerSubscription(
  "orderStatusChanged",
  "Order",
  [
    { name: "orderId", type: "ID", nullable: false },
    { name: "toStatus", type: "String", nullable: true },
  ],
  "Get notified when order status changes",
  {
    filter: {
      conditions: [
        { argument: "orderId", path: "$.id" },
        { argument: "toStatus", path: "$.status" },
      ],
    },
  }
);

// ============================================================================
// EXAMPLE 6: Change Data Capture (CDC) Pattern
// ============================================================================

fraiseql.registerSubscription(
  "userCDC",
  "User",
  [],
  "Change data capture for users",
  { topic: "cdc" }
);

fraiseql.registerSubscription(
  "orderCDC",
  "Order",
  [],
  "Change data capture for orders",
  { topic: "cdc" }
);

// ============================================================================
// EXAMPLE 7: Multi-Topic Fan-Out Pattern
// ============================================================================

fraiseql.registerSubscription(
  "criticalOrders",
  "Order",
  [{ name: "customerId", type: "ID", nullable: true }],
  "High-priority orders",
  {
    topic: "orders.critical",
    filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] },
  }
);

fraiseql.registerSubscription("standardOrders", "Order", [], "Standard orders", {
  topic: "orders.standard",
});

// A subscription that is on its way out says so, and generated clients warn.
fraiseql.registerSubscription(
  "lowPriorityOrders",
  "Order",
  [],
  "Low-priority orders",
  { topic: "orders.low_priority", deprecated: "fold into standardOrders" }
);

// ============================================================================
// EXAMPLE 8: Queries Complementing Subscriptions
// ============================================================================

fraiseql.registerQuery(
  "getUser",
  "User",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get user by ID (works with userUpdatesForId subscription)",
  { sql_source: "v_user" }
);

fraiseql.registerQuery(
  "getOrder",
  "Order",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get order by ID (complements order subscriptions)",
  { sql_source: "v_order" }
);

fraiseql.registerQuery(
  "customerOrderHistory",
  "Order",
  true,
  false,
  [{ name: "customerId", type: "ID", nullable: false }],
  "Get all orders for customer (backfill before subscribing)",
  { sql_source: "v_order" }
);

// ============================================================================
// MUTATIONS
// ============================================================================

fraiseql.registerMutation(
  "createOrder",
  "Order",
  false,
  false,
  [
    { name: "customerId", type: "ID", nullable: false },
    { name: "amount", type: "Decimal", nullable: false },
  ],
  "Create order (will trigger orderEvents subscription)",
  { sql_source: "fn_create_order", operation: "insert" }
);

fraiseql.registerMutation(
  "updateOrderStatus",
  "Order",
  false,
  false,
  [
    { name: "orderId", type: "ID", nullable: false },
    { name: "status", type: "String", nullable: false },
  ],
  "Update order status (will trigger relevant subscriptions)",
  { sql_source: "fn_update_order_status", operation: "update" }
);

// ============================================================================
// EXPORT SCHEMA
// ============================================================================

fraiseql.exportSchema("schema.json");

const subscriptions = fraiseql.SchemaRegistry.getSchema().subscriptions;
console.log(`Registered ${subscriptions.length} subscription(s):`);
for (const subscription of subscriptions) {
  const topic = subscription.topic ? `  topic=${subscription.topic}` : "";
  const conditions = subscription.filter?.conditions.length ?? 0;
  const filter = conditions > 0 ? `  filter=${conditions} condition(s)` : "";
  console.log(`   ${subscription.name}  ${subscription.return_type}${topic}${filter}`);
}
