import { Observer } from "../observers";
import { SchemaRegistry } from "../registry";
import { Source } from "../sources";

/**
 * `@Observer` and `@Source` must read the decorated member's name under **both**
 * decorator protocols (issue #925).
 *
 * The rest of this suite applies the decorators with `@`-syntax, which means whatever
 * protocol the test transpiler emits — vitest honours `experimentalDecorators` from
 * `tsconfig.json`, so those tests exercise the *legacy* three-argument form and pass.
 * `tsx`, which the SDK's own README and examples tell users to run, uses esbuild and
 * emits the **TC39** two-argument form, where the second argument is a context object
 * rather than a string. Both decorators stored that argument verbatim as the member's
 * name, so `npx tsx examples/…` produced
 * `"name": {"kind":"method","name":"pollOrders",…}` and the compiler rejected the whole
 * document with `invalid type: map, expected a string` — the defect frozen into the
 * committed `ecommerce_schema.json` artifacts that #925 found.
 *
 * These tests therefore call the decorator functions **directly** with each protocol's
 * arguments, so they assert the SDK's behaviour rather than the transpiler's choice.
 */
describe("decorator protocol independence", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  /** What esbuild/tsx passes as the second argument to a method decorator. */
  const tc39Context = (name: string) => ({
    kind: "method" as const,
    name,
    metadata: {},
    static: false,
    private: false,
    access: {},
  });

  it("@Source takes the member name from a TC39 context object", () => {
    Source({ schedule: "*/5 * * * *" })({}, tc39Context("pollOrders"));

    const source = SchemaRegistry.getSchema().sources?.[0];
    expect(source?.name).toBe("pollOrders");
    expect(source?.function).toBe("pollOrders");
  });

  it("@Source still takes a legacy string propertyKey", () => {
    Source({ schedule: "*/5 * * * *" })({}, "pollOrders", {} as PropertyDescriptor);

    expect(SchemaRegistry.getSchema().sources?.[0]?.name).toBe("pollOrders");
  });

  it("@Observer takes the member name from a TC39 context object", () => {
    Observer({ entity: "Order", event: "created", actions: [] })(
      {},
      tc39Context("onOrderCreated")
    );

    expect(SchemaRegistry.getSchema().observers?.[0]?.name).toBe("onOrderCreated");
  });

  it("@Observer still takes a legacy string propertyKey", () => {
    Observer({ entity: "Order", event: "created", actions: [] })(
      {},
      "onOrderCreated",
      {} as PropertyDescriptor
    );

    expect(SchemaRegistry.getSchema().observers?.[0]?.name).toBe("onOrderCreated");
  });

  it("refuses a second argument it cannot read a name from", () => {
    // Registering an unusable name would surface two commands later as an opaque
    // compiler parse error naming no decorator and no member.
    expect(() => Source({ schedule: "* * * * *" })({}, { kind: "method" })).toThrow(
      /@Source could not determine the decorated member's name/
    );
  });
});
