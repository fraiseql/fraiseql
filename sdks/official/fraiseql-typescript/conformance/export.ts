/**
 * Author the cross-SDK conformance fixture with the TypeScript SDK's public API.
 *
 * Driven by sdks/official/conformance/run.py; see sdks/official/conformance/README.md.
 *
 * The one rule for every SDK's copy of this file: author through the SDK, never
 * hand-assemble the JSON.
 *
 * The imperative `registerTypeFields` / `registerQuery` / `registerMutation` functions
 * are the authoring surface. `@Query`/`@Mutation`/`@Subscription` cannot be: TypeScript
 * erases the types they would need, so they refuse rather than register the placeholders
 * they used to (#733).
 */

import {
  SchemaRegistry,
  registerTypeFields,
  registerQuery,
  registerMutation,
  enum_,
  input,
  exportSchema,
} from "../src/index";

function authorMinimal(): void {
  registerTypeFields(
    "User",
    [
      { name: "id", type: "ID", nullable: false },
      { name: "email", type: "String", nullable: false },
    ],
    undefined,
    { sqlSource: "v_user" }
  );

  registerQuery("users", "User", true, false, [], undefined, { sql_source: "v_user" });
}

function authorFull(): void {
  registerTypeFields(
    "User",
    [
      { name: "id", type: "ID", nullable: false },
      { name: "email", type: "String", nullable: false },
      { name: "name", type: "String", nullable: true, description: 'The user\'s "display" name' },
      { name: "salary", type: "Float", nullable: true, requires_scope: "read:User.salary" },
    ],
    undefined,
    { sqlSource: "v_user", relay: true }
  );

  registerTypeFields(
    "Order",
    [
      { name: "id", type: "ID", nullable: false },
      { name: "total", type: "Float", nullable: false },
      { name: "status", type: "String", nullable: false },
    ],
    undefined,
    { sqlSource: "v_order" }
  );

  registerTypeFields(
    "UserNotFound",
    [
      { name: "message", type: "String", nullable: false },
      { name: "code", type: "String", nullable: false },
    ],
    undefined,
    { sqlSource: "v_user_not_found", isError: true }
  );

  registerTypeFields(
    "Document",
    [
      { name: "id", type: "ID", nullable: false },
      {
        name: "embedding",
        type: "Vector",
        nullable: false,
        vectorConfig: { dimensions: 1536, indexType: "ivf_flat", distanceMetric: "l2" },
      },
      {
        name: "fingerprint",
        type: "BitVector",
        nullable: false,
        vectorConfig: { dimensions: 768, indexType: "hnsw", distanceMetric: "hamming" },
      },
      {
        name: "compact",
        type: "HalfVector",
        nullable: true,
        vectorConfig: { dimensions: 1536, indexType: "hnsw", distanceMetric: "inner_product" },
      },
      {
        name: "terms",
        type: "SparseVector",
        nullable: true,
        vectorConfig: { dimensions: 30000, indexType: "none", distanceMetric: "cosine" },
      },
      { name: "similarity", type: "Float", nullable: false, vectorDistance: "embedding" },
    ],
    undefined,
    { sqlSource: "v_document" }
  );

  enum_("OrderStatus", { PENDING: "PENDING", SHIPPED: "SHIPPED", CANCELLED: "CANCELLED" });

  input("CreateUserInput", [
    { name: "email", type: "String", nullable: false },
    { name: "name", type: "String", nullable: true },
  ]);

  registerQuery("users", "User", true, false, [], undefined, { sql_source: "v_user" });

  registerQuery(
    "user",
    "User",
    false,
    true,
    [{ name: "id", type: "ID", nullable: false }],
    undefined,
    { sql_source: "v_user" }
  );

  registerQuery("tenantOrders", "Order", true, false, [], undefined, {
    sql_source: "v_order",
    inject_params: { tenant_id: "jwt:tenant_id" },
    cache_ttl_seconds: 300,
    requires_role: "admin",
  });

  registerMutation(
    "createUser",
    "User",
    false,
    false,
    [
      { name: "email", type: "String", nullable: false },
      { name: "name", type: "String", nullable: true },
    ],
    undefined,
    {
      sql_source: "fn_create_user",
      operation: "insert",
      invalidates_views: ["v_user", "v_user_summary"],
      invalidates_fact_tables: ["tf_signup"],
    }
  );

  registerMutation("placeOrder", "Order", false, false, [], undefined, {
    sql_source: "fn_place_order",
    operation: "insert",
    inject_params: { user_id: "jwt:sub" },
    invalidates_views: ["v_order_summary"],
    invalidates_fact_tables: ["tf_sale"],
  });
}

const fixture = process.env.FRAISEQL_CONFORMANCE_FIXTURE;
const out = process.env.FRAISEQL_CONFORMANCE_OUT;
if (!fixture || !out) {
  console.error("FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set");
  process.exit(2);
}

SchemaRegistry.clear();
if (fixture === "minimal") {
  authorMinimal();
} else if (fixture === "full") {
  authorFull();
} else {
  console.error(`unknown fixture ${fixture}`);
  process.exit(2);
}

exportSchema(out);
