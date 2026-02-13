import { SchemaRegistry, field, registerTypeFields } from "../src/index";

describe("Field-Level Metadata", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("field() helper function", () => {
    it("should create metadata with requires_scope", () => {
      const metadata = field({ requiresScope: "read:User.salary" });

      expect(metadata.requiresScope).toBe("read:User.salary");
      expect(metadata.deprecated).toBeUndefined();
      expect(metadata.description).toBeUndefined();
    });

    it("should create metadata with multiple scopes", () => {
      const metadata = field({ requiresScope: ["read:User.salary", "admin:view"] });

      expect(Array.isArray(metadata.requiresScope)).toBe(true);
      expect(metadata.requiresScope).toEqual(["read:User.salary", "admin:view"]);
    });

    it("should create metadata with deprecated flag", () => {
      const metadata = field({ deprecated: true });

      expect(metadata.deprecated).toBe(true);
    });

    it("should create metadata with deprecation reason", () => {
      const metadata = field({ deprecated: "Use newField instead" });

      expect(metadata.deprecated).toBe("Use newField instead");
    });

    it("should create metadata with description", () => {
      const metadata = field({ description: "User salary information" });

      expect(metadata.description).toBe("User salary information");
    });

    it("should combine multiple metadata options", () => {
      const metadata = field({
        requiresScope: "read:User.salary",
        deprecated: "Use totalCompensation instead",
        description: "Annual salary (deprecated)",
      });

      expect(metadata.requiresScope).toBe("read:User.salary");
      expect(metadata.deprecated).toBe("Use totalCompensation instead");
      expect(metadata.description).toBe("Annual salary (deprecated)");
    });
  });

  describe("registerTypeFields with metadata", () => {
    it("should register field with requires_scope", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "read:User.salary",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const userType = schema.types[0];
      const salaryField = userType.fields.find((f) => f.name === "salary");

      expect(salaryField).toBeDefined();
      expect(salaryField?.requiresScope).toBe("read:User.salary");
    });

    it("should register field with multiple scopes", () => {
      registerTypeFields("User", [
        {
          name: "ssn",
          type: "String",
          nullable: false,
          requiresScope: ["read:User.ssn", "hr:view_pii"],
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const ssnField = schema.types[0].fields[0];

      expect(Array.isArray(ssnField.requiresScope)).toBe(true);
      expect(ssnField.requiresScope).toEqual(["read:User.ssn", "hr:view_pii"]);
    });

    it("should register field with deprecated marker", () => {
      registerTypeFields("User", [
        {
          name: "oldEmail",
          type: "String",
          nullable: true,
          deprecated: "Use email instead",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const oldEmailField = schema.types[0].fields[0];

      expect(oldEmailField.deprecated).toBe("Use email instead");
    });

    it("should register field with description", () => {
      registerTypeFields("User", [
        {
          name: "name",
          type: "String",
          nullable: false,
          description: "Full name of the user",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const nameField = schema.types[0].fields[0];

      expect(nameField.description).toBe("Full name of the user");
    });

    it("should register field with all metadata combined", () => {
      registerTypeFields("User", [
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "read:User.salary",
          deprecated: "Use totalCompensation instead",
          description: "Annual salary (deprecated - use totalCompensation)",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const salaryField = schema.types[0].fields[0];

      expect(salaryField.requiresScope).toBe("read:User.salary");
      expect(salaryField.deprecated).toBe("Use totalCompensation instead");
      expect(salaryField.description).toBe(
        "Annual salary (deprecated - use totalCompensation)"
      );
    });

    it("should preserve metadata in multi-field registration", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "email",
          type: "Email",
          nullable: false,
          description: "User email address",
        },
        {
          name: "name",
          type: "String",
          nullable: false,
        },
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "read:User.salary",
        },
        {
          name: "ssn",
          type: "String",
          nullable: true,
          requiresScope: "hr:view_pii",
          deprecated: "Use nationalId instead",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const userType = schema.types[0];

      expect(userType.fields).toHaveLength(5);

      const emailField = userType.fields.find((f) => f.name === "email");
      expect(emailField?.description).toBe("User email address");

      const salaryField = userType.fields.find((f) => f.name === "salary");
      expect(salaryField?.requiresScope).toBe("read:User.salary");

      const ssnField = userType.fields.find((f) => f.name === "ssn");
      expect(ssnField?.requiresScope).toBe("hr:view_pii");
      expect(ssnField?.deprecated).toBe("Use nationalId instead");
    });

    it("should handle fields without metadata", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      const userType = schema.types[0];

      expect(userType.fields[0].requiresScope).toBeUndefined();
      expect(userType.fields[0].deprecated).toBeUndefined();
      expect(userType.fields[0].description).toBeUndefined();
    });

    it("should support default values with metadata", () => {
      registerTypeFields("User", [
        {
          name: "role",
          type: "String",
          nullable: false,
          default: "user",
          description: "User role",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const roleField = schema.types[0].fields[0];

      expect(roleField.default).toBe("user");
      expect(roleField.description).toBe("User role");
    });
  });

  describe("Interface fields with metadata", () => {
    it("should register interface with field metadata", () => {
      SchemaRegistry.registerInterface("Node", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "createdAt",
          type: "DateTime",
          nullable: false,
          description: "Creation timestamp",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const nodeInterface = schema.interfaces![0];

      expect(nodeInterface.fields[1].description).toBe("Creation timestamp");
    });
  });

  describe("Input type fields with metadata", () => {
    it("should register input type with field metadata and defaults", () => {
      SchemaRegistry.registerInputType("CreateUserInput", [
        { name: "email", type: "Email", nullable: false },
        {
          name: "name",
          type: "String",
          nullable: false,
          description: "User full name",
        },
        {
          name: "role",
          type: "String",
          nullable: false,
          default: "user",
          description: "User role (defaults to user)",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const inputType = schema.input_types![0];

      expect(inputType.fields[1].description).toBe("User full name");
      expect(inputType.fields[2].default).toBe("user");
      expect(inputType.fields[2].description).toBe("User role (defaults to user)");
    });
  });

  describe("Schema export with field metadata", () => {
    it("should export schema with all field metadata", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "email",
          type: "Email",
          nullable: false,
          description: "Email address",
        },
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "read:User.salary",
        },
        {
          name: "oldPhone",
          type: "String",
          nullable: true,
          deprecated: "Use phoneNumber instead",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const json = JSON.stringify(schema, null, 2);

      expect(json).toContain("email");
      expect(json).toContain("Email address");
      expect(json).toContain("salary");
      expect(json).toContain("read:User.salary");
      expect(json).toContain("oldPhone");
      expect(json).toContain("Use phoneNumber instead");
    });

    it("should preserve field metadata when round-tripping through JSON", () => {
      registerTypeFields("User", [
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: ["read:User.salary", "admin:view"],
          deprecated: "Use totalCompensation",
          description: "Annual salary",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const json = JSON.stringify(schema);
      const parsed = JSON.parse(json);

      const salaryField = parsed.types[0].fields[0];
      expect(salaryField.requiresScope).toEqual(["read:User.salary", "admin:view"]);
      expect(salaryField.deprecated).toBe("Use totalCompensation");
      expect(salaryField.description).toBe("Annual salary");
    });
  });

  describe("Common use cases", () => {
    it("should support field-level access control pattern", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        { name: "email", type: "String", nullable: false },
        {
          name: "ssn",
          type: "String",
          nullable: false,
          requiresScope: "pii:read",
        },
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "hr:read_compensation",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const userType = schema.types[0];

      const ssnField = userType.fields.find((f) => f.name === "ssn");
      expect(ssnField?.requiresScope).toBe("pii:read");

      const salaryField = userType.fields.find((f) => f.name === "salary");
      expect(salaryField?.requiresScope).toBe("hr:read_compensation");
    });

    it("should support API versioning with deprecation", () => {
      registerTypeFields("Product", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "name",
          type: "String",
          nullable: false,
          description: "Product name",
        },
        {
          name: "oldPrice",
          type: "Decimal",
          nullable: true,
          deprecated: "Use pricing.list instead",
        },
        {
          name: "oldCategory",
          type: "String",
          nullable: true,
          deprecated: "Use categories instead",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const productType = schema.types[0];

      const deprecatedFields = productType.fields.filter((f) => f.deprecated);
      expect(deprecatedFields).toHaveLength(2);
    });

    it("should support rich field documentation", () => {
      registerTypeFields("Order", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "items",
          type: "OrderItem",
          nullable: false,
          description: "Line items in the order. Read requires order:details scope.",
        },
        {
          name: "discountApplied",
          type: "Decimal",
          nullable: true,
          requiresScope: "orders:view_discounts",
          description: "Discount amount applied to this order. Requires orders:view_discounts scope.",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const orderType = schema.types[0];

      const itemsField = orderType.fields.find((f) => f.name === "items");
      expect(itemsField?.description).toContain("Line items");

      const discountField = orderType.fields.find((f) => f.name === "discountApplied");
      expect(discountField?.description).toContain("orders:view_discounts");
    });
  });

  describe("Backward compatibility", () => {
    it("should work with fields that don't have metadata", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.types[0].fields).toHaveLength(2);
      expect(schema.types[0].fields[0].name).toBe("id");
    });

    it("should allow mixing fields with and without metadata", () => {
      registerTypeFields("User", [
        { name: "id", type: "ID", nullable: false },
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "read:User.salary",
        },
        { name: "name", type: "String", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      const userType = schema.types[0];

      expect(userType.fields[0].requiresScope).toBeUndefined();
      expect(userType.fields[1].requiresScope).toBe("read:User.salary");
      expect(userType.fields[2].requiresScope).toBeUndefined();
    });
  });
});
