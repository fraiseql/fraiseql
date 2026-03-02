/**
 * Generate parity schema for cross-SDK comparison.
 *
 * Usage:
 *   bun run tests/generate-parity-schema.ts
 */

import { SchemaRegistry, registerTypeFields, registerQuery, registerMutation } from "../src/index";

SchemaRegistry.clear();

// --- Types ---

registerTypeFields("User", [
  { name: "id",    type: "ID",     nullable: false },
  { name: "email", type: "String", nullable: false },
  { name: "name",  type: "String", nullable: false },
], undefined, { sqlSource: "v_user" });

registerTypeFields("Order", [
  { name: "id",    type: "ID",    nullable: false },
  { name: "total", type: "Float", nullable: false },
], undefined, { sqlSource: "v_order" });

registerTypeFields("UserNotFound", [
  { name: "message", type: "String", nullable: false },
  { name: "code",    type: "String", nullable: false },
], undefined, { isError: true, sqlSource: "v_user_not_found" });

// --- Queries ---

registerQuery("users", "User", true, false, [], undefined, {
  sqlSource: "v_user",
});

registerQuery("tenantOrders", "Order", true, false, [], undefined, {
  sqlSource: "v_order",
  inject: { tenant_id: "jwt:tenant_id" },
  cacheTtlSeconds: 300,
  requiresRole: "admin",
});

// --- Mutations ---

registerMutation("createUser", "User", false, false, [
  { name: "email", type: "String", nullable: false },
  { name: "name",  type: "String", nullable: false },
], undefined, {
  sqlSource: "fn_create_user",
  operation: "insert",
});

registerMutation("placeOrder", "Order", false, false, [], undefined, {
  sqlSource: "fn_place_order",
  operation: "insert",
  inject: { user_id: "jwt:sub" },
  invalidatesViews: ["v_order_summary"],
  invalidatesFactTables: ["tf_sales"],
});

// Output schema as JSON
const schema = SchemaRegistry.getSchema();
const output = {
  types: schema.types,
  queries: schema.queries,
  mutations: schema.mutations,
};
process.stdout.write(JSON.stringify(output, null, 2) + "\n");
