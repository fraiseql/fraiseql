/**
 * The federation field decorators under the TC39 decorator protocol, which esbuild — and
 * so `tsx` — emits: a field decorator is handed `(undefined, context)` and no class, and
 * records its mark in `context.metadata`, the object the runtime stores on the class as
 * `Class[Symbol.metadata]`. This suite runs with legacy decorators, so the protocol is
 * driven by hand, the way the transpiled code calls it.
 */

import {
  Key,
  Extends,
  External,
  Shareable,
  Inaccessible,
  generateSchemaJson,
  SchemaRegistry,
} from "../src/federation";

/* eslint-disable @typescript-eslint/no-explicit-any -- schema.json is untyped JSON */

const METADATA: symbol =
  (Symbol as { metadata?: symbol }).metadata ?? Symbol.for("Symbol.metadata");

/** Apply `decorators` to `field` of `cls` as TC39 transpiled code does. */
function decorateField(
  cls: any,
  field: string,
  decorators: Array<(target: unknown, context: unknown) => unknown>
): void {
  cls[METADATA] ??= {};
  for (const decorate of decorators) {
    decorate(undefined, { kind: "field", name: field, metadata: cls[METADATA] });
  }
}

describe("federation field decorators under the TC39 protocol", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("records external, shareable and inaccessible fields on the decorated class", () => {
    class Warehouse {
      id = "";
      name = "";
      code = "";
    }
    decorateField(Warehouse, "id", [External() as never]);
    decorateField(Warehouse, "name", [Shareable() as never]);
    decorateField(Warehouse, "code", [Inaccessible() as never]);
    Extends()(Key("id")(Warehouse));
    SchemaRegistry.registerType("Warehouse", [
      { name: "id", type: "ID", nullable: false },
      { name: "name", type: "String", nullable: false },
      { name: "code", type: "String", nullable: false },
    ]);

    const schema: any = generateSchemaJson([Warehouse]);

    expect(schema.federation.entities).toEqual([
      {
        name: "Warehouse",
        key_fields: ["id"],
        extends: true,
        external_fields: ["id"],
        shareable_fields: ["name"],
      },
    ]);
    const code = schema.types[0].fields.find((f: any) => f.name === "code");
    expect(code.directives).toEqual([{ name: "inaccessible" }]);
  });

  it("keeps a subclass's marks out of its parent's", () => {
    class Base {
      id = "";
    }
    decorateField(Base, "id", [Shareable() as never]);
    class Derived extends Base {
      code = "";
    }
    (Derived as any)[METADATA] = Object.create((Base as any)[METADATA]);
    decorateField(Derived, "code", [Shareable() as never]);
    Key("id")(Base);
    Key("id")(Derived);

    const schema: any = generateSchemaJson([Base, Derived]);

    expect(schema.federation.entities).toEqual([
      { name: "Base", key_fields: ["id"], shareable_fields: ["id"] },
      { name: "Derived", key_fields: ["id"], shareable_fields: ["code"] },
    ]);
  });

  it("marks a class shareable when handed a class context", () => {
    class Money {
      amount = 0;
    }
    Shareable()(Money, { kind: "class", name: "Money" });

    const schema: any = generateSchemaJson([Money]);

    expect(schema.federation.shareable_types).toEqual(["Money"]);
  });
});
