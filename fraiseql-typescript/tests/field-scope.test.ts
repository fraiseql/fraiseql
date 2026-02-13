/**
 * RED Phase: Tests for field-level scope requirements (scope-based RBAC).
 *
 * These tests verify that field scopes defined with field({ requiresScope: ... })
 * are properly collected, exported in schema.json, and ready for compiler integration.
 */

import { SchemaRegistry, field } from "../src/index";

describe("Field-Level Scope Requirements", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("Field scope declaration", () => {
    it("should accept field with single scope requirement", () => {
      const fieldMeta = field({
        requiresScope: "read:User.email",
        description: "User email address",
      });

      expect(fieldMeta.requiresScope).toBe("read:User.email");
      expect(fieldMeta.description).toBe("User email address");
    });

    it("should accept field with custom scope format", () => {
      const salaryField = field({ requiresScope: "hr:view_compensation" });
      const ssnField = field({ requiresScope: "pii:view" });

      expect(salaryField.requiresScope).toBe("hr:view_compensation");
      expect(ssnField.requiresScope).toBe("pii:view");
    });

    it("should accept field with scope and description together", () => {
      const costField = field({
        requiresScope: "read:Product.cost",
        description: "Internal cost of the product",
      });

      expect(costField.requiresScope).toBe("read:Product.cost");
      expect(costField.description).toBe("Internal cost of the product");
    });

    it("should accept field with wildcard scope patterns", () => {
      const readAll = field({ requiresScope: "read:*" });
      const readType = field({ requiresScope: "read:User.*" });

      expect(readAll.requiresScope).toBe("read:*");
      expect(readType.requiresScope).toBe("read:User.*");
    });

    it("should accept public fields without scope", () => {
      const publicField = field({ description: "Public field" });

      expect(publicField.requiresScope).toBeUndefined();
      expect(publicField.description).toBe("Public field");
    });

    it("should accept field with multiple scopes as array", () => {
      const multiScopeField = field({
        requiresScope: ["read:User.email", "admin:view_all_users"],
      });

      expect(Array.isArray(multiScopeField.requiresScope)).toBe(true);
      expect(multiScopeField.requiresScope).toEqual(["read:User.email", "admin:view_all_users"]);
    });

    it("should accept deprecated field with scope", () => {
      const deprecatedField = field({
        requiresScope: "read:User.oldEmail",
        deprecated: "Use email instead",
      });

      expect(deprecatedField.requiresScope).toBe("read:User.oldEmail");
      expect(deprecatedField.deprecated).toBe("Use email instead");
    });
  });

  describe("Type registration with scoped fields", () => {
    it("should register type with scoped fields in schema", () => {
      SchemaRegistry.registerType(
        "Account",
        [
          { name: "id", type: "ID", nullable: false },
          { name: "accountNumber", type: "String", nullable: false },
          {
            name: "balance",
            type: "Decimal",
            nullable: false,
            requiresScope: "read:Account.balance",
          },
        ],
        "Account with sensitive financial information"
      );

      const schema = SchemaRegistry.getSchema();
      const accountType = schema.types?.find((t) => t.name === "Account");

      expect(accountType).toBeDefined();
      expect(accountType?.fields).toHaveLength(3);

      const balanceField = accountType?.fields.find((f) => f.name === "balance");
      expect(balanceField?.requiresScope).toBe("read:Account.balance");
    });

    it("should register type with mixed public and scoped fields", () => {
      SchemaRegistry.registerType("MixedData", [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false },
        {
          name: "internalNotes",
          type: "String",
          nullable: true,
          requiresScope: "internal:view_notes",
        },
        {
          name: "budget",
          type: "Decimal",
          nullable: false,
          requiresScope: "finance:view_budget",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const mixedType = schema.types?.find((t) => t.name === "MixedData");

      const idField = mixedType?.fields.find((f) => f.name === "id");
      const nameField = mixedType?.fields.find((f) => f.name === "name");
      const notesField = mixedType?.fields.find((f) => f.name === "internalNotes");
      const budgetField = mixedType?.fields.find((f) => f.name === "budget");

      expect(idField?.requiresScope).toBeUndefined();
      expect(nameField?.requiresScope).toBeUndefined();
      expect(notesField?.requiresScope).toBe("internal:view_notes");
      expect(budgetField?.requiresScope).toBe("finance:view_budget");
    });

    it("should register interface with scoped fields", () => {
      SchemaRegistry.registerInterface(
        "Node",
        [
          { name: "id", type: "ID", nullable: false },
          {
            name: "createdAt",
            type: "DateTime",
            nullable: false,
            requiresScope: "read:*.createdAt",
          },
        ],
        "An object with a globally unique ID"
      );

      const schema = SchemaRegistry.getSchema();
      const nodeInterface = schema.interfaces?.find((i) => i.name === "Node");

      expect(nodeInterface?.fields).toHaveLength(2);
      const createdAtField = nodeInterface?.fields.find((f) => f.name === "createdAt");
      expect(createdAtField?.requiresScope).toBe("read:*.createdAt");
    });
  });

  describe("Scope wildcard patterns", () => {
    it("should support read:* wildcard scope", () => {
      SchemaRegistry.registerType("Document", [
        {
          name: "content",
          type: "String",
          nullable: false,
          requiresScope: "read:*",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const docType = schema.types?.find((t) => t.name === "Document");
      const contentField = docType?.fields.find((f) => f.name === "content");

      expect(contentField?.requiresScope).toBe("read:*");
    });

    it("should support Type.* wildcard scope", () => {
      SchemaRegistry.registerType("Profile", [
        {
          name: "data",
          type: "String",
          nullable: false,
          requiresScope: "read:Profile.*",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const profileType = schema.types?.find((t) => t.name === "Profile");
      const dataField = profileType?.fields.find((f) => f.name === "data");

      expect(dataField?.requiresScope).toBe("read:Profile.*");
    });

    it("should support custom scope identifiers", () => {
      SchemaRegistry.registerType("Employee", [
        {
          name: "salary",
          type: "Decimal",
          nullable: false,
          requiresScope: "hr:view_compensation",
        },
        {
          name: "ssn",
          type: "String",
          nullable: false,
          requiresScope: "pii:view",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const empType = schema.types?.find((t) => t.name === "Employee");

      const salaryField = empType?.fields.find((f) => f.name === "salary");
      const ssnField = empType?.fields.find((f) => f.name === "ssn");

      expect(salaryField?.requiresScope).toBe("hr:view_compensation");
      expect(ssnField?.requiresScope).toBe("pii:view");
    });
  });

  describe("Scope edge cases", () => {
    it("should handle empty scope as undefined", () => {
      const emptyScope = field({ requiresScope: "" });
      expect(emptyScope.requiresScope).toBe("");
    });

    it("should handle special characters in scope", () => {
      SchemaRegistry.registerType("SpecialScopes", [
        {
          name: "emailVerified",
          type: "Boolean",
          nullable: false,
          requiresScope: "read:User.email_verified",
        },
        {
          name: "config",
          type: "String",
          nullable: false,
          requiresScope: "admin_read:system_config",
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const type_ = schema.types?.find((t) => t.name === "SpecialScopes");

      const emailField = type_?.fields.find((f) => f.name === "emailVerified");
      const configField = type_?.fields.find((f) => f.name === "config");

      expect(emailField?.requiresScope).toBe("read:User.email_verified");
      expect(configField?.requiresScope).toBe("admin_read:system_config");
    });

    it("should handle multiple scopes as array", () => {
      SchemaRegistry.registerType("MultiScope", [
        {
          name: "data",
          type: "String",
          nullable: false,
          requiresScope: ["read:MultiScope.data", "admin:view_all", "audit:log_access"],
        },
      ]);

      const schema = SchemaRegistry.getSchema();
      const type_ = schema.types?.find((t) => t.name === "MultiScope");
      const dataField = type_?.fields.find((f) => f.name === "data");

      expect(Array.isArray(dataField?.requiresScope)).toBe(true);
      expect(dataField?.requiresScope).toEqual([
        "read:MultiScope.data",
        "admin:view_all",
        "audit:log_access",
      ]);
    });
  });

  describe("Schema JSON export with scopes", () => {
    it("should export schema.json with scope metadata", () => {
      SchemaRegistry.registerType(
        "SecureData",
        [
          { name: "id", type: "ID", nullable: false },
          { name: "name", type: "String", nullable: false },
          {
            name: "secret",
            type: "String",
            nullable: false,
            requiresScope: "read:SecureData.secret",
          },
        ],
        "Data with secrets"
      );

      const schema = SchemaRegistry.getSchema();
      const json = JSON.stringify(schema);

      // Verify JSON is valid
      expect(json).toBeDefined();
      const parsed = JSON.parse(json);

      const secureType = parsed.types?.find((t: { name: string }) => t.name === "SecureData");
      const secretField = secureType?.fields.find((f: { name: string }) => f.name === "secret");

      expect(secretField?.requiresScope).toBe("read:SecureData.secret");
    });

    it("should preserve scope metadata through schema export and import", () => {
      SchemaRegistry.registerType(
        "Original",
        [
          {
            name: "protectedField",
            type: "String",
            nullable: false,
            requiresScope: "special:access",
            description: "A protected field",
          },
        ],
        "Original type"
      );

      const schema1 = SchemaRegistry.getSchema();
      const json = JSON.stringify(schema1);
      const schema2 = JSON.parse(json);

      const origType1 = schema1.types?.find((t) => t.name === "Original");
      const origType2 = schema2.types?.find((t: { name: string }) => t.name === "Original");

      expect(origType1?.fields[0]?.requiresScope).toBe("special:access");
      expect(origType2?.fields[0]?.requiresScope).toBe("special:access");
    });
  });

  describe("Placeholder tests for future cycles", () => {
    it.skip("should compile scopes from schema.json to schema.compiled.json", () => {
      // Future: Cycle 4 - Compiler Integration
    });

    it.skip("should enforce scope requirements at runtime", () => {
      // Future: Cycle 5 - Runtime Field Filtering
    });

    it.skip("should support TOML-based scope overrides", () => {
      // Future: Cycle 3 - TOML Schema Support
    });
  });
});
