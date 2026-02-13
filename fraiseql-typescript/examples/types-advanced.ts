/**
 * FraiseQL Type System Example: Enums, Interfaces, and Unions
 *
 * This example demonstrates the complete FraiseQL type system:
 * - Enums: Fixed sets of allowed values
 * - Interfaces: Shared field definitions across types
 * - Unions: Polymorphic types (multiple alternatives)
 * - Input types: Structured input parameters
 *
 * All decorators are compile-time only. They generate schema.json
 * for the Rust compiler, with zero runtime overhead.
 */

import * as fraiseql from "../src/index";

// ============================================================================
// ENUMS - Fixed sets of allowed values
// ============================================================================

// Define enum for order status
const OrderStatus = fraiseql.enum_("OrderStatus", {
  PENDING: "pending",
  PROCESSING: "processing",
  SHIPPED: "shipped",
  DELIVERED: "delivered",
  CANCELLED: "cancelled",
});

// Define enum for payment methods
const PaymentMethod = fraiseql.enum_("PaymentMethod", {
  CREDIT_CARD: "credit_card",
  DEBIT_CARD: "debit_card",
  BANK_TRANSFER: "bank_transfer",
  PAYPAL: "paypal",
});

// Define enum for user roles with description
const UserRole = fraiseql.enum_(
  "UserRole",
  {
    ADMIN: "admin",
    MODERATOR: "moderator",
    USER: "user",
  },
  { description: "Role-based access control for users" }
);

// ============================================================================
// INTERFACES - Shared field definitions
// ============================================================================

// Define Node interface (globally unique ID)
const Node = fraiseql.interface_("Node", [
  { name: "id", type: "ID", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Define Auditable interface (who changed it and when)
const Auditable = fraiseql.interface_("Auditable", [
  { name: "createdBy", type: "String", nullable: false },
  { name: "updatedBy", type: "String", nullable: true },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// ============================================================================
// OBJECT TYPES - Using interfaces
// ============================================================================

// User type implements Node interface
@fraiseql.Type()
class User {
  id!: string;
  email!: string;
  name!: string;
  role!: string; // Will be cast to UserRole enum
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "role", type: "UserRole", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Order type with status enum
@fraiseql.Type()
class Order {
  id!: string;
  userId!: string;
  totalAmount!: number;
  status!: string; // Will be cast to OrderStatus enum
  paymentMethod!: string; // Will be cast to PaymentMethod enum
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("Order", [
  { name: "id", type: "ID", nullable: false },
  { name: "userId", type: "ID", nullable: false },
  { name: "totalAmount", type: "Decimal", nullable: false },
  { name: "status", type: "OrderStatus", nullable: false },
  { name: "paymentMethod", type: "PaymentMethod", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// Post type with description
@fraiseql.Type()
class Post {
  id!: string;
  authorId!: string;
  title!: string;
  content!: string;
  published!: boolean;
  createdAt!: string;
  updatedAt!: string;
}

fraiseql.registerTypeFields("Post", [
  { name: "id", type: "ID", nullable: false },
  { name: "authorId", type: "ID", nullable: false },
  { name: "title", type: "String", nullable: false },
  { name: "content", type: "String", nullable: false },
  { name: "published", type: "Boolean", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "updatedAt", type: "DateTime", nullable: false },
]);

// ============================================================================
// UNIONS - Polymorphic types (search results)
// ============================================================================

// Define union for search results
const SearchResult = fraiseql.union("SearchResult", ["User", "Post", "Order"], {
  description: "Result of a global search query",
});

// ============================================================================
// INPUT TYPES - Structured parameters
// ============================================================================

// Input for creating users
const CreateUserInput = fraiseql.input("CreateUserInput", [
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "role", type: "UserRole", nullable: false, default: "USER" },
]);

// Input for filtering orders
const OrderFilter = fraiseql.input("OrderFilter", [
  { name: "status", type: "OrderStatus", nullable: true },
  { name: "minAmount", type: "Decimal", nullable: true },
  { name: "maxAmount", type: "Decimal", nullable: true },
  { name: "paymentMethod", type: "PaymentMethod", nullable: true },
]);

// Input for search
const SearchInput = fraiseql.input("SearchInput", [
  { name: "query", type: "String", nullable: false },
  { name: "limit", type: "Int", nullable: false, default: 10 },
  { name: "offset", type: "Int", nullable: false, default: 0 },
]);

// ============================================================================
// QUERIES - Using enums, interfaces, and unions
// ============================================================================

@fraiseql.Query({ sqlSource: "v_user" })
function getUser(userId: string): User {
  pass;
}

fraiseql.registerQuery(
  "getUser",
  "User",
  false,
  false,
  [{ name: "userId", type: "ID", nullable: false }],
  "Get a single user by ID"
);

@fraiseql.Query({ sqlSource: "v_user" })
function listUsers(limit: number = 10, offset: number = 0): User[] {
  pass;
}

fraiseql.registerQuery(
  "listUsers",
  "User",
  true,
  false,
  [
    { name: "limit", type: "Int", nullable: false, default: 10 },
    { name: "offset", type: "Int", nullable: false, default: 0 },
  ],
  "Get list of users"
);

@fraiseql.Query({ sqlSource: "v_order" })
function listOrders(filter?: Record<string, unknown>): Order[] {
  pass;
}

fraiseql.registerQuery(
  "listOrders",
  "Order",
  true,
  false,
  [{ name: "filter", type: "OrderFilter", nullable: true }],
  "List orders with optional filtering"
);

// Global search returning union type
@fraiseql.Query()
function search(input: Record<string, unknown>): unknown[] {
  pass;
}

fraiseql.registerQuery(
  "search",
  "SearchResult",
  true,
  false,
  [{ name: "input", type: "SearchInput", nullable: false }],
  "Global search across users, posts, and orders"
);

// ============================================================================
// MUTATIONS - Using enums and input types
// ============================================================================

@fraiseql.Mutation({ sqlSource: "fn_create_user", operation: "CREATE" })
function createUser(input: Record<string, unknown>): User {
  pass;
}

fraiseql.registerMutation(
  "createUser",
  "User",
  false,
  false,
  [{ name: "input", type: "CreateUserInput", nullable: false }],
  "Create a new user",
  { sql_source: "fn_create_user", operation: "CREATE" }
);

@fraiseql.Mutation({ sqlSource: "fn_update_order_status", operation: "UPDATE" })
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
  "Update order status",
  { sql_source: "fn_update_order_status", operation: "UPDATE" }
);

// ============================================================================
// EXPORT SCHEMA
// ============================================================================

if (require.main === module) {
  fraiseql.exportSchema("schema.json");
  console.log("✅ Schema exported to schema.json");
  console.log("  Enums: OrderStatus, PaymentMethod, UserRole");
  console.log("  Interfaces: Node, Auditable");
  console.log("  Unions: SearchResult");
  console.log("  Input Types: CreateUserInput, OrderFilter, SearchInput");
}
