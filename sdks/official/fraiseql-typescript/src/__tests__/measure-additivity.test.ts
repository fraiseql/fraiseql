import { describe, expect, it, beforeEach } from "vitest";

import { SchemaRegistry } from "../registry";

// #1459: a measure declares how it aggregates over time. A balance is reduced per account
// and per bucket (its last known value), then summed across accounts; summed across days it
// is wrong. The declaration travels to the compiler unchanged.
describe("measure additivity", () => {
  beforeEach(() => SchemaRegistry.clear());

  it("exports a semi-additive, a delta and a non-additive measure", () => {
    SchemaRegistry.registerFactTable(
      "tf_account_day",
      [
        {
          name: "closing_balance",
          sql_type: "numeric",
          nullable: false,
          additivity: { kind: "semi_additive", over: "day", using: "last", entity: ["account_id"] },
        },
        {
          name: "odometer",
          sql_type: "numeric",
          nullable: false,
          additivity: { kind: "delta", over: "day", entity: ["account_id"] },
        },
        { name: "rate", sql_type: "numeric", nullable: true, additivity: { kind: "non_additive" } },
        { name: "deposits", sql_type: "numeric", nullable: false },
      ],
      { name: "data", paths: [] },
      [
        { name: "account_id", sql_type: "bigint", indexed: true },
        { name: "day", sql_type: "date", indexed: true },
      ],
    );
    const measures = SchemaRegistry.getSchema().fact_tables?.[0]?.measures ?? [];
    expect(measures.map((m) => m.additivity)).toEqual([
      { kind: "semi_additive", over: "day", using: "last", entity: ["account_id"] },
      { kind: "delta", over: "day", entity: ["account_id"] },
      { kind: "non_additive" },
      undefined,
    ]);
  });
});
