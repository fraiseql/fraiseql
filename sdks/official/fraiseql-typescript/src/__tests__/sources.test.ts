import { Source } from "../sources";
import { SchemaRegistry } from "../registry";

describe("@Source", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("registers a source with defaults and emits it in the schema", () => {
    class Sources {
      @Source({ schedule: "*/5 * * * *" })
      pollOrders(): void {}
    }
    void Sources;

    const schema = SchemaRegistry.getSchema();
    expect(schema.sources).toHaveLength(1);
    expect(schema.sources?.[0]).toEqual({
      name: "pollOrders",
      schedule: "*/5 * * * *",
      function: "pollOrders", // defaults to the decorated method name
      enabled: true, // defaults to true
    });
  });

  it("carries run_as, cursor, and an explicit function/enabled", () => {
    class Sources {
      @Source({
        schedule: "0 * * * *",
        function: "stripePull",
        cursor: "stripe-cursor",
        enabled: false,
        runAs: { roles: ["ingest_writer"], scopes: ["write:order"], tenant: "acme" },
      })
      stripeSync(): void {}
    }
    void Sources;

    const schema = SchemaRegistry.getSchema();
    expect(schema.sources?.[0]).toEqual({
      name: "stripeSync",
      schedule: "0 * * * *",
      function: "stripePull",
      cursor: "stripe-cursor",
      enabled: false,
      run_as: { roles: ["ingest_writer"], scopes: ["write:order"], tenant: "acme" },
    });
  });

  it("omits sources from the schema when none are registered", () => {
    const schema = SchemaRegistry.getSchema();
    expect(schema.sources).toBeUndefined();
  });
});
