import { describe, expect, it } from "vitest";

import { isScalarType } from "../scalars";
import { extractFieldInfo, typeToGraphQL } from "../types";

// #2258: a JavaScript `bigint` is a signed integer of any width, the engine's `BigInt` (a
// signed 64-bit integer, served as its digits). A `number` stays `Float`.
describe("bigint", () => {
  it("maps the BigInt constructor and the `bigint` type string to BigInt", () => {
    expect(typeToGraphQL(BigInt)).toEqual(["BigInt", false]);
    expect(typeToGraphQL("bigint")).toEqual(["BigInt", false]);
    expect(typeToGraphQL("bigint | null")).toEqual(["BigInt", true]);
    expect(typeToGraphQL("bigint[]")).toEqual(["[BigInt!]", false]);
    expect(typeToGraphQL(Number)).toEqual(["Float", false]);
  });

  it("extracts bigint fields as BigInt", () => {
    expect(extractFieldInfo({ amount: "bigint", balance: "bigint | null" })).toEqual({
      amount: { type: "BigInt", nullable: false },
      balance: { type: "BigInt", nullable: true },
    });
  });

  it("knows BigInt as a scalar name", () => {
    expect(isScalarType("BigInt")).toBe(true);
  });
});
