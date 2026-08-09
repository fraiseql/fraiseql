/**
 * Enums and input types.
 *
 * Enums are compile-time only: they become GraphQL enum types and constrain the values
 * a field or argument accepts. Input types group arguments into one object.
 *
 * This example shows:
 * - a bare enum, and one carrying a description
 * - using an enum as a field type and as an argument type
 * - input types built from enum-typed fields
 *
 * Usage:
 *   npx tsx examples/enums-example.ts
 *   fraiseql-cli compile schema.json
 */

import {
  enum_,
  exportSchema,
  input,
  registerMutation,
  registerQuery,
  registerTypeFields,
} from "../src/index";

// ============================================================================
// Enums
// ============================================================================

enum_("OrderStatus", {
  PENDING: "pending",
  SHIPPED: "shipped",
  DELIVERED: "delivered",
  CANCELLED: "cancelled",
});

enum_(
  "Priority",
  {
    LOW: "low",
    MEDIUM: "medium",
    HIGH: "high",
    CRITICAL: "critical",
  },
  { description: "Priority level for tasks and issues" }
);

// ============================================================================
// Types using those enums
// ============================================================================

// A field's `type` is the enum's *name*. The compiler resolves it against the enums
// declared in the same document and emits `FieldType::Enum` — before #923 it fell
// through to `Object`, and introspection told clients a scalar enum was an object.
registerTypeFields(
  "Order",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "status", type: "OrderStatus", nullable: false },
    { name: "createdAt", type: "DateTime", nullable: false },
  ],
  "A customer order",
  { sqlSource: "v_order" }
);

registerTypeFields(
  "Task",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "title", type: "String", nullable: false },
    { name: "priority", type: "Priority", nullable: false },
    { name: "completed", type: "Boolean", nullable: false },
  ],
  "A task with a priority",
  { sqlSource: "v_task" }
);

// ============================================================================
// Input types
// ============================================================================

input("OrderFilter", [{ name: "status", type: "OrderStatus", nullable: true }]);

input("TaskFilter", [
  { name: "priority", type: "Priority", nullable: true },
  { name: "completed", type: "Boolean", nullable: true },
]);

// ============================================================================
// Queries and mutations taking enum arguments
// ============================================================================

registerQuery(
  "order",
  "Order",
  false,
  true,
  [{ name: "id", type: "ID", nullable: false }],
  "Fetch one order by id",
  { sql_source: "v_order" }
);

registerQuery(
  "orders",
  "Order",
  true,
  false,
  [{ name: "status", type: "OrderStatus", nullable: true }],
  "List orders, optionally filtered by status",
  { sql_source: "v_order" }
);

registerQuery(
  "tasks",
  "Task",
  true,
  false,
  [{ name: "filter", type: "TaskFilter", nullable: true }],
  "List tasks, filtered by an input object",
  { sql_source: "v_task" }
);

registerMutation(
  "updateOrderStatus",
  "Order",
  false,
  false,
  [
    { name: "orderId", type: "ID", nullable: false },
    { name: "status", type: "OrderStatus", nullable: false },
  ],
  "Move an order to a new status",
  { sql_source: "fn_update_order", operation: "update", invalidates_views: ["v_order"] }
);

registerMutation(
  "createTask",
  "Task",
  false,
  false,
  [
    { name: "title", type: "String", nullable: false },
    { name: "priority", type: "Priority", nullable: false },
  ],
  "Create a task at a given priority",
  { sql_source: "fn_create_task", operation: "insert", invalidates_views: ["v_task"] }
);

// ============================================================================
// Export
// ============================================================================

exportSchema("schema.json");
console.log("   Enums:       OrderStatus, Priority");
console.log("   Input types: OrderFilter, TaskFilter");
