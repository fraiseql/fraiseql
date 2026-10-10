import { describe, expect, it, beforeEach } from "vitest";

import { registerMutation } from "../decorators";
import type { MutationConfig } from "../decorators";
import { SchemaRegistry } from "../registry";

// #1397: a cascade mutation declares typed success fields; the compiler adds them to its
// payload, and the function returns them in its row's `result jsonb` column.
describe("mutation success fields", () => {
  beforeEach(() => SchemaRegistry.clear());

  it("exports the declared success fields", () => {
    const config: MutationConfig = {
      sqlSource: "fn_create_order",
      operation: "CREATE",
      cascade: true,
      successFields: [
        { name: "recoveredItems", type: "Int", nullable: false },
        { name: "recovery", type: "Recovery", nullable: true },
      ],
    };
    registerMutation("createOrder", "Order", false, false, [], undefined, { ...config });
    const mutation = SchemaRegistry.getSchema().mutations[0];
    expect(mutation?.success_fields).toEqual([
      { name: "recoveredItems", type: "Int", nullable: false },
      { name: "recovery", type: "Recovery", nullable: true },
    ]);
  });

  it("refuses success fields on a mutation with no cascade payload", () => {
    expect(() =>
      registerMutation("createOrder", "Order", false, false, [], undefined, {
        successFields: [{ name: "n", type: "Int", nullable: false }],
      })
    ).toThrow(/successFields.*cascade: true/);
  });
});

// #1391: a cascade mutation can also take its cascade from pg_tviews' affected set.
describe("mutation cascade source", () => {
  beforeEach(() => SchemaRegistry.clear());

  it("exports cascade_source", () => {
    const config: MutationConfig = { cascade: true, cascadeSource: "pg_tviews" };
    registerMutation("createOrder", "Order", false, false, [], undefined, { ...config });
    expect(SchemaRegistry.getSchema().mutations[0]?.cascade_source).toBe("pg_tviews");
  });

  it("refuses a cascade source without cascade, or an unknown one", () => {
    for (const config of [{ cascadeSource: "pg_tviews" }, { cascade: true, cascadeSource: "x" }]) {
      SchemaRegistry.clear();
      expect(() =>
        registerMutation("createOrder", "Order", false, false, [], undefined, config)
      ).toThrow(/cascadeSource/);
    }
  });
});
