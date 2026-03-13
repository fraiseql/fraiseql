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
 */

import * as fraiseql from "../src/index";

// ============================================================================
// TYPE DEFINITIONS
// ============================================================================

@fraiseql.Type()
class User {
  id!: string;
  email!: string;
  name!: string;
  status!: string;
}

fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "status", type: "String", nullable: false },
]);

@fraiseql.Type()
class Order {
  id!: string;
  customerId!: string;
  status!: string;
  totalAmount!: number;
  createdAt!: string;
}

fraiseql.registerTypeFields("Order", [
  { name: "id", type: "ID", nullable: false },
  { name: "customerId", type: "ID", nullable: false },
  { name: "status", type: "String", nullable: false },
  { name: "totalAmount", type: "Decimal", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
]);

@fraiseql.Type()
class Payment {
  id!: string;
  orderId!: string;
  amount!: number;
  status!: string;
  processedAt!: string;
}

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

@fraiseql.Query({ sqlSource: "v_user" })
function getUser(id: string): User {
  pass;
}

fraiseql.registerQuery(
  "getUser",
  "User",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get user by ID (works with userUpdatesForId subscription)"
);

@fraiseql.Query({ sqlSource: "v_order" })
function getOrder(id: string): Order {
  pass;
}

fraiseql.registerQuery(
  "getOrder",
  "Order",
  false,
  false,
  [{ name: "id", type: "ID", nullable: false }],
  "Get order by ID (complements order subscriptions)"
);

@fraiseql.Query({ sqlSource: "v_order" })
function customerOrders(customerId: string): Order[] {
  pass;
}

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

@fraiseql.Mutation({ sqlSource: "fn_create_order", operation: "CREATE" })
function createOrder(customerId: string, amount: number): Order {
  pass;
}

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

@fraiseql.Mutation({ sqlSource: "fn_update_order", operation: "UPDATE" })
function updateOrderStatus(orderId: string, status: string): Order {
  pass;
}

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

if (require.main === module) {
  fraiseql.exportSchema("schema.json");
  console.log("✅ Schema exported to schema.json");
  console.log("  ");
  console.log("  Event Type Filtering:");
  console.log("    ✓ userCreated (operation: CREATE)");
  console.log("    ✓ userUpdated (operation: UPDATE)");
  console.log("    ✓ userDeleted (operation: DELETE)");
  console.log("  ");
  console.log("  Topic-Based:");
  console.log("    ✓ orderEvents (topic: order_events)");
  console.log("    ✓ orderLifecycle (topic: orders, ops: CREATE/UPDATE/DELETE)");
  console.log("  ");
  console.log("  Filtered Subscriptions:");
  console.log("    ✓ customerOrders (customerId filter)");
  console.log("    ✓ expensiveOrders (minAmount, maxAmount filters)");
  console.log("  ");
  console.log("  Patterns Demonstrated:");
  console.log("    ✓ Real-time notifications (paymentProcessed)");
  console.log("    ✓ Change Data Capture (userCDC, orderCDC)");
  console.log("    ✓ Alerts (unusualOrders, userStatusAlert)");
  console.log("    ✓ Fan-out routing (criticalOrders, standardOrders)");
}
