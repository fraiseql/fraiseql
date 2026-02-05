import { SchemaRegistry, enum_, interface_, union, input } from "../src/index";

describe("Type System Decorators", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("enum_ decorator", () => {
    it("should create a basic enum", () => {
      enum_("OrderStatus", {
        PENDING: "pending",
        SHIPPED: "shipped",
        DELIVERED: "delivered",
      });

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums).toHaveLength(1);
      expect(schema.enums![0].name).toBe("OrderStatus");
      expect(schema.enums![0].values).toHaveLength(3);
      expect(schema.enums![0].values[0].name).toBe("PENDING");
    });

    it("should create enum with description", () => {
      enum_("Status", {}, { description: "Order status" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums![0].description).toBe("Order status");
    });

    it("should register multiple enum values", () => {
      enum_("Color", {
        RED: "red",
        GREEN: "green",
        BLUE: "blue",
      });

      const schema = SchemaRegistry.getSchema();
      const colorEnum = schema.enums![0];
      expect(colorEnum.values).toHaveLength(3);
      expect(colorEnum.values.map((v) => v.name)).toEqual(["RED", "GREEN", "BLUE"]);
    });

    it("should return the values object for backward compatibility", () => {
      const values = { A: "a", B: "b" };
      const result = enum_("MyEnum", values);

      expect(result).toBe(values);
    });

    it("should handle enum with single value", () => {
      enum_("SingleValue", { ONLY: "only" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums![0].values).toHaveLength(1);
    });
  });

  describe("interface_ decorator", () => {
    it("should create a basic interface", () => {
      interface_("Node", [
        { name: "id", type: "ID", nullable: false },
        { name: "createdAt", type: "DateTime", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.interfaces).toHaveLength(1);
      expect(schema.interfaces![0].name).toBe("Node");
      expect(schema.interfaces![0].fields).toHaveLength(2);
    });

    it("should create interface with description", () => {
      interface_("Node", [], { description: "An object with a globally unique ID" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.interfaces![0].description).toBe("An object with a globally unique ID");
    });

    it("should support nullable and non-nullable fields", () => {
      interface_("SearchResult", [
        { name: "id", type: "ID", nullable: false },
        { name: "description", type: "String", nullable: true },
      ]);

      const schema = SchemaRegistry.getSchema();
      const iface = schema.interfaces![0];
      expect(iface.fields[0].nullable).toBe(false);
      expect(iface.fields[1].nullable).toBe(true);
    });

    it("should support all scalar types", () => {
      interface_("AllScalars", [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false },
        { name: "age", type: "Int", nullable: false },
        { name: "score", type: "Float", nullable: false },
        { name: "active", type: "Boolean", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      const types = schema.interfaces![0].fields.map((f) => f.type);
      expect(types).toEqual(["ID", "String", "Int", "Float", "Boolean"]);
    });

    it("should return an empty object as marker", () => {
      const result = interface_("Test", []);
      expect(result).toEqual({});
    });
  });

  describe("union decorator", () => {
    it("should create a basic union", () => {
      union("SearchResult", ["User", "Post", "Comment"]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.unions).toHaveLength(1);
      expect(schema.unions![0].name).toBe("SearchResult");
      expect(schema.unions![0].member_types).toEqual(["User", "Post", "Comment"]);
    });

    it("should create union with description", () => {
      union("SearchResult", [], { description: "Result of a search query" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.unions![0].description).toBe("Result of a search query");
    });

    it("should support single member union", () => {
      union("SingleMember", ["User"]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.unions![0].member_types).toHaveLength(1);
    });

    it("should support multiple member types", () => {
      union("MultiMember", ["User", "Post", "Comment", "Tag"]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.unions![0].member_types).toHaveLength(4);
    });

    it("should return an empty object as marker", () => {
      const result = union("Test", []);
      expect(result).toEqual({});
    });
  });

  describe("input decorator", () => {
    it("should create a basic input type", () => {
      input("CreateUserInput", [
        { name: "name", type: "String", nullable: false },
        { name: "email", type: "String", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.input_types).toHaveLength(1);
      expect(schema.input_types![0].name).toBe("CreateUserInput");
      expect(schema.input_types![0].fields).toHaveLength(2);
    });

    it("should create input with description", () => {
      input("CreateUserInput", [], { description: "Input for creating a new user" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.input_types![0].description).toBe("Input for creating a new user");
    });

    it("should support default values", () => {
      input("CreateUserInput", [
        { name: "name", type: "String", nullable: false },
        { name: "role", type: "String", nullable: false, default: "user" },
      ]);

      const schema = SchemaRegistry.getSchema();
      const fields = schema.input_types![0].fields;
      expect(fields[1].default).toBe("user");
    });

    it("should support nullable and non-nullable fields", () => {
      input("FilterInput", [
        { name: "required", type: "String", nullable: false },
        { name: "optional", type: "String", nullable: true },
      ]);

      const schema = SchemaRegistry.getSchema();
      const fields = schema.input_types![0].fields;
      expect(fields[0].nullable).toBe(false);
      expect(fields[1].nullable).toBe(true);
    });

    it("should support all scalar types", () => {
      input("AllScalarsInput", [
        { name: "id", type: "ID", nullable: false },
        { name: "name", type: "String", nullable: false },
        { name: "age", type: "Int", nullable: false },
        { name: "score", type: "Float", nullable: false },
        { name: "active", type: "Boolean", nullable: false },
      ]);

      const schema = SchemaRegistry.getSchema();
      const types = schema.input_types![0].fields.map((f) => f.type);
      expect(types).toEqual(["ID", "String", "Int", "Float", "Boolean"]);
    });

    it("should return an empty object as marker", () => {
      const result = input("Test", []);
      expect(result).toEqual({});
    });
  });

  describe("Multiple registrations", () => {
    it("should register multiple enums", () => {
      enum_("Status", { ACTIVE: "active", INACTIVE: "inactive" });
      enum_("Priority", { HIGH: "high", LOW: "low" });

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums).toHaveLength(2);
    });

    it("should register enums, interfaces, and unions together", () => {
      enum_("Status", { ACTIVE: "active" });
      interface_("Node", [{ name: "id", type: "ID", nullable: false }]);
      union("SearchResult", ["User", "Post"]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums).toHaveLength(1);
      expect(schema.interfaces).toHaveLength(1);
      expect(schema.unions).toHaveLength(1);
    });

    it("should clear all type system definitions", () => {
      enum_("Status", { ACTIVE: "active" });
      interface_("Node", [{ name: "id", type: "ID", nullable: false }]);
      union("SearchResult", ["User", "Post"]);
      input("FilterInput", [{ name: "query", type: "String", nullable: true }]);

      SchemaRegistry.clear();

      const schema = SchemaRegistry.getSchema();
      expect(schema.enums).toBeUndefined();
      expect(schema.interfaces).toBeUndefined();
      expect(schema.unions).toBeUndefined();
      expect(schema.input_types).toBeUndefined();
    });
  });

  describe("Schema export", () => {
    it("should export schema with all type system definitions", () => {
      enum_("Status", { PENDING: "pending", ACTIVE: "active" });
      interface_("Node", [{ name: "id", type: "ID", nullable: false }]);
      union("SearchResult", ["User", "Post"]);
      input("FilterInput", [{ name: "query", type: "String", nullable: true }]);

      const schema = SchemaRegistry.getSchema();

      expect(schema).toHaveProperty("enums");
      expect(schema).toHaveProperty("interfaces");
      expect(schema).toHaveProperty("unions");
      expect(schema).toHaveProperty("input_types");
      expect(schema.enums).toHaveLength(1);
      expect(schema.interfaces).toHaveLength(1);
      expect(schema.unions).toHaveLength(1);
      expect(schema.input_types).toHaveLength(1);
    });

    it("should not include optional sections if empty", () => {
      const schema = SchemaRegistry.getSchema();

      expect(schema.enums).toBeUndefined();
      expect(schema.interfaces).toBeUndefined();
      expect(schema.unions).toBeUndefined();
      expect(schema.input_types).toBeUndefined();
    });
  });
});
