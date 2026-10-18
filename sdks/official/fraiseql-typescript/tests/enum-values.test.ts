import { SchemaRegistry, enum_ } from "../src/index";

// #1611: a member is stored as the value TypeScript gives it, a string or an integer, and as
// its name when the value is its name; any other value is refused, naming the member.
describe("enum_ member values", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  const exported = (name: string) =>
    SchemaRegistry.getSchema().enums!.find((e) => e.name === name)!.values;

  it("exports a string value that differs from the name", () => {
    enum_("OrderStatus", { PENDING: "pending", IN_TRANSIT: "transit", SHIPPED: "SHIPPED" });

    expect(exported("OrderStatus")).toEqual([
      { name: "PENDING", value: "pending" },
      { name: "IN_TRANSIT", value: "transit" },
      { name: "SHIPPED" },
    ]);
  });

  it("exports a numeric enum's integers and skips its reverse mapping", () => {
    enum Priority {
      LOW = 1,
      HIGH = 3,
    }

    enum_("Priority", Priority);

    expect(exported("Priority")).toEqual([
      { name: "LOW", value: 1 },
      { name: "HIGH", value: 3 },
    ]);
  });

  it("refuses a value that is neither a string nor an integer, naming the member", () => {
    expect(() => enum_("Ratio", { HALF: 0.5 })).toThrow(/Ratio\.HALF/);
    expect(() => enum_("Flag", { ON: true })).toThrow(/Flag\.ON/);
    expect(() => enum_("Shape", { BOX: { w: 1 } })).toThrow(/Shape\.BOX/);
  });
});
