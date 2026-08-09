import { SchemaRegistry } from "../registry";
import { exportSchemaToString } from "../schema";
import { interface_, registerQuery, registerTypeFields, union } from "../decorators";

/**
 * The pre-export validator must recognise every kind an operation can return (#925).
 *
 * It was added to catch #733 — a `@Type()` class whose fields were never registered,
 * exported as a plausible-looking type with `"fields": []` — and it builds its set of
 * known names from `types`, `enums` and the five builtin scalars. Unions and interfaces
 * were left out, so a query returning a union the same document declares was refused at
 * export with "is not a registered type", and two shipped examples could not run.
 *
 * The compiler accepts such a query: a union return type compiles cleanly, so the SDK
 * was refusing a document the rest of the pipeline is happy with.
 */
describe("validateSchemaBeforeExport", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  const declareUser = () =>
    registerTypeFields(
      "User",
      [{ name: "id", type: "ID", nullable: false }],
      undefined,
      { sqlSource: "v_user" }
    );

  it("accepts a query returning a declared union", () => {
    declareUser();
    registerTypeFields("Post", [{ name: "id", type: "ID", nullable: false }], undefined, {
      sqlSource: "v_post",
    });
    union("SearchResult", ["User", "Post"]);

    registerQuery("search", "SearchResult", true, false, [], undefined, {
      sql_source: "v_user",
    });

    expect(() => exportSchemaToString()).not.toThrow();
  });

  it("accepts a query returning a declared interface", () => {
    declareUser();
    interface_("Node", [{ name: "id", type: "ID", nullable: false }]);

    registerQuery("node", "Node", false, true, [], undefined, { sql_source: "v_user" });

    expect(() => exportSchemaToString()).not.toThrow();
  });

  it("still refuses a return type nothing declares", () => {
    declareUser();
    registerQuery("ghost", "Nowhere", false, false, [], undefined, {
      sql_source: "v_user",
    });

    expect(() => exportSchemaToString()).toThrow(/'Nowhere' which is not a registered type/);
  });

  it("still refuses a type registered with no fields", () => {
    SchemaRegistry.registerType("Empty", []);

    expect(() => exportSchemaToString()).toThrow(/registered with no fields/);
  });
});

/**
 * `FieldMetadata.requiresScope` must reach the compiler as `requires_scope` (#925).
 *
 * The key was declared on the public `Field` type, documented in the field-metadata
 * example, and never normalised — so it travelled to the compiler camelCased. The
 * compiler refuses it by name (`declares a scope requirement under 'requiresScope',
 * which the compiler does not read`), which is why this was loud rather than a silent
 * unguarded field, but it made the documented spelling unusable.
 */
describe("field requiresScope", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  const exported = () => JSON.parse(exportSchemaToString()).types[0].fields;

  it("is emitted as requires_scope", () => {
    registerTypeFields(
      "User",
      [
        { name: "id", type: "ID", nullable: false },
        { name: "salary", type: "Float", nullable: true, requiresScope: "read:User.salary" },
      ],
      undefined,
      { sqlSource: "v_user" }
    );

    const salary = exported().find((f: { name: string }) => f.name === "salary");
    expect(salary.requires_scope).toBe("read:User.salary");
    expect(salary.requiresScope).toBeUndefined();
  });

  it("accepts the snake_case spelling unchanged", () => {
    registerTypeFields(
      "User",
      [
        { name: "id", type: "ID", nullable: false },
        { name: "salary", type: "Float", nullable: true, requires_scope: "read:User.salary" },
      ] as never,
      undefined,
      { sqlSource: "v_user" }
    );

    const salary = exported().find((f: { name: string }) => f.name === "salary");
    expect(salary.requires_scope).toBe("read:User.salary");
  });

  it("refuses multiple required scopes rather than dropping all but one", () => {
    // The compiled schema and the runtime field filter represent exactly one required
    // scope, so a list cannot be honoured. Silently taking the first would serve the
    // field to callers holding only that one.
    expect(() =>
      registerTypeFields(
        "User",
        [
          { name: "id", type: "ID", nullable: false },
          { name: "ssn", type: "String", nullable: true, requiresScope: ["pii:read", "hr:view"] },
        ],
        undefined,
        { sqlSource: "v_user" }
      )
    ).toThrow(/single scope/);
  });
});
