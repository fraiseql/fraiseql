/**
 * FraiseQL Enum Example
 *
 * Enums define fixed sets of allowed values.
 * They are compile-time only and generate GraphQL enum types in the schema.
 *
 * This example shows:
 * - Basic enum definition
 * - Enum with description
 * - Using enums as field types
 * - Enums in query parameters
 */

import * as fraiseql from "../src/index";

// ============================================================================
// Define Enums
// ============================================================================

// Simple enum: Order status
const OrderStatus = fraiseql.enum_("OrderStatus", {
  PENDING: "pending",
  SHIPPED: "shipped",
  DELIVERED: "delivered",
  CANCELLED: "cancelled",
});

// Enum with description
const Priority = fraiseql.enum_(
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
// Define Types Using Enums
// ============================================================================

@fraiseql.Type()
class Order {
  id!: string;
  status!: string; // Will be OrderStatus enum
  createdAt!: string;
}

fraiseql.registerTypeFields("Order", [
  { name: "id", type: "ID", nullable: false },
  { name: "status", type: "OrderStatus", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
]);

@fraiseql.Type()
class Task {
  id!: string;
  title!: string;
  priority!: string; // Will be Priority enum
  completed!: boolean;
}

fraiseql.registerTypeFields("Task", [
  { name: "id", type: "ID", nullable: false },
  { name: "title", type: "String", nullable: false },
  { name: "priority", type: "Priority", nullable: false },
  { name: "completed", type: "Boolean", nullable: false },
]);

// ============================================================================
// Define Input Types with Enums
// ============================================================================

const OrderFilter = fraiseql.input("OrderFilter", [
  { name: "status", type: "OrderStatus", nullable: true },
]);

const TaskFilter = fraiseql.input("TaskFilter", [
  { name: "priority", type: "Priority", nullable: true },
  { name: "completed", type: "Boolean", nullable: true },
]);

// ============================================================================
// Queries with Enum Parameters
// ============================================================================

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
  "Get order by ID"
);

@fraiseql.Query({ sqlSource: "v_order" })
function listOrders(status?: string): Order[] {
  pass;
}

fraiseql.registerQuery(
  "listOrders",
  "Order",
  true,
  false,
  [{ name: "status", type: "OrderStatus", nullable: true }],
  "List orders, optionally filtered by status"
);

@fraiseql.Query({ sqlSource: "v_task" })
function listTasks(filter?: Record<string, unknown>): Task[] {
  pass;
}

fraiseql.registerQuery(
  "listTasks",
  "Task",
  true,
  false,
  [{ name: "filter", type: "TaskFilter", nullable: true }],
  "List tasks with optional filtering"
);

// ============================================================================
// Mutations with Enums
// ============================================================================

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
    { name: "status", type: "OrderStatus", nullable: false },
  ],
  "Update order status"
);

@fraiseql.Mutation({ sqlSource: "fn_create_task", operation: "CREATE" })
function createTask(title: string, priority: string): Task {
  pass;
}

fraiseql.registerMutation(
  "createTask",
  "Task",
  false,
  false,
  [
    { name: "title", type: "String", nullable: false },
    { name: "priority", type: "Priority", nullable: false },
  ],
  "Create a new task with priority"
);

// ============================================================================
// Export Schema
// ============================================================================

if (require.main === module) {
  fraiseql.exportSchema("schema.json");
  console.log("✅ Schema exported to schema.json");
  console.log("  Enums: OrderStatus, Priority");
  console.log("  Types: Order, Task");
  console.log("  Input Types: OrderFilter, TaskFilter");
}
