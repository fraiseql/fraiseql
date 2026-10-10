import { SchemaRegistry } from "../registry";
import { registerMutation, registerTypeFields } from "../decorators";
import { extractFieldInfo, extractFunctionSignature } from "../types";

/**
 * Localized fields and arguments (#1513): a `String` stored as a locale map, emitted as
 * `"localized": true` on the field or argument.
 */
describe("localized authoring", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("emits `localized` on a type field configured with it", () => {
    registerTypeFields(
      "Product",
      [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false, localized: true },
        { name: "sku", type: "String", nullable: false },
      ],
      undefined,
      { sqlSource: "tv_product" }
    );
    const product = SchemaRegistry.getSchema().types.find((t) => t.name === "Product")!;
    const fields = product.fields as unknown as Array<Record<string, unknown>>;
    expect(fields.find((f) => f.name === "name")).toEqual({
      name: "name",
      type: "String",
      nullable: false,
      localized: true,
    });
    expect(fields.find((f) => f.name === "sku")).not.toHaveProperty("localized");
  });

  it("reads `Localized<string>` in the type-string API, nullable or not", () => {
    expect(
      extractFieldInfo({ name: "Localized<string>", note: "Localized<string> | null" })
    ).toEqual({
      name: { type: "String", nullable: false, localized: true },
      note: { type: "String", nullable: true, localized: true },
    });
    const signature = extractFunctionSignature(
      "renameProduct",
      { id: "ID", name: "Localized<string> | null" },
      "Product"
    );
    expect(signature.arguments[1]).toEqual({
      name: "name",
      type: "String",
      nullable: true,
      localized: true,
    });
  });

  it("refuses Localized of anything but string at the declaration", () => {
    expect(() => extractFieldInfo({ stock: "Localized<number>" })).toThrow(
      /Localized<number> is not supported/
    );
  });

  // The `localized: true` option bypasses the type-string check above, so the export
  // refuses it on anything but a String, wherever it was declared (#1527).
  it("refuses the `localized` option on a non-String type field at export", () => {
    registerTypeFields(
      "Priced",
      [{ name: "price", type: "Float", nullable: false, localized: true }],
      undefined,
      { sqlSource: "v_priced" }
    );
    expect(() => SchemaRegistry.getSchema()).toThrow(
      /price.*only a String can be localized/
    );
  });

  it("refuses the `localized` option on a non-String input field at export", () => {
    SchemaRegistry.registerInputType("PriceInput", [
      { name: "amount", type: "Float", nullable: false, localized: true },
    ]);
    expect(() => SchemaRegistry.getSchema()).toThrow(
      /amount.*only a String can be localized/
    );
  });

  it("refuses the `localized` option on a non-String mutation argument at export", () => {
    registerMutation("setPrice", "Product", false, false, [
      { name: "amount", type: "Float", nullable: false, localized: true },
    ]);
    expect(() => SchemaRegistry.getSchema()).toThrow(
      /amount.*only a String can be localized/
    );
  });
});
