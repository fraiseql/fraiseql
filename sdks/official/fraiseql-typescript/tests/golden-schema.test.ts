/**
 * Golden schema fixture comparisons.
 *
 * Each test builds the same schema that the corresponding JSON fixture
 * describes and verifies that the TypeScript SDK emits exactly the fields
 * the Rust compiler expects.
 */

import * as fs from "fs";
import * as path from "path";
import { SchemaRegistry, registerTypeFields, registerQuery, registerMutation } from "../src/index";

const GOLDEN_DIR = path.resolve(__dirname, "../../tests/fixtures/golden");

function loadGolden(name: string): Record<string, unknown> {
  return JSON.parse(fs.readFileSync(path.join(GOLDEN_DIR, name), "utf8"));
}

beforeEach(() => SchemaRegistry.clear());

// ─── Fixture 01: basic query and mutation ─────────────────────────────────────

describe("Golden fixture 01 — basic query and mutation", () => {
  function buildFixture01() {
    registerQuery("users", "User", true, false, [
      { name: "email", type: "String", nullable: true },
      { name: "limit", type: "Int", nullable: false, default: 10 },
    ], "List all users with optional filtering", {
      sqlSource: "v_user",
      jsonbColumn: "payload",
    });

    registerQuery("user", "User", false, true, [
      { name: "id", type: "ID", nullable: false },
    ], "Fetch a single user by ID", {
      sqlSource: "v_user",
      jsonbColumn: "payload",
    });

    registerMutation("createUser", "User", false, false, [
      { name: "email", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ], "Create a new user account", {
      sqlSource: "fn_create_user",
      operation: "CREATE",
    });
  }

  it("query sql_source matches golden", () => {
    buildFixture01();
    const golden = loadGolden("01-basic-query-mutation.json") as {
      queries: Array<{ sql_source: string; name: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenUsers = golden.queries.find((q) => q.name === "users")!;
    const tsUsers = schema.queries.find((q) => q.name === "users")!;
    expect(tsUsers.sql_source).toBe(goldenUsers.sql_source);
  });

  it("mutation sql_source matches golden", () => {
    buildFixture01();
    const golden = loadGolden("01-basic-query-mutation.json") as {
      mutations: Array<{ sql_source: string; name: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenCreate = golden.mutations.find((m) => m.name === "createUser")!;
    const tsCreate = schema.mutations.find((m) => m.name === "createUser")!;
    expect(tsCreate.sql_source).toBe(goldenCreate.sql_source);
  });

  it("query jsonb_column matches golden", () => {
    buildFixture01();
    const golden = loadGolden("01-basic-query-mutation.json") as {
      queries: Array<{ jsonb_column?: string; name: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenUsers = golden.queries.find((q) => q.name === "users")!;
    const tsUsers = schema.queries.find((q) => q.name === "users")!;
    expect(tsUsers.jsonb_column).toBe(goldenUsers.jsonb_column);
  });
});

// ─── Fixture 04: error types ──────────────────────────────────────────────────

describe("Golden fixture 04 — error types", () => {
  function buildFixture04() {
    registerTypeFields("CreateUserSuccess", [
      { name: "id", type: "ID", nullable: false },
      { name: "email", type: "String", nullable: false },
    ], undefined, { sqlSource: "v_user" });

    registerTypeFields("DuplicateEmailError", [
      { name: "message", type: "String", nullable: false },
      { name: "conflicting_id", type: "ID", nullable: true },
      { name: "code", type: "Int", nullable: false },
    ], "Error returned when email already exists", {
      sqlSource: "v_user",
      isError: true,
    });

    registerTypeFields("ValidationError", [
      { name: "message", type: "String", nullable: false },
      { name: "field", type: "String", nullable: false },
      { name: "rule", type: "String", nullable: false },
    ], "Validation failure with field-level details", {
      sqlSource: "v_user",
      isError: true,
    });
  }

  it("success type has no is_error flag", () => {
    buildFixture04();
    const schema = SchemaRegistry.getSchema();
    const success = schema.types.find((t) => t.name === "CreateUserSuccess")!;
    expect(success.is_error).toBeUndefined();
  });

  it("error type has is_error = true", () => {
    buildFixture04();
    const golden = loadGolden("04-error-type.json") as {
      types: Array<{ name: string; is_error?: boolean }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenErr = golden.types.find((t) => t.name === "DuplicateEmailError")!;
    const tsErr = schema.types.find((t) => t.name === "DuplicateEmailError")!;
    expect(tsErr.is_error).toBe(goldenErr.is_error);
  });

  it("error type description matches golden", () => {
    buildFixture04();
    const golden = loadGolden("04-error-type.json") as {
      types: Array<{ name: string; description?: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenVal = golden.types.find((t) => t.name === "ValidationError")!;
    const tsVal = schema.types.find((t) => t.name === "ValidationError")!;
    expect(tsVal.description).toBe(goldenVal.description);
  });

  it("error type sql_source matches golden", () => {
    buildFixture04();
    const golden = loadGolden("04-error-type.json") as {
      types: Array<{ name: string; sql_source?: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenErr = golden.types.find((t) => t.name === "DuplicateEmailError")!;
    const tsErr = schema.types.find((t) => t.name === "DuplicateEmailError")!;
    expect(tsErr.sql_source).toBe(goldenErr.sql_source);
  });
});

// ─── Fixture 05: security / inject / cache ────────────────────────────────────

describe("Golden fixture 05 — security, inject, cache", () => {
  function buildFixture05() {
    registerTypeFields("Order", [
      { name: "id", type: "ID", nullable: false },
      { name: "tenant_id", type: "UUID", nullable: false },
      { name: "amount", type: "Decimal", nullable: false },
      { name: "status", type: "String", nullable: false },
    ], "A tenant-scoped order (admin-only)", {
      sqlSource: "v_order",
      requiresRole: "admin",
    });

    registerQuery("orders", "Order", true, false, [], undefined, {
      sqlSource: "v_order",
      requiresRole: "admin",
      inject: { tenant_id: "jwt:tenant_id" },
      cacheTtlSeconds: 300,
      additionalViews: ["v_order_summary", "v_order_items"],
    });

    registerMutation("createOrder", "Order", false, false, [
      { name: "amount", type: "Decimal", nullable: false },
      { name: "description", type: "String", nullable: true },
    ], "Create an order; auto-stamps tenant and user from JWT", {
      sqlSource: "fn_create_order",
      operation: "CREATE",
      inject: { user_id: "jwt:sub", tenant_id: "jwt:org_id" },
      invalidatesFactTables: ["tf_sales", "tf_order_count"],
      invalidatesViews: ["v_order_summary", "v_order_items"],
    });
  }

  it("type requires_role matches golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      types: Array<{ name: string; requires_role?: string }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenOrder = golden.types.find((t) => t.name === "Order")!;
    const tsOrder = schema.types.find((t) => t.name === "Order")!;
    expect(tsOrder.requires_role).toBe(goldenOrder.requires_role);
  });

  it("query inject_params match golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      queries: Array<{ name: string; inject_params?: Record<string, { source: string; claim: string }> }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenOrders = golden.queries.find((q) => q.name === "orders")!;
    const tsOrders = schema.queries.find((q) => q.name === "orders")!;
    expect(tsOrders.inject_params).toEqual(goldenOrders.inject_params);
  });

  it("query cache_ttl_seconds matches golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      queries: Array<{ name: string; cache_ttl_seconds?: number }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenOrders = golden.queries.find((q) => q.name === "orders")!;
    const tsOrders = schema.queries.find((q) => q.name === "orders")!;
    expect(tsOrders.cache_ttl_seconds).toBe(goldenOrders.cache_ttl_seconds);
  });

  it("query additional_views match golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      queries: Array<{ name: string; additional_views?: string[] }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenOrders = golden.queries.find((q) => q.name === "orders")!;
    const tsOrders = schema.queries.find((q) => q.name === "orders")!;
    expect(tsOrders.additional_views).toEqual(goldenOrders.additional_views);
  });

  it("mutation inject_params match golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      mutations: Array<{ name: string; inject_params?: Record<string, { source: string; claim: string }> }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenCreate = golden.mutations.find((m) => m.name === "createOrder")!;
    const tsCreate = schema.mutations.find((m) => m.name === "createOrder")!;
    expect(tsCreate.inject_params).toEqual(goldenCreate.inject_params);
  });

  it("mutation invalidates_fact_tables match golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      mutations: Array<{ name: string; invalidates_fact_tables?: string[] }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenCreate = golden.mutations.find((m) => m.name === "createOrder")!;
    const tsCreate = schema.mutations.find((m) => m.name === "createOrder")!;
    expect(tsCreate.invalidates_fact_tables).toEqual(goldenCreate.invalidates_fact_tables);
  });

  it("mutation invalidates_views match golden", () => {
    buildFixture05();
    const golden = loadGolden("05-security-inject-cache.json") as {
      mutations: Array<{ name: string; invalidates_views?: string[] }>;
    };
    const schema = SchemaRegistry.getSchema();

    const goldenCreate = golden.mutations.find((m) => m.name === "createOrder")!;
    const tsCreate = schema.mutations.find((m) => m.name === "createOrder")!;
    expect(tsCreate.invalidates_views).toEqual(goldenCreate.invalidates_views);
  });
});
