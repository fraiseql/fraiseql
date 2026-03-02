/**
 * TypeScript SDK field-completeness tests.
 *
 * Every field that the Rust compiler can read from schema.json must be
 * exercised here.  The tests are deliberately exhaustive — one assertion
 * per field so that a regression is immediately pinpointed.
 */

import { SchemaRegistry, registerTypeFields, registerQuery, registerMutation } from "../src/index";

beforeEach(() => SchemaRegistry.clear());

// ─── registerTypeFields ───────────────────────────────────────────────────────

describe("registerTypeFields — all fields", () => {
  it("sql_source is stored when sqlSource option is provided", () => {
    registerTypeFields("Product", [], undefined, { sqlSource: "v_product" });
    expect(SchemaRegistry.getSchema().types[0].sql_source).toBe("v_product");
  });

  it("jsonb_column is stored when jsonbColumn option is provided", () => {
    registerTypeFields("Product", [], undefined, { jsonbColumn: "payload" });
    expect(SchemaRegistry.getSchema().types[0].jsonb_column).toBe("payload");
  });

  it("is_error is set to true when isError option is true", () => {
    registerTypeFields("NotFoundError", [], undefined, { isError: true });
    expect(SchemaRegistry.getSchema().types[0].is_error).toBe(true);
  });

  it("is_error is absent when isError option is not set", () => {
    registerTypeFields("Widget", []);
    expect(SchemaRegistry.getSchema().types[0].is_error).toBeUndefined();
  });

  it("requires_role is stored when requiresRole option is provided", () => {
    registerTypeFields("AdminView", [], undefined, { requiresRole: "admin" });
    expect(SchemaRegistry.getSchema().types[0].requires_role).toBe("admin");
  });

  it("implements list is stored when implements option is provided", () => {
    registerTypeFields("Article", [], undefined, { implements: ["Node", "Auditable"] });
    expect(SchemaRegistry.getSchema().types[0].implements).toEqual(["Node", "Auditable"]);
  });

  it("relay flag is stored when relay option is true", () => {
    registerTypeFields("PageInfo", [], undefined, { relay: true });
    expect(SchemaRegistry.getSchema().types[0].relay).toBe(true);
  });

  it("all type options can be combined", () => {
    registerTypeFields(
      "Order",
      [{ name: "id", type: "ID", nullable: false }],
      "An order",
      {
        sqlSource: "v_order",
        jsonbColumn: "data",
        requiresRole: "admin",
        implements: ["Node"],
      }
    );
    const t = SchemaRegistry.getSchema().types[0];
    expect(t.sql_source).toBe("v_order");
    expect(t.jsonb_column).toBe("data");
    expect(t.requires_role).toBe("admin");
    expect(t.implements).toEqual(["Node"]);
    expect(t.description).toBe("An order");
  });
});

// ─── registerQuery — missing fields ──────────────────────────────────────────

describe("registerQuery — all config fields", () => {
  it("sql_source is emitted via camelCase sqlSource config key", () => {
    registerQuery("users", "User", true, false, [], undefined, { sqlSource: "v_user" });
    expect(SchemaRegistry.getSchema().queries[0].sql_source).toBe("v_user");
    // camelCase original must NOT be present
    expect((SchemaRegistry.getSchema().queries[0] as Record<string, unknown>)["sqlSource"]).toBeUndefined();
  });

  it("cache_ttl_seconds is emitted via cacheTtlSeconds config key", () => {
    registerQuery("orders", "Order", true, false, [], undefined, {
      sqlSource: "v_order",
      cacheTtlSeconds: 300,
    });
    expect(SchemaRegistry.getSchema().queries[0].cache_ttl_seconds).toBe(300);
  });

  it("cache_ttl_seconds of 0 is emitted (disables caching)", () => {
    registerQuery("fresh", "X", true, false, [], undefined, {
      sqlSource: "v_x",
      cacheTtlSeconds: 0,
    });
    expect(SchemaRegistry.getSchema().queries[0].cache_ttl_seconds).toBe(0);
  });

  it("inject_params are structured from jwt:<claim> shorthand", () => {
    registerQuery("tenantOrders", "Order", true, false, [], undefined, {
      sqlSource: "v_order",
      inject: { tenant_id: "jwt:tenant_id", user_id: "jwt:sub" },
    });
    const q = SchemaRegistry.getSchema().queries[0];
    expect(q.inject_params).toEqual({
      tenant_id: { source: "jwt", claim: "tenant_id" },
      user_id: { source: "jwt", claim: "sub" },
    });
    // raw inject key must not survive
    expect((q as Record<string, unknown>)["inject"]).toBeUndefined();
  });

  it("additional_views are emitted via additionalViews config key", () => {
    registerQuery("reports", "Report", true, false, [], undefined, {
      sqlSource: "v_report",
      additionalViews: ["v_report_summary", "v_report_detail"],
    });
    expect(SchemaRegistry.getSchema().queries[0].additional_views).toEqual([
      "v_report_summary",
      "v_report_detail",
    ]);
  });

  it("requires_role is emitted via requiresRole config key", () => {
    registerQuery("adminData", "Admin", true, false, [], undefined, {
      sqlSource: "v_admin",
      requiresRole: "admin",
    });
    expect(SchemaRegistry.getSchema().queries[0].requires_role).toBe("admin");
  });

  it("deprecation.reason is emitted when deprecated string is provided", () => {
    registerQuery("legacyUsers", "User", true, false, [], undefined, {
      sqlSource: "v_user",
      deprecated: "Use users instead",
    });
    const q = SchemaRegistry.getSchema().queries[0];
    expect((q as Record<string, unknown>)["deprecation"]).toEqual({ reason: "Use users instead" });
    // raw deprecated key must not survive
    expect((q as Record<string, unknown>)["deprecated"]).toBeUndefined();
  });

  it("relay cursor fields are emitted via relayCursorColumn and relayCursorType", () => {
    registerQuery("products", "Product", true, false, [], undefined, {
      sqlSource: "v_product",
      relay: true,
      relayCursorColumn: "id",
      relayCursorType: "uuid",
    });
    const q = SchemaRegistry.getSchema().queries[0];
    expect(q.relay).toBe(true);
    expect(q.relay_cursor_column).toBe("id");
    expect(q.relay_cursor_type).toBe("uuid");
  });

  it("relay cursor type int64 is emitted correctly", () => {
    registerQuery("events", "Event", true, false, [], undefined, {
      sqlSource: "v_event",
      relay: true,
      relayCursorColumn: "seq",
      relayCursorType: "int64",
    });
    expect(SchemaRegistry.getSchema().queries[0].relay_cursor_type).toBe("int64");
  });
});

