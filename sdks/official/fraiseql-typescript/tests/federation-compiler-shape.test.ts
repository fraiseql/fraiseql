/**
 * The federation decorators reach schema.json in the shapes the compiler reads: entities
 * in the top-level `federation.entities`, a type-level `@Shareable()` in
 * `federation.shareable_types`, `@Inaccessible()` as a field `directives` entry. Markers
 * the compiled schema has no carrier for are refused by name where they are declared.
 */

import {
  Type,
  Key,
  Extends,
  External,
  Requires,
  Provides,
  Shareable,
  Inaccessible,
  Override,
  generateSchemaJson,
  SchemaRegistry,
} from "../src/federation";
import { registerTypeFields } from "../src/decorators";
import { exportSchema } from "../src/schema";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

/* eslint-disable @typescript-eslint/no-explicit-any -- schema.json is untyped JSON */

describe("federation markers in the compiler's shape", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("emits an entity with its key, extends, external and shareable fields", () => {
    @Extends()
    @Key("id")
    @Type()
    class Product {
      @External() id: string;
      @Shareable() name: string;
      stock: number;
    }
    registerTypeFields("Product", [
      { name: "id", type: "ID", nullable: false },
      { name: "name", type: "String", nullable: false },
      { name: "stock", type: "Int", nullable: false },
    ]);

    const schema: any = generateSchemaJson([Product], { serviceName: "inventory" });

    expect(schema.federation).toEqual({
      enabled: true,
      version: "v2",
      service_name: "inventory",
      entities: [
        {
          name: "Product",
          key_fields: ["id"],
          extends: true,
          external_fields: ["id"],
          shareable_fields: ["name"],
        },
      ],
    });
    const product = schema.types.find((t: any) => t.name === "Product");
    expect(product.federation).toBeUndefined();
    expect(product.fields.every((f: any) => f.federation === undefined)).toBe(true);
  });

  it("emits a composite key as the one field set it was written as", () => {
    @Key("tenant_id id")
    @Type()
    class Account {
      tenant_id: string;
      id: string;
    }

    const schema: any = generateSchemaJson([Account]);

    expect(schema.federation.entities).toEqual([{ name: "Account", key_fields: ["tenant_id id"] }]);
  });

  it("refuses a second @Key: the compiled schema carries one key per entity", () => {
    expect(() => {
      @Key("email")
      @Key("id")
      @Type()
      class Account {
        id: string;
        email: string;
      }
      return Account;
    }).toThrow(/@Key on type Account: .*one key/);
  });

  it("emits a type-level @Shareable() as a shareable type, not an entity", () => {
    @Shareable()
    @Type()
    class Money {
      amount: number;
      currency: string;
    }

    const schema: any = generateSchemaJson([Money]);

    expect(schema.federation.shareable_types).toEqual(["Money"]);
    expect(schema.federation.entities).toEqual([]);
  });

  it("emits @Inaccessible() as a field directive", () => {
    @Key("id")
    @Type()
    class Account {
      id: string;
      @Inaccessible() internal_code: string;
      name: string;
    }
    registerTypeFields("Account", [
      { name: "id", type: "ID", nullable: false },
      { name: "internal_code", type: "String", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);

    const schema: any = generateSchemaJson([Account]);
    const fields = schema.types.find((t: any) => t.name === "Account").fields;

    expect(fields.find((f: any) => f.name === "internal_code").directives).toEqual([
      { name: "inaccessible" },
    ]);
    expect(fields.find((f: any) => f.name === "name").directives).toBeUndefined();
  });

  it("exportSchema writes the federation block when given the federated types", () => {
    @Key("id")
    @Type()
    class Product {
      id: string;
      name: string;
    }
    registerTypeFields("Product", [
      { name: "id", type: "ID", nullable: false },
      { name: "name", type: "String", nullable: false },
    ]);
    const out = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "fraiseql-fed-")), "schema.json");

    exportSchema(out, { federation: { types: [Product], serviceName: "catalog" } });

    const written = JSON.parse(fs.readFileSync(out, "utf-8"));
    expect(written.federation.service_name).toBe("catalog");
    expect(written.federation.entities).toEqual([{ name: "Product", key_fields: ["id"] }]);
  });

  it.each([
    ["@Requires", () => Requires("weight")],
    ["@Provides", () => Provides("name")],
    ["@Override", () => Override("legacy-pricing")],
  ])("refuses %s: the compiled schema has no carrier for it", (marker, declare) => {
    expect(declare).toThrow(new RegExp(`${marker}.*no carrier in the compiled schema`));
  });
});
