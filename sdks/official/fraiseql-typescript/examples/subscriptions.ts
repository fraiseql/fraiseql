/**
 * FraiseQL Subscriptions Example
 *
 * This example demonstrates subscription support for real-time event streaming:
 * - Basic subscriptions with entity type filtering
 * - Event type filtering (CREATE, UPDATE, DELETE)
 * - Topic-based subscriptions for event channels
 * - Filter arguments for targeted subscriptions
 * - Common patterns: CDC, real-time notifications, alerts
 *
 * Subscriptions in FraiseQL are compiled database event projections,
 * sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * **This example does not write a schema file, and cannot yet.** Every SDK emits a
 * subscription as `{name, entity_type, nullable, …}` while `IntermediateSubscription`
 * expects `{name, return_type, topic, filter, fields, …}`, and that struct is
 * `deny_unknown_fields` — so a document declaring any subscription is refused at
 * compile, in every language (#1024). The registrations below are real and are printed;
 * writing a `schema.json` here would only produce a file the next command rejects.
 *
 * Usage:
 *   npx tsx examples/subscriptions.ts
 */

import * as fraiseql from "../src/index";

// ============================================================================
// TYPE DEFINITIONS
// ============================================================================

fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "status", type: "String", nullable: false },
]);

fraiseql.registerTypeFields("Order", [
  { name: "id", type: "ID", nullable: false },
  { name: "customerId", type: "ID", nullable: false },
  { name: "status", type: "String", nullable: false },
  { name: "totalAmount", type: "Decimal", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
]);

fraiseql.registerTypeFields("Payment", [
  { name: "id", type: "ID", nullable: false },
  { name: "orderId", type: "ID", nullable: false },
  { name: "amount", type: "Decimal", nullable: false },
  { name: "status", type: "String", nullable: false },
  { name: "processedAt", type: "DateTime", nullable: false },
]);

// ============================================================================
// EXAMPLE 1: Basic Event Type Filtering
// ============================================================================

// Subscribe to all user changes
fraiseql.registerSubscription(
  "userChanged",
  "User",
  false,
  [],
  "Subscribe to any user changes (create, update, delete)"
);

// Subscribe only to user creation
fraiseql.registerSubscription(
  "userCreated",
  "User",
  false,
  [],
  "Subscribe to new user registrations",
  { operation: "CREATE" }
);

// Subscribe only to user updates
fraiseql.registerSubscription(
  "userUpdated",
  "User",
  false,
  [],
  "Subscribe to user profile updates",
  { operation: "UPDATE" }
);

// Subscribe only to user deletion
fraiseql.registerSubscription(
  "userDeleted",
  "User",
  false,
  [],
  "Subscribe to user deletions",
  { operation: "DELETE" }
);

// ============================================================================
// EXAMPLE 2: Topic-Based Subscriptions
// ============================================================================

// Topic-based filtering for order events
fraiseql.registerSubscription(
  "orderEvents",
  "Order",
  false,
  [],
  "Subscribe to order events on order_events topic",
  { topic: "order_events" }
);

// Topic with operation filtering
fraiseql.registerSubscription(
  "newOrdersStream",
  "Order",
  false,
  [],
  "Stream of new orders",
  { topic: "orders", operation: "CREATE" }
);

// Topic with multiple operations
fraiseql.registerSubscription(
  "orderLifecycle",
  "Order",
  false,
  [],
  "Track full order lifecycle",
  { topic: "orders", operations: ["CREATE", "UPDATE", "DELETE"] }
);

// ============================================================================
// EXAMPLE 3: Filtered Subscriptions with Arguments
// ============================================================================

// Subscribe to changes for a specific user
fraiseql.registerSubscription(
  "userUpdatesForId",
  "User",
  false,
  [{ name: "userId", type: "ID", nullable: false }],
  "Subscribe to updates for a specific user",
  { operation: "UPDATE" }
);

// Subscribe to orders for a customer
fraiseql.registerSubscription(
  "customerOrders",
  "Order",
  false,
  [{ name: "customerId", type: "ID", nullable: false }],
  "Subscribe to order changes for a specific customer"
);

// Subscribe to high-value orders
fraiseql.registerSubscription(
  "expensiveOrders",
  "Order",
  false,
  [
    { name: "minAmount", type: "Decimal", nullable: false },
    { name: "maxAmount", type: "Decimal", nullable: true },
  ],
  "Subscribe to orders above a minimum amount",
  { operation: "CREATE" }
);

// ============================================================================
// EXAMPLE 4: Real-Time Notification Patterns
// ============================================================================

// Real-time payment processing
fraiseql.registerSubscription(
  "paymentProcessed",
  "Payment",
  false,
  [
    { name: "status", type: "String", nullable: false },
    { name: "minAmount", type: "Decimal", nullable: true },
  ],
  "Real-time payment processing notifications",
  { topic: "payments", operation: "UPDATE" }
);

// Real-time order status updates
fraiseql.registerSubscription(
  "orderStatusChanged",
  "Order",
  false,
  [
    { name: "orderId", type: "ID", nullable: false },
    { name: "fromStatus", type: "String", nullable: true },
    { name: "toStatus", type: "String", nullable: true },
  ],
  "Get notified when order status changes",
  { operation: "UPDATE" }
);

// ============================================================================
// EXAMPLE 5: Change Data Capture (CDC) Pattern
// ============================================================================

// Capture all user changes for data synchronization
fraiseql.registerSubscription(
  "userCDC",
  "User",
  false,
  [],
  "Change data capture for users (all operations)",
  { topic: "cdc", operations: ["CREATE", "UPDATE", "DELETE"] }
);

// Capture all order changes for audit trail
fraiseql.registerSubscription(
  "orderCDC",
  "Order",
  false,
  [],
  "Change data capture for orders (all operations)",
  { topic: "cdc", operations: ["CREATE", "UPDATE", "DELETE"] }
);

// ============================================================================
// EXAMPLE 6: Alert Pattern with Filters
// ============================================================================

// Alert on unusual activity
fraiseql.registerSubscription(
  "unusualOrders",
  "Order",
  false,
  [
    { name: "minAmount", type: "Decimal", nullable: false },
    { name: "timeWindowMinutes", type: "Int", nullable: true },
  ],
  "Alert on orders above threshold within time window",
  { operation: "CREATE" }
);

// Alert on user status changes
fraiseql.registerSubscription(
  "userStatusAlert",
  "User",
  false,
  [
    { name: "fromStatus", type: "String", nullable: false },
    { name: "toStatus", type: "String", nullable: false },
  ],
  "Alert when user status transitions",
  { operation: "UPDATE" }
);

// ============================================================================
// EXAMPLE 7: Multi-Topic Fan-Out Pattern
// ============================================================================

// Different channels for different priorities
fraiseql.registerSubscription(
  "criticalOrders",
  "Order",
  false,
  [{ name: "minAmount", type: "Decimal", nullable: false }],
  "High-priority orders",
  { topic: "orders.critical", operation: "CREATE" }
);

fraiseql.registerSubscription(
  "standardOrders",
  "Order",
  false,
  [],
  "Standard orders",
  { topic: "orders.standard", operation: "CREATE" }
);

fraiseql.registerSubscription(
  "lowPriorityOrders",
  "Order",
  false,
  [],
  "Low-priority orders",
  { topic: "orders.low_priority", operation: "CREATE" }
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
  "Get user by ID (works with userUpdatesForId subscription)"
);

fraiseql.registerQuery(
  "getOrder",
  "Order",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get order by ID (complements order subscriptions)"
);

fraiseql.registerQuery(
  "customerOrders",
  "Order",
  true,
  false,
  [{ name: "customerId", type: "ID", nullable: false }],
  "Get all orders for customer (backfill before subscribing)"
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
  "Create order (will trigger orderCreated subscription)"
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
  "Update order status (will trigger relevant subscriptions)"
);

// ============================================================================
// EXPORT SCHEMA
// ============================================================================

const subscriptions = fraiseql.SchemaRegistry.getSchema().subscriptions;
console.log(`Registered ${subscriptions.length} subscription(s):`);
for (const subscription of subscriptions) {
  const topic = subscription.topic ? `  topic=${subscription.topic}` : "";
  const operation = subscription.operation ? `  op=${subscription.operation}` : "";
  console.log(`   ${subscription.name}  ${subscription.entity_type}${operation}${topic}`);
}
console.log("");
console.log("Not exported: `entity_type` is not a key the compiler accepts — see #1024.");
