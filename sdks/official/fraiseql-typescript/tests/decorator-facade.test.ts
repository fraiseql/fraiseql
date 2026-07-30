import {
  SchemaRegistry,
  Type,
  Query,
  Mutation,
  Subscription,
  registerTypeFields,
  exportSchemaToString,
} from "../src/index";

/**
 * `@Type`, `@Query`, `@Mutation` and `@Subscription` used to register placeholders:
 * a type with zero fields, and operations whose return type was the literal string
 * "Query"/"Mutation" with no arguments. Nothing warned. A developer following the
 * package's own `@packageDocumentation` example exported a plausible-looking
 * `schema.json` describing a schema they had not written, and only discovered it when
 * the compiled server answered queries with the wrong shape (#733).
 *
 * TypeScript erases types at runtime. A class decorator cannot see `id: number`, and a
 * method decorator cannot see `: User[]` — not with `reflect-metadata` either, which
 * carries no element type for arrays and no name for a structural return type. So there
 * is no honest implementation of these four, and the imperative registration functions
 * are the whole authoring surface.
 *
 * These tests pin the refusal. The rule they encode: an authoring tool whose entire
 * contract is "the schema.json is the truth" must never emit a guess.
 */
describe("decorator façade", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("exporting a @Type whose fields never arrived is refused", () => {
    @Type({ sqlSource: "v_user" })
    class User {
      id!: number;
    }
    void User;

    // The decorator itself is a legitimate marker: it records the name, and the
    // federation path completes the fields later. What must not happen is the
    // *export* succeeding with `"fields": []`.
    expect(() => exportSchemaToString()).toThrow(/Type 'User' was registered with no fields/);
    expect(() => exportSchemaToString()).toThrow(/registerTypeFields/);
  });

  it("the same @Type exports cleanly once its fields are registered", () => {
    @Type({ sqlSource: "v_user" })
    class User {
      id!: number;
    }
    void User;
    registerTypeFields("User", [{ name: "id", type: "ID", nullable: false }], undefined, {
      sqlSource: "v_user",
    });

    const exported = JSON.parse(exportSchemaToString()) as {
      types: Array<{ name: string; fields: unknown[] }>;
    };
    expect(exported.types).toHaveLength(1);
    expect(exported.types[0].fields).toHaveLength(1);
  });

  it("@Query refuses rather than registering a placeholder return type", () => {
    expect(() => {
      class Root {
        @Query({ sqlSource: "v_user" })
        static users(): void {}
      }
      return Root;
    }).toThrow(/registerQuery/);

    expect(SchemaRegistry.getSchema().queries).toHaveLength(0);
  });

  it("@Mutation refuses rather than registering a placeholder return type", () => {
    expect(() => {
      class Root {
        @Mutation({ sqlSource: "fn_create_user" })
        static createUser(): void {}
      }
      return Root;
    }).toThrow(/registerMutation/);

    expect(SchemaRegistry.getSchema().mutations).toHaveLength(0);
  });

  it("@Subscription refuses rather than registering placeholder arguments", () => {
    expect(() => {
      class Root {
        @Subscription({ entityType: "Order" })
        static orderCreated(): void {}
      }
      return Root;
    }).toThrow(/registerSubscription/);

    expect(SchemaRegistry.getSchema().subscriptions).toHaveLength(0);
  });

  it("the refusal names the imperative replacement, not just the problem", () => {
    let message = "";
    try {
      class Root {
        @Query({ sqlSource: "v_user" })
        static users(): void {}
      }
      void Root;
    } catch (error) {
      message = (error as Error).message;
    }
    // A developer hitting this has a schema file open and needs the next line to write,
    // not a diagnosis. The three facts that gets them there: which decorator, which
    // function replaces it, and where the worked example is.
    expect(message).toContain("@Query");
    expect(message).toContain("registerQuery");
    expect(message).toContain("README");
  });
});
