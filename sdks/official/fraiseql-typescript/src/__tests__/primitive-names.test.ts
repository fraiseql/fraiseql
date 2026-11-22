import { describe, expect, it } from "vitest";
import { extractFieldInfo, extractFunctionSignature, typeToGraphQL } from "../types";

// TypeScript's primitive type names read as their constructors do: `number` is `Float`,
// `string` is `String`, `boolean` is `Boolean` (#2274). They had passed through as
// written, names the compiler refuses. `"Date"` is the engine's calendar `Date` and stays.
describe("primitive type names", () => {
  it("map like their constructors", () => {
    expect(typeToGraphQL("number")).toEqual(typeToGraphQL(Number));
    expect(typeToGraphQL("string")).toEqual(typeToGraphQL(String));
    expect(typeToGraphQL("boolean")).toEqual(typeToGraphQL(Boolean));
    expect(typeToGraphQL("number")).toEqual(["Float", false]);
    expect(typeToGraphQL("string | null")).toEqual(["String", true]);
    expect(typeToGraphQL("boolean[]")).toEqual(["[Boolean!]", false]);
    expect(typeToGraphQL("Date")).toEqual(["Date", false]);
    // An inherited property name is a type name like any other, not a mapping.
    expect(typeToGraphQL("constructor")).toEqual(["constructor", false]);
  });

  it("are what extractFieldInfo's documented example gives", () => {
    expect(extractFieldInfo({ id: "number", name: "string", email: "string | null" })).toEqual({
      id: { type: "Float", nullable: false },
      name: { type: "String", nullable: false },
      email: { type: "String", nullable: true },
    });
  });

  it("are what extractFunctionSignature's documented example gives", () => {
    const signature = extractFunctionSignature("users", { limit: "number", offset: "number" }, "User[]");
    expect(signature.arguments).toEqual([
      { name: "limit", type: "Float", nullable: false },
      { name: "offset", type: "Float", nullable: false },
    ]);
  });
});
