/**
 * Tests for minimal types.json export (refactored TOML-based workflow).
 *
 * This test verifies the new minimal export behavior where TypeScript
 * only generates types.json (not complete schema.json with queries,
 * mutations, federation, security, observers, analytics).
 *
 * All configuration moves to fraiseql.toml instead.
 */

import * as fs from "fs";
import * as path from "path";
import * as os from "os";

import { SchemaRegistry } from "../src/index";
import { exportTypes } from "../src/schema";

describe("export types minimal (TOML-based workflow)", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  test("exportTypes() should create minimal types.json with only types", () => {
    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
      { name: "email", type: "String", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "user_types.json");

    exportTypes(outputPath);

    // Load and verify output
    const content = fs.readFileSync(outputPath, "utf-8");
    const schema = JSON.parse(content);

    // Should have types section
    expect(schema.types).toBeDefined();
    expect(schema.types).toHaveLength(1);

    // Should have User type
    const userType = schema.types[0];
    expect(userType.name).toBe("User");
    expect(userType.fields).toHaveLength(3);

    // Verify fields
    const fieldNames = userType.fields.map((f: any) => f.name);
    expect(fieldNames).toEqual(expect.arrayContaining(["id", "name", "email"]));

    // IMPORTANT: No queries, mutations, federation, security, observers, analytics
    expect(!schema.queries || schema.queries.length === 0).toBeTruthy();
    expect(!schema.mutations || schema.mutations.length === 0).toBeTruthy();
    expect(!schema.federation || schema.federation === null).toBeTruthy();
    expect(!schema.security || schema.security === null).toBeTruthy();
    expect(!schema.observers || schema.observers === null).toBeTruthy();
    expect(!schema.analytics || schema.analytics === null).toBeTruthy();

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should handle multiple types correctly", () => {
    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    SchemaRegistry.registerType("Product", [
      { name: "id", type: "String", nullable: false },
      { name: "title", type: "String", nullable: false },
      { name: "price", type: "Float", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "schema_types.json");

    exportTypes(outputPath);

    const content = fs.readFileSync(outputPath, "utf-8");
    const schema = JSON.parse(content);

    expect(schema.types).toHaveLength(2);
    const typeNames = schema.types.map((t: any) => t.name);
    expect(typeNames).toEqual(expect.arrayContaining(["User", "Product"]));

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should include enums in output", () => {
    SchemaRegistry.registerEnum("Status", [
      { name: "ACTIVE" },
      { name: "INACTIVE" },
    ]);

    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "status", type: "Status", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "schema_types.json");

    exportTypes(outputPath);

    const content = fs.readFileSync(outputPath, "utf-8");
    const schema = JSON.parse(content);

    // Should have both enum and type
    expect(schema.enums).toBeDefined();
    expect(schema.enums.length).toBeGreaterThan(0);
    expect(schema.types).toHaveLength(1);

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should include input types in output", () => {
    SchemaRegistry.registerInputType("CreateUserInput", [
      { name: "name", type: "String", nullable: false },
      { name: "email", type: "String", nullable: false },
    ]);

    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "schema_types.json");

    exportTypes(outputPath);

    const content = fs.readFileSync(outputPath, "utf-8");
    const schema = JSON.parse(content);

    // Should have both input type and type
    expect(schema.input_types).toBeDefined();
    expect(schema.input_types.length).toBeGreaterThan(0);
    expect(schema.types).toHaveLength(1);

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should NOT include queries or mutations in output", () => {
    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    // Queries and mutations defined but should NOT appear in types.json
    SchemaRegistry.registerQuery(
      "getUser",
      "User",
      false,
      false,
      [{ name: "userId", type: "String", nullable: false }],
      "Get user",
      { sql_source: "v_user" }
    );

    SchemaRegistry.registerMutation(
      "createUser",
      "User",
      false,
      false,
      [
        { name: "name", type: "String", nullable: false },
        { name: "email", type: "String", nullable: false },
      ],
      "Create user",
      { sql_source: "m_create_user", operation: "CREATE" }
    );

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "schema_types.json");

    exportTypes(outputPath);

    const content = fs.readFileSync(outputPath, "utf-8");
    const schema = JSON.parse(content);

    // Should only have the type
    expect(schema.types).toHaveLength(1);
    expect(schema.types[0].name).toBe("User");

    // Queries and mutations should NOT be in types.json
    // They move to fraiseql.toml [queries] and [mutations] sections
    expect(!schema.queries || schema.queries.length === 0).toBeTruthy();
    expect(!schema.mutations || schema.mutations.length === 0).toBeTruthy();

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should work with pretty formatting", () => {
    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "user_types.json");

    exportTypes(outputPath, { pretty: true });

    const content = fs.readFileSync(outputPath, "utf-8");

    // Should have nice formatting (contains newlines and indentation)
    expect(content).toContain("\n");
    expect(content).toContain("  ");

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });

  test("exportTypes() should work with compact formatting", () => {
    SchemaRegistry.registerType("User", [
      { name: "id", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-"));
    const outputPath = path.join(tmpDir, "user_types.json");

    exportTypes(outputPath, { pretty: false });

    const content = fs.readFileSync(outputPath, "utf-8");

    // Should be compact (JSON string)
    const parsed = JSON.parse(content);
    expect(parsed).toBeDefined();
    expect(parsed.types).toHaveLength(1);

    // Cleanup
    fs.rmSync(tmpDir, { recursive: true });
  });
});