// ─── registerMutation — missing fields ───────────────────────────────────────

describe("registerMutation — all config fields", () => {
  it("sql_source is emitted via camelCase sqlSource config key", () => {
    registerMutation("createUser", "User", false, false, [], undefined, {
      sqlSource: "fn_create_user",
      operation: "CREATE",
    });
    const m = SchemaRegistry.getSchema().mutations[0];
    expect(m.sql_source).toBe("fn_create_user");
    expect((m as Record<string, unknown>)["sqlSource"]).toBeUndefined();
  });

  it("inject_params are structured from jwt:<claim> shorthand", () => {
    registerMutation("createOrder", "Order", false, false, [], undefined, {
      sqlSource: "fn_create_order",
      inject: { user_id: "jwt:sub", tenant_id: "jwt:org_id" },
    });
    const m = SchemaRegistry.getSchema().mutations[0];
    expect(m.inject_params).toEqual({
      user_id: { source: "jwt", claim: "sub" },
      tenant_id: { source: "jwt", claim: "org_id" },
    });
    expect((m as Record<string, unknown>)["inject"]).toBeUndefined();
  });

  it("invalidates_views are emitted via invalidatesViews config key", () => {
    registerMutation("placeOrder", "Order", false, false, [], undefined, {
      sqlSource: "fn_place_order",
      invalidatesViews: ["v_order_summary", "v_order_items"],
    });
    expect(SchemaRegistry.getSchema().mutations[0].invalidates_views).toEqual([
      "v_order_summary",
      "v_order_items",
    ]);
  });

  it("invalidates_fact_tables are emitted via invalidatesFactTables config key", () => {
    registerMutation("recordSale", "Sale", false, false, [], undefined, {
      sqlSource: "fn_record_sale",
      invalidatesFactTables: ["tf_sales", "tf_revenue"],
    });
    expect(SchemaRegistry.getSchema().mutations[0].invalidates_fact_tables).toEqual([
      "tf_sales",
      "tf_revenue",
    ]);
  });

  it("deprecation.reason is emitted when deprecated string is provided", () => {
    registerMutation("legacyCreate", "X", false, false, [], undefined, {
      sqlSource: "fn_legacy",
      deprecated: "Use createUser instead",
    });
    const m = SchemaRegistry.getSchema().mutations[0];
    expect((m as Record<string, unknown>)["deprecation"]).toEqual({
      reason: "Use createUser instead",
    });
    expect((m as Record<string, unknown>)["deprecated"]).toBeUndefined();
  });

  it("all mutation config fields can be combined", () => {
    registerMutation("createTenantOrder", "Order", false, false, [], "Create order", {
      sqlSource: "fn_create_tenant_order",
      operation: "CREATE",
      inject: { tenant_id: "jwt:tenant_id", user_id: "jwt:sub" },
      invalidatesViews: ["v_order_summary"],
      invalidatesFactTables: ["tf_sales"],
    });
    const m = SchemaRegistry.getSchema().mutations[0];
    expect(m.sql_source).toBe("fn_create_tenant_order");
    expect(m.operation).toBe("CREATE");
    expect(m.inject_params).toEqual({
      tenant_id: { source: "jwt", claim: "tenant_id" },
      user_id: { source: "jwt", claim: "sub" },
    });
    expect(m.invalidates_views).toEqual(["v_order_summary"]);
    expect(m.invalidates_fact_tables).toEqual(["tf_sales"]);
  });
});
