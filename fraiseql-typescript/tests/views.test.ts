/**
 * Tests for DDL generation helpers (views.ts).
 *
 * This test suite verifies:
 * - All functions work correctly with valid inputs
 * - Error handling for invalid inputs
 * - Generated SQL syntax is correct
 * - All interfaces are properly typed
 */

import * as fs from "fs";
import * as path from "path";
import {
  loadSchema,
  generateTvDdl,
  generateTaDdl,
  generateCompositionViews,
  suggestRefreshStrategy,
  validateGeneratedDdl,
  type SchemaObject,
  type GenerateTvOptions,
  type GenerateTaOptions,
} from "../src/views";

// Helper to create a test schema
function createTestSchema(): SchemaObject {
  return {
    types: [
      {
        name: "User",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "name", type: "String", nullable: false },
          { name: "email", type: "String", nullable: false },
          { name: "created_at", type: "DateTime", nullable: false },
        ],
        relationships: [
          { name: "posts", target_entity: "Post", cardinality: "many" },
        ],
      },
      {
        name: "Post",
        fields: [
          { name: "id", type: "Int", nullable: false },
          { name: "title", type: "String", nullable: false },
          { name: "content", type: "String", nullable: false },
          { name: "user_id", type: "Int", nullable: false },
        ],
        relationships: [],
      },
    ],
    queries: {},
    mutations: {},
  };
}

