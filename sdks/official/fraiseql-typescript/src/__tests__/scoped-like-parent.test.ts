import { SchemaRegistry } from "../registry";
import { registerTypeFields } from "../decorators";

/**
 * `scopedLikeParent` on a field that embeds a scoped type (#1737).
 *
 * A type whose own query is scoped by `inject` does not compile inside another type's
 * document unless the embedding field declares it scoped like its parent. The compiler
 * checks the claim; the SDK's part is to carry the declaration under the key the compiler
 * reads, and only where it was made.
 */
describe("scopedLikeParent", () => {
  beforeEach(() => SchemaRegistry.clear());

  const fieldOf = (typeName: string, fieldName: string): Record<string, unknown> => {
    const type = SchemaRegistry.getSchema().types.find((t) => t.name === typeName)!;
    return type.fields.find((f) => f.name === fieldName)! as unknown as Record<string, unknown>;
  };

  it("is exported as scoped_like_parent on the field that declares it", () => {
    registerTypeFields(
      "Order",
      [
        { name: "id", type: "ID", nullable: false },
        { name: "lines", type: "[OrderLine!]", nullable: false, scopedLikeParent: true },
        { name: "firstLine", type: "OrderLine", nullable: true },
        { name: "lastLine", type: "OrderLine", nullable: true, scopedLikeParent: false },
      ],
      undefined,
      { sqlSource: "v_order" }
    );

    expect(fieldOf("Order", "lines").scoped_like_parent).toBe(true);
    expect(fieldOf("Order", "lines").scopedLikeParent).toBeUndefined();
    expect("scoped_like_parent" in fieldOf("Order", "firstLine")).toBe(false);
    expect("scoped_like_parent" in fieldOf("Order", "lastLine")).toBe(false);
    expect("scopedLikeParent" in fieldOf("Order", "lastLine")).toBe(false);
  });
});
