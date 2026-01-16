import { SchemaRegistry } from "../src/registry";

describe("SchemaRegistry", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("registerType", () => {
    it("should register a type with fields", () => {
      SchemaRegistry.registerType("User", [
        { name: "id", type: "Int", nullable: false },
        { name: "name", type: "String", nullable: false },
        { name: "email", type: "String", nullable: true },
      ]);

      const schema = SchemaRegistry.getSchema();
      expect(schema.types).toHaveLength(1);
      expect(schema.types[0].name).toBe("User");
      expect(schema.types[0].fields).toHaveLength(3);
    });

    it("should register a type with description", () => {
      SchemaRegistry.registerType(
        "User",
        [{ name: "id", type: "Int", nullable: false }],
        "Represents a user in the system"
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.types[0].description).toBe("Represents a user in the system");
    });
  });

  describe("registerQuery", () => {
    it("should register a simple query", () => {
      SchemaRegistry.registerQuery(
        "user",
        "User",
        false,
        true,
        [{ name: "id", type: "Int", nullable: false }],
        "Get a single user by ID",
        { sql_source: "v_user" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.queries).toHaveLength(1);
      expect(schema.queries[0].name).toBe("user");
      expect(schema.queries[0].return_type).toBe("User");
      expect(schema.queries[0].sql_source).toBe("v_user");
    });

    it("should register a list query", () => {
      SchemaRegistry.registerQuery(
        "users",
        "[User!]",
        true,
        false,
        [{ name: "limit", type: "Int", nullable: false, default: 10 }],
        "Get all users",
        { sql_source: "v_user" }
      );

      const schema = SchemaRegistry.getSchema();
      const query = schema.queries[0];
      expect(query.returns_list).toBe(true);
      expect(query.return_type).toBe("User"); // Should be cleaned
      expect(query.arguments[0].default).toBe(10);
    });
  });

  describe("registerMutation", () => {
    it("should register a CREATE mutation", () => {
      SchemaRegistry.registerMutation(
        "createUser",
        "User",
        false,
        false,
        [
          { name: "name", type: "String", nullable: false },
          { name: "email", type: "String", nullable: false },
        ],
        "Create a new user",
        { sql_source: "fn_create_user", operation: "CREATE" }
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.mutations).toHaveLength(1);
      expect(schema.mutations[0].name).toBe("createUser");
      expect(schema.mutations[0].operation).toBe("CREATE");
    });
  });

  describe("registerFactTable", () => {
    it("should register a fact table", () => {
      SchemaRegistry.registerFactTable(
        "tf_sales",
        [
          { name: "revenue", sql_type: "Float", nullable: false },
          { name: "quantity", sql_type: "Int", nullable: false },
        ],
        {
          name: "data",
          paths: [{ name: "category", json_path: "data->>'category'", data_type: "text" }],
        },
        [{ name: "customer_id", sql_type: "Text", indexed: true }]
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.fact_tables).toHaveLength(1);
      expect(schema.fact_tables![0].table_name).toBe("tf_sales");
      expect(schema.fact_tables![0].measures).toHaveLength(2);
    });
  });

  describe("registerAggregateQuery", () => {
    it("should register an aggregate query", () => {
      SchemaRegistry.registerAggregateQuery(
        "salesAggregate",
        "tf_sales",
        true,
        true,
        "Sales aggregation"
      );

      const schema = SchemaRegistry.getSchema();
      expect(schema.aggregate_queries).toHaveLength(1);
      expect(schema.aggregate_queries![0].name).toBe("salesAggregate");
      expect(schema.aggregate_queries![0].fact_table).toBe("tf_sales");
    });
  });

  describe("getSchema", () => {
    it("should return empty schema initially", () => {
      const schema = SchemaRegistry.getSchema();
      expect(schema.types).toHaveLength(0);
      expect(schema.queries).toHaveLength(0);
      expect(schema.mutations).toHaveLength(0);
    });

    it("should include fact_tables only if present", () => {
      const schema = SchemaRegistry.getSchema();
      expect(schema.fact_tables).toBeUndefined();

      SchemaRegistry.registerFactTable("tf_sales", [], { name: "data", paths: [] }, []);
      const schemaWithFactTable = SchemaRegistry.getSchema();
      expect(schemaWithFactTable.fact_tables).toBeDefined();
    });
  });

  describe("clear", () => {
    it("should clear all registered definitions", () => {
      SchemaRegistry.registerType("User", [{ name: "id", type: "Int", nullable: false }]);
      SchemaRegistry.registerQuery("user", "User", false, true, []);

      SchemaRegistry.clear();

      const schema = SchemaRegistry.getSchema();
      expect(schema.types).toHaveLength(0);
      expect(schema.queries).toHaveLength(0);
    });
  });
});
