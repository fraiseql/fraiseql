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
  registerSubscription,
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
      {
        name: "name",
        type: "String",
        nullable: true,
        description: 'The user\'s "display" name',
        deprecated: "use displayName",
      },
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

  // `crud` is an authoring-time expansion the compiler has no concept of, so the only
  // evidence this SDK implements it is that the operations and input objects appear in
  // the compiled schema. `computed` is the same: emitting the flag makes the document
  // uncompilable, so the sole evidence it was honoured is `slug` on the type and absent
  // from both input objects.
  registerTypeFields(
    "SupportTicket",
    [
      { name: "id", type: "Int", nullable: false },
      { name: "title", type: "String", nullable: false },
      { name: "slug", type: "String", nullable: false, computed: true },
    ],
    undefined,
    { sqlSource: "v_support_ticket", crud: true }
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
    // #966's actor allow-list, enforced in the same executor gate as `requires_role` on
    // every transport, and authorable in no SDK until #1123.
    requires_actor: ["human_user", "service_account"],
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
      requires_actor: ["service_account"],
    }
  );

  registerMutation("placeOrder", "Order", false, false, [], undefined, {
    sql_source: "fn_place_order",
    operation: "insert",
    inject_params: { user_id: "jwt:sub" },
    invalidates_views: ["v_order_summary"],
    invalidates_fact_tables: ["tf_sale"],
  });

  registerSubscription(
    "orderUpdated",
    "Order",
    [{ name: "orderId", type: "ID", nullable: true }],
    "Stream of order update events",
    {
      topic: "order_events",
      filter: { conditions: [{ argument: "orderId", path: "$.id" }] },
      fields: ["id", "total"],
    }
  );
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