describe("DDL Generation Helpers", () => {
  describe("loadSchema", () => {
    it("should load a valid schema from file", () => {
      // Create a temporary test file
      const testFile = path.join(__dirname, "test-schema.json");
      const schema = createTestSchema();
      fs.writeFileSync(testFile, JSON.stringify(schema, null, 2));

      try {
        const loaded = loadSchema(testFile);
        expect(loaded.types).toHaveLength(2);
        expect(loaded.types[0].name).toBe("User");
      } finally {
        fs.unlinkSync(testFile);
      }
    });

    it("should throw error for missing file", () => {
      expect(() => {
        loadSchema("/nonexistent/schema.json");
      }).toThrow("Schema file not found");
    });

    it("should throw error for invalid JSON", () => {
      const testFile = path.join(__dirname, "invalid.json");
      fs.writeFileSync(testFile, "{ invalid json }");

      try {
        expect(() => {
          loadSchema(testFile);
        }).toThrow("Invalid JSON in schema file");
      } finally {
        fs.unlinkSync(testFile);
      }
    });
  });

  describe("generateTvDdl", () => {
    const schema = createTestSchema();

    it("should generate valid DDL for table-backed JSON view", () => {
      const options: GenerateTvOptions = {
        schema,
        entity: "User",
        view: "user_profile",
        refreshStrategy: "trigger-based",
      };

      const ddl = generateTvDdl(options);

      expect(ddl).toContain("CREATE TABLE tv_user_profile");
      expect(ddl).toContain("entity_id INTEGER NOT NULL UNIQUE");
      expect(ddl).toContain("entity_json JSONB NOT NULL");
      expect(ddl).toContain("is_stale BOOLEAN DEFAULT false");
      expect(ddl).toContain("idx_tv_user_profile_entity_id");
      expect(ddl).toContain("CREATE INDEX IF NOT EXISTS idx_tv_user_profile_entity_json_gin");
    });

    it("should include trigger-based refresh function", () => {
      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        refreshStrategy: "trigger-based",
      });

      expect(ddl).toContain("CREATE OR REPLACE FUNCTION refresh_tv_user_profile()");
      expect(ddl).toContain("TRIGGER trg_refresh_tv_user_profile");
      expect(ddl).toContain("AFTER INSERT OR UPDATE OR DELETE");
    });

    it("should include scheduled refresh function", () => {
      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        refreshStrategy: "scheduled",
      });

      expect(ddl).toContain("CREATE OR REPLACE FUNCTION refresh_tv_user_profile_batch()");
      expect(ddl).toContain("pg_cron");
    });

    it("should include composition views when requested", () => {
      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        includeCompositionViews: true,
      });

      expect(ddl).toContain("cv_User_posts");
      expect(ddl).toContain("batch_compose_User");
    });

    it("should exclude composition views when disabled", () => {
      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        includeCompositionViews: false,
      });

      expect(ddl).not.toContain("batch_compose_User");
    });

    it("should include monitoring functions when requested", () => {
      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        includeMonitoringFunctions: true,
      });

      expect(ddl).toContain("check_staleness_user_profile");
      expect(ddl).toContain("v_staleness_user_profile");
    });

    it("should throw error for missing entity", () => {
      expect(() => {
        generateTvDdl({
          schema,
          entity: "NonExistent",
          view: "test_view",
        });
      }).toThrow("Entity 'NonExistent' not found in schema");
    });

    it("should throw error for invalid entity name", () => {
      expect(() => {
        generateTvDdl({
          schema,
          entity: "123Invalid",
          view: "test_view",
        });
      }).toThrow("Invalid entity name");
    });

    it("should throw error for invalid view name", () => {
      expect(() => {
        generateTvDdl({
          schema,
          entity: "User",
          view: "123-invalid",
        });
      }).toThrow("Invalid view name");
    });
  });

  describe("generateTaDdl", () => {
    const schema = createTestSchema();

    it("should generate valid DDL for table-backed Arrow view", () => {
      const options: GenerateTaOptions = {
        schema,
        entity: "User",
        view: "user_stats",
        refreshStrategy: "scheduled",
      };

      const ddl = generateTaDdl(options);

      expect(ddl).toContain("CREATE TABLE ta_user_stats");
      expect(ddl).toContain("batch_number INTEGER NOT NULL");
      expect(ddl).toContain("col_id BYTEA");
      expect(ddl).toContain("col_name BYTEA");
      expect(ddl).toContain("row_count INTEGER NOT NULL DEFAULT 0");
      expect(ddl).toContain("batch_size_bytes BIGINT");
      expect(ddl).toContain("compression CHAR(10) DEFAULT 'none'");
    });

    it("should generate Arrow columns for each field", () => {
      const ddl = generateTaDdl({
        schema,
        entity: "User",
        view: "user_stats",
      });

      expect(ddl).toContain("col_id BYTEA");
      expect(ddl).toContain("col_name BYTEA");
      expect(ddl).toContain("col_email BYTEA");
      expect(ddl).toContain("col_created_at BYTEA");
    });

    it("should include batch metadata columns", () => {
      const ddl = generateTaDdl({
        schema,
        entity: "User",
        view: "user_stats",
      });

      expect(ddl).toContain("dictionary_encoded_fields TEXT[]");
      expect(ddl).toContain("field_compression_codecs TEXT[]");
      expect(ddl).toContain("last_materialized_row_count BIGINT");
      expect(ddl).toContain("estimated_decode_time_ms INTEGER");
    });

    it("should throw error for missing entity", () => {
      expect(() => {
        generateTaDdl({
          schema,
          entity: "NonExistent",
          view: "test_view",
        });
      }).toThrow("Entity 'NonExistent' not found in schema");
    });
  });

  describe("generateCompositionViews", () => {
    const schema = createTestSchema();

    it("should generate composition views for relationships", () => {
      const sql = generateCompositionViews({
        schema,
        entity: "User",
        relationships: ["posts"],
      });

      expect(sql).toContain("cv_User_posts");
      expect(sql).toContain("LEFT JOIN");
      expect(sql).toContain("batch_compose_User");
    });

    it("should generate multiple composition views", () => {
      const sql = generateCompositionViews({
        schema,
        entity: "User",
        relationships: ["posts", "comments"],
      });

      expect(sql).toContain("cv_User_posts");
      expect(sql).toContain("cv_User_comments");
      expect(sql).toContain("batch_compose_User");
    });

    it("should handle empty relationships", () => {
      const sql = generateCompositionViews({
        schema,
        entity: "User",
        relationships: [],
      });

      expect(sql).toContain("No composition views generated");
    });
  });

  describe("suggestRefreshStrategy", () => {
    it("should suggest trigger-based for high write volume", () => {
      const strategy = suggestRefreshStrategy({
        writeVolumePerMinute: 500,
        latencyRequirementMs: 1000,
        readVolumePerSecond: 10,
      });

      expect(strategy).toBe("trigger-based");
    });

    it("should suggest trigger-based for strict latency requirement", () => {
      const strategy = suggestRefreshStrategy({
        writeVolumePerMinute: 10,
        latencyRequirementMs: 100,
        readVolumePerSecond: 10,
      });

      expect(strategy).toBe("trigger-based");
    });

    it("should suggest trigger-based for high read volume with strict latency", () => {
      const strategy = suggestRefreshStrategy({
        writeVolumePerMinute: 50,
        latencyRequirementMs: 800,
        readVolumePerSecond: 100,
      });

      expect(strategy).toBe("trigger-based");
    });

    it("should suggest scheduled for low write volume", () => {
      const strategy = suggestRefreshStrategy({
        writeVolumePerMinute: 10,
        latencyRequirementMs: 5000,
        readVolumePerSecond: 1,
      });

      expect(strategy).toBe("scheduled");
    });
  });

  describe("validateGeneratedDdl", () => {
    it("should validate correct DDL", () => {
      const sql = "CREATE TABLE test (id INT PRIMARY KEY);";
      const errors = validateGeneratedDdl(sql);

      expect(errors.filter((e) => !e.startsWith("Warning"))).toHaveLength(0);
    });

    it("should detect empty DDL", () => {
      const errors = validateGeneratedDdl("");
      expect(errors).toContain("Generated DDL is empty");
    });

    it("should detect unbalanced parentheses", () => {
      const sql = "CREATE TABLE test (id INT PRIMARY KEY;";
      const errors = validateGeneratedDdl(sql);

      expect(errors.some((e) => e.includes("Unbalanced parentheses"))).toBe(true);
    });

    it("should detect unbalanced quotes", () => {
      const sql = "CREATE TABLE test ('id INT PRIMARY KEY);";
      const errors = validateGeneratedDdl(sql);

      expect(errors.some((e) => e.includes("quote"))).toBe(true);
    });

    it("should warn about missing CREATE statement", () => {
      const sql = "DROP TABLE test;";
      const errors = validateGeneratedDdl(sql);

      expect(errors.some((e) => e.includes("CREATE"))).toBe(true);
    });

    it("should warn about missing COMMENT statements", () => {
      const sql = "CREATE TABLE test (id INT);";
      const errors = validateGeneratedDdl(sql);

      expect(errors.some((e) => e.includes("COMMENT"))).toBe(true);
    });
  });

  describe("End-to-end workflow", () => {
    it("should generate complete workflow: tv_ with all components", () => {
      const schema = createTestSchema();

      const ddl = generateTvDdl({
        schema,
        entity: "User",
        view: "user_profile",
        refreshStrategy: "trigger-based",
        includeCompositionViews: true,
        includeMonitoringFunctions: true,
      });

      // Verify all major components
      expect(ddl).toContain("CREATE TABLE tv_user_profile");
      expect(ddl).toContain("cv_User_posts");
      expect(ddl).toContain("batch_compose_User");
      expect(ddl).toContain("refresh_tv_user_profile()");
      expect(ddl).toContain("check_staleness_user_profile");
      expect(ddl).toContain("v_staleness_user_profile");

      // Verify it's syntactically valid
      const errors = validateGeneratedDdl(ddl);
      const criticalErrors = errors.filter((e) => !e.startsWith("Warning"));
      expect(criticalErrors).toHaveLength(0);
    });

    it("should generate complete workflow: ta_ with all components", () => {
      const schema = createTestSchema();

      const ddl = generateTaDdl({
        schema,
        entity: "User",
        view: "user_stats",
        refreshStrategy: "scheduled",
        includeMonitoringFunctions: true,
      });

      // Verify all major components
      expect(ddl).toContain("CREATE TABLE ta_user_stats");
      expect(ddl).toContain("col_id BYTEA");
      expect(ddl).toContain("refresh_ta_user_stats_batch()");
      expect(ddl).toContain("check_staleness_user_stats");

      // Verify it's syntactically valid
      const errors = validateGeneratedDdl(ddl);
      const criticalErrors = errors.filter((e) => !e.startsWith("Warning"));
      expect(criticalErrors).toHaveLength(0);
    });
  });
});
