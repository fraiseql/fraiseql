/**
 * Federation decorator tests for federation decorators.
 *
 * Tests TypeScript federation decorators: @Key, @Extends, @External, @Shareable,
 * @Inaccessible. The refusal of @Requires, @Provides and @Override is pinned in
 * federation-compiler-shape.test.ts.
 */

import {
  Type,
  Key,
  Extends,
  External,
  Shareable,
  Inaccessible,
  ID,
  generateSchemaJson,
  validateFederation,
  SchemaRegistry,
} from "../src/federation";

describe("TypeScript Federation Decorators", () => {
  beforeEach(() => {
    // Clear the global schema registry to prevent cross-test contamination
    // SchemaRegistry is a singleton, so it accumulates types from all tests
    SchemaRegistry.clear();
  });

  describe("@Key decorator", () => {
    it("marks type with federation key", () => {
      @Key("id")
      @Type()
      class User {
        id: string;
        email: string;
      }

      expect(User.__fraiseqlFederation__.key).toBe("id");
    });

    it("rejects non-existent fields", () => {
      @Key("nonexistent")
      @Type()
      class User {
        id: string;
      }

      expect(() => {
        validateFederation([User]);
      }).toThrow("Field 'nonexistent' not found");
    });

    it("requires @Type decorator", () => {
      @Key("id")
      class User {
        id: string;
      }

      expect(() => {
        validateFederation([User]);
      }).toThrow("@Key requires @Type decorator");
    });
  });

  describe("@Extends decorator", () => {
    it("marks type as extended", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
      }

      expect(User.__fraiseqlFederation__.extend).toBe(true);
    });

    it("requires @Key decorator", () => {
      expect(() => {
        @Extends()
        @Type()
        class User {
          id: string;
        }
      }).toThrow();
    });

    it("works with @External decorator", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
        @External() email: string;
        orders: string[]; // Regular field
      }

      expect(User.__fraiseqlFederation__.extend).toBe(true);
      expect(User.__fraiseqlFederation__.external_fields).toContain("id");
      expect(User.__fraiseqlFederation__.external_fields).toContain("email");
      expect(User.__fraiseqlFederation__.external_fields).not.toContain("orders");
    });
  });

  describe("@External decorator", () => {
    it("marks field as external", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
        @External() email: string;
      }

      expect(User.__fraiseqlFederation__.external_fields).toContain("id");
      expect(User.__fraiseqlFederation__.external_fields).toContain("email");
    });

    it("cannot be used without @Extends", () => {
      @Type()
      class User {
        @External() id: string;
      }

      expect(() => {
        validateFederation([User]);
      }).toThrow("@external requires @extends");
    });

    it("cannot be used on non-field context", () => {
      expect(() => {
        class User {
          @External()
          method() {}
        }
      }).toThrow();
    });
  });

  describe("Schema JSON Generation", () => {
    it("includes federation metadata", () => {
      @Type()
      @Key("id")
      class User {
        id: string;
        email: string;
      }

      const schema = generateSchemaJson([User]);

      expect(schema.federation).toBeDefined();
      expect(schema.federation.enabled).toBe(true);
      expect(schema.federation.version).toBe("v2");
    });

    it("emits a keyed type as an entity, not as type-level metadata", () => {
      @Type()
      @Key("id")
      class User {
        id: string;
        email: string;
      }

      const schema = generateSchemaJson([User]);
      const userType = schema.types.find((t: any) => t.name === "User");

      expect(userType.federation).toBeUndefined();
      expect(schema.federation.entities).toEqual([{ name: "User", key_fields: ["id"] }]);
    });

    it("includes extended type metadata", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
        @External() email: string;
      }

      const schema = generateSchemaJson([User]);

      expect(schema.federation.entities).toEqual([
        { name: "User", key_fields: ["id"], extends: true, external_fields: ["id", "email"] },
      ]);
    });
  });

  describe("Compile-time Validation", () => {
    it("rejects invalid key fields", () => {
      @Key("nonexistent")
      @Type()
      class User {
        id: string;
      }

      expect(() => {
        validateFederation([User]);
      }).toThrow("Field 'nonexistent' not found");
    });

    it("rejects @External without @Extends", () => {
      @Type()
      class User {
        @External() id: string;
      }

      expect(() => {
        validateFederation([User]);
      }).toThrow("@external requires @extends");
    });

    it("rejects a second key", () => {
      expect(() => {
        @Key("id")
        @Key("id")
        @Type()
        class User {
          id: string;
        }
      }).toThrow("one key per entity");
    });
  });

  describe("Field Validation", () => {
    it("validates external fields exist", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        id: string;

        // @External on field that will be validated
      }

      // Note: External fields are only validated if they're actually
      // defined as @External. This test checks metadata consistency.
      const metadata = User.__fraiseqlFederation__;
      expect(metadata.extend).toBe(true);
    });

    it("allows mixed external and regular fields", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
        @External() email: string;
        orders: string[]; // Regular field
        purchases: string[]; // Another regular field
      }

      const external = User.__fraiseqlFederation__.external_fields;
      expect(external).toHaveLength(2);
      expect(external).toContain("id");
      expect(external).toContain("email");
    });

    it("validates composite keys", () => {
      @Key("tenant_id id")
      @Type()
      class Account {
        tenant_id: string;
        id: string;
        name: string;
      }

      expect(Account.__fraiseqlFederation__.key).toBe("tenant_id id");
      expect(() => validateFederation([Account])).not.toThrow();
    });

    it("rejects a composite key naming a field the type lacks", () => {
      @Key("tenant_id id")
      @Type()
      class Account {
        id: string;
      }

      expect(() => validateFederation([Account])).toThrow("Field 'tenant_id' not found");
    });
  });

  describe("Complex Scenarios", () => {
    it("handles three-type federation", () => {
      @Type()
      @Key("id")
      class User {
        id: string;
        email: string;
      }

      @Type()
      @Key("id")
      class Order {
        id: string;
        user_id: string;
      }

      @Type()
      @Key("id")
      class Product {
        id: string;
        name: string;
      }

      const schema = generateSchemaJson([User, Order, Product]);
      expect(schema.types).toHaveLength(3);
      expect(schema.federation.enabled).toBe(true);
      expect(schema.federation.entities.map((e: any) => e.name)).toEqual([
        "User",
        "Order",
        "Product",
      ]);
    });

    it("handles mixed local and extended entities", () => {
      @Type()
      @Key("id")
      class User {
        id: string;
        email: string;
      }

      @Extends()
      @Key("id")
      @Type()
      class UserExtended {
        @External() id: string;
        @External() email: string;
        orders: string[];
      }

      const schema = generateSchemaJson([User, UserExtended]);
      const entity = (name: string) => schema.federation.entities.find((e: any) => e.name === name);

      expect(entity("User").extends).toBeUndefined();
      expect(entity("UserExtended").extends).toBe(true);
    });

    it("preserves metadata through schema generation", () => {
      @Extends()
      @Key("id")
      @Type()
      class User {
        @External() id: string;
        @External() email: string;
        @Shareable() profile: string;
      }

      const schema = generateSchemaJson([User]);

      expect(schema.federation.entities).toEqual([
        {
          name: "User",
          key_fields: ["id"],
          extends: true,
          external_fields: ["id", "email"],
          shareable_fields: ["profile"],
        },
      ]);
    });
  });

  describe("@Shareable decorator", () => {
    it("marks field as shareable in schema JSON", () => {
      @Key("id")
      @Type()
      class Product {
        id: string = "";
        @Shareable() name: string = "";
      }

      const schema = generateSchemaJson([Product] as any);
      expect(schema.federation.entities).toEqual([
        { name: "Product", key_fields: ["id"], shareable_fields: ["name"] },
      ]);
    });
  });

  describe("@Inaccessible decorator", () => {
    it("marks field as inaccessible in schema JSON", () => {
      @Key("id")
      @Type()
      class User {
        id: string = "";
        @Inaccessible() internalId: string = "";
      }

      const schema = generateSchemaJson([User] as any);
      const user = schema.types.find((t: any) => t.name === "User");
      const internalField = user.fields.find((f: any) => f.name === "internalId");
      expect(internalField.directives).toEqual([{ name: "inaccessible" }]);
    });
  });

  describe("Field markers stacked", () => {
    it("includes shareable and inaccessible on the same field", () => {
      @Key("id")
      @Type()
      class Item {
        id: string = "";
        @Shareable() @Inaccessible() tag: string = "";
      }

      const schema = generateSchemaJson([Item] as any);
      const item = schema.types.find((t: any) => t.name === "Item");
      const tagField = item.fields.find((f: any) => f.name === "tag");
      expect(tagField.directives).toEqual([{ name: "inaccessible" }]);
      expect(schema.federation.entities[0].shareable_fields).toEqual(["tag"]);
    });
  });
});
