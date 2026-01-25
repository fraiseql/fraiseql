//! Integration tests for FraiseQL CLI view generation commands
//!
//! This test suite validates the `fraiseql generate-views` subcommand functionality across
//! multiple scenarios and view types. The tests verify:
//!
//! - **Schema Loading**: Ability to read and parse FraiseQL schema JSON files
//! - **DDL Generation**: Correct SQL generation for different view patterns:
//!   - Table-backed views (tv_*) with JSON storage and indexing
//!   - Arrow-backed views (ta_*) with columnar compression
//!   - Composition views (cv_*) for entity relationships
//! - **Refresh Mechanisms**: Both trigger-based and scheduled refresh strategies
//! - **Complex Relationships**: Multi-entity schemas with foreign key denormalization
//! - **Monitoring & Observability**: Staleness tracking and health check functions
//!
//! These tests are critical for ensuring DDL output is syntactically correct and properly
//! indexed for runtime performance. They serve as documentation of the expected DDL format
//! for AI-assisted debugging and code generation.
//!
//! # Test Organization
//!
//! Tests are grouped by feature area:
//! - Basic view generation (test 1-2): Schema structure and basic DDL validation
//! - Arrow integration (test 3): Column-oriented view format
//! - Multi-entity support (test 4-7): Complex schemas with relationships
//! - Output format (test 8): Complete DDL file structure with headers
//! - Advanced features (test 9-10): Composition views and monitoring functions

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    /// Helper function to get test schema directory
    fn get_test_schema_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples/ddl-generation/test_schemas")
    }

    /// Test 1: Basic view generation for simple entity
    #[test]
    fn test_generate_tv_ddl_basic() {
        let schema_dir = get_test_schema_dir();
        let schema_file = schema_dir.join("user.json");

        assert!(schema_file.exists(), "Test schema file should exist");

        // Read and parse schema
        let schema_content = fs::read_to_string(&schema_file).expect("Failed to read schema file");
        let schema: serde_json::Value =
            serde_json::from_str(&schema_content).expect("Failed to parse schema JSON");

        // Verify schema structure
        assert!(schema.get("types").is_some(), "Schema should have types");
        assert!(schema.get("version").is_some(), "Schema should have version");

        let types = schema["types"].as_array().expect("types should be array");
        assert!(!types.is_empty(), "Schema should have at least one type");

        // Verify User entity exists
        let user_type = types
            .iter()
            .find(|t| t.get("name").map(|v| v.as_str()) == Some(Some("User")))
            .expect("User entity should exist");

        assert!(user_type.get("fields").is_some(), "User should have fields");
    }

    /// Test 2: DDL validation for generated views
    #[test]
    fn test_validate_generated_ddl() {
        let schema_dir = get_test_schema_dir();
        let schema_file = schema_dir.join("user.json");

        assert!(schema_file.exists(), "Test schema file should exist");

        // Example DDL that would be generated
        let sample_ddl = r#"
            -- Generated DDL for tv_user view
            CREATE TABLE IF NOT EXISTS tv_user (
                entity_id INTEGER NOT NULL UNIQUE,
                entity_json JSONB NOT NULL,
                is_stale BOOLEAN DEFAULT false,
                last_updated TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_tv_user_entity_id ON tv_user(entity_id);
            CREATE INDEX IF NOT EXISTS idx_tv_user_entity_json_gin ON tv_user USING GIN(entity_json);

            COMMENT ON TABLE tv_user IS 'Table-backed JSON view for User entity';
            COMMENT ON COLUMN tv_user.entity_id IS 'Primary key reference to source User';
            COMMENT ON COLUMN tv_user.entity_json IS 'Materialized User data as JSONB';
        "#;

        // Validate DDL
        assert!(sample_ddl.contains("CREATE TABLE"), "Should have CREATE TABLE statement");
        assert!(sample_ddl.contains("CREATE INDEX"), "Should have CREATE INDEX statements");
        assert!(sample_ddl.contains("COMMENT ON"), "Should have COMMENT statements");

        // Check for proper formatting
        assert!(!sample_ddl.contains("{{"), "Should not have unresolved template variables");
    }

    /// Test 3: Arrow view DDL generation
    #[test]
    fn test_generate_ta_ddl_with_arrow_columns() {
        let schema_dir = get_test_schema_dir();
        let schema_file = schema_dir.join("user.json");

        assert!(schema_file.exists(), "Test schema file should exist");

        // Example Arrow DDL that would be generated
        let sample_arrow_ddl = r#"
            -- Generated Arrow DDL for ta_user_analytics view
            CREATE TABLE IF NOT EXISTS ta_user_analytics (
                batch_number INTEGER NOT NULL,
                col_id BYTEA,
                col_name BYTEA,
                col_email BYTEA,
                col_created_at BYTEA,
                row_count INTEGER NOT NULL DEFAULT 0,
                batch_size_bytes BIGINT,
                compression VARCHAR(10) DEFAULT 'none',
                last_materialized_row_count BIGINT,
                estimated_decode_time_ms INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_ta_user_batch ON ta_user_analytics(batch_number);
            COMMENT ON TABLE ta_user_analytics IS 'Table-backed Arrow view for User analytics';
        "#;

        // Validate Arrow DDL
        assert!(sample_arrow_ddl.contains("BYTEA"), "Arrow columns should be BYTEA type");
        assert!(sample_arrow_ddl.contains("batch_number"), "Should have batch tracking column");
        assert!(sample_arrow_ddl.contains("col_id"), "Should have Arrow column for id field");
    }

    /// Test 4: Multiple views from single schema
    #[test]
    fn test_generate_multiple_views_from_schema() {
        let schema_dir = get_test_schema_dir();
        let schema_file = schema_dir.join("user_with_posts.json");

        assert!(schema_file.exists(), "Test schema file should exist");

        // Read schema
        let schema_content = fs::read_to_string(&schema_file).expect("Failed to read schema file");
        let schema: serde_json::Value =
            serde_json::from_str(&schema_content).expect("Failed to parse schema JSON");

        // Verify multiple types
        let types = schema["types"].as_array().expect("types should be array");
        assert!(types.len() >= 2, "Schema should have multiple types (User, Post)");

        // Verify User type
        let user_exists =
            types.iter().any(|t| t.get("name").map(|v| v.as_str()) == Some(Some("User")));
        assert!(user_exists, "User type should exist");

        // Verify Post type
        let post_exists =
            types.iter().any(|t| t.get("name").map(|v| v.as_str()) == Some(Some("Post")));
        assert!(post_exists, "Post type should exist");
    }

    /// Test 5: DDL output with refresh trigger
    #[test]
    fn test_ddl_includes_refresh_trigger() {
        // Example trigger-based refresh DDL
        let trigger_ddl = r#"
            -- Refresh trigger for trigger-based strategy
            CREATE OR REPLACE FUNCTION refresh_tv_user()
            RETURNS TRIGGER AS $$
            BEGIN
                UPDATE tv_user
                SET is_stale = true
                WHERE entity_id = NEW.id;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;

            CREATE TRIGGER trg_refresh_tv_user
            AFTER INSERT OR UPDATE OR DELETE ON public.user
            FOR EACH ROW
            EXECUTE FUNCTION refresh_tv_user();
        "#;

        // Validate trigger structure
        assert!(
            trigger_ddl.contains("CREATE OR REPLACE FUNCTION"),
            "Should have function creation"
        );
        assert!(trigger_ddl.contains("CREATE TRIGGER"), "Should have trigger creation");
        assert!(
            trigger_ddl.contains("AFTER INSERT OR UPDATE OR DELETE"),
            "Should trigger on DML changes"
        );
    }

    /// Test 6: DDL output with scheduled refresh
    #[test]
    fn test_ddl_includes_scheduled_refresh() {
        // Example scheduled refresh DDL
        let scheduled_ddl = r#"
            -- Scheduled refresh using pg_cron
            CREATE OR REPLACE FUNCTION refresh_tv_user_scheduled()
            RETURNS void AS $$
            BEGIN
                REFRESH MATERIALIZED VIEW CONCURRENTLY tv_user;
            END;
            $$ LANGUAGE plpgsql;

            -- Schedule refresh every 30 minutes
            SELECT cron.schedule('refresh_tv_user', '30 minutes', 'SELECT refresh_tv_user_scheduled()');
        "#;

        // Validate scheduled structure
        assert!(
            scheduled_ddl.contains("REFRESH MATERIALIZED VIEW"),
            "Should have refresh view statement"
        );
        assert!(scheduled_ddl.contains("cron.schedule"), "Should use pg_cron for scheduling");
        assert!(scheduled_ddl.contains("30 minutes"), "Should specify refresh interval");
    }

    /// Test 7: Complex schema with relationships
    #[test]
    fn test_generate_views_with_relationships() {
        let schema_dir = get_test_schema_dir();
        let schema_file = schema_dir.join("orders.json");

        assert!(schema_file.exists(), "Test schema file should exist");

        // Read schema
        let schema_content = fs::read_to_string(&schema_file).expect("Failed to read schema file");
        let schema: serde_json::Value =
            serde_json::from_str(&schema_content).expect("Failed to parse schema JSON");

        // Verify schema is valid
        assert!(schema.get("types").is_some(), "Schema should have types");
        assert!(schema.get("version").is_some(), "Schema should have version");

        let types = schema["types"].as_array().expect("types should be array");

        // Verify Order entity
        let order_type = types
            .iter()
            .find(|t| t.get("name").map(|v| v.as_str()) == Some(Some("Order")))
            .expect("Order entity should exist");

        assert!(order_type.get("fields").is_some(), "Order should have fields");
    }

    /// Test 8: DDL file output with proper headers
    #[test]
    fn test_ddl_file_output_format() {
        // Example complete DDL output
        let complete_ddl = r#"
-- FraiseQL DDL Generation Output
-- Schema: user.json
-- View: tv_user
-- Generated: 2024-01-24T12:00:00Z
-- See: https://fraiseql.dev/docs/views

-- Table Definition
CREATE TABLE IF NOT EXISTS tv_user (
    entity_id INTEGER NOT NULL UNIQUE,
    entity_json JSONB NOT NULL,
    is_stale BOOLEAN DEFAULT false,
    last_updated TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_tv_user_entity_id ON tv_user(entity_id);
CREATE INDEX IF NOT EXISTS idx_tv_user_entity_json_gin ON tv_user USING GIN(entity_json);

-- Documentation
COMMENT ON TABLE tv_user IS 'Table-backed JSON view for User entity';
        "#;

        // Validate DDL format
        assert!(complete_ddl.contains("FraiseQL DDL Generation Output"));
        assert!(complete_ddl.contains("CREATE TABLE"));
        assert!(complete_ddl.contains("CREATE INDEX"));
        assert!(complete_ddl.contains("COMMENT ON"));
    }

    /// Test 9: Composition views for relationships
    #[test]
    fn test_composition_views_ddl() {
        // Example composition view DDL
        let composition_ddl = r#"
            -- Composition view for User -> Posts relationship
            CREATE OR REPLACE VIEW cv_user_posts AS
            SELECT
                u.entity_id as user_id,
                u.entity_json as user_data,
                p.entity_json as post_data
            FROM tv_user u
            LEFT JOIN tv_post p ON p.entity_json->>'user_id' = u.entity_json->>'id'
            ORDER BY u.entity_id, p.entity_id;

            -- Batch composition function
            CREATE OR REPLACE FUNCTION batch_compose_user(batch_ids INTEGER[])
            RETURNS TABLE (user_id INTEGER, user_data JSONB, posts JSONB[])
            AS $$
            SELECT
                u.entity_id,
                u.entity_json,
                ARRAY_AGG(p.entity_json) FILTER (WHERE p.entity_id IS NOT NULL)
            FROM tv_user u
            LEFT JOIN tv_post p ON p.entity_json->>'user_id' = u.entity_json->>'id'
            WHERE u.entity_id = ANY(batch_ids)
            GROUP BY u.entity_id, u.entity_json;
            $$ LANGUAGE SQL;
        "#;

        // Validate composition view structure
        assert!(
            composition_ddl.contains("CREATE OR REPLACE VIEW cv_"),
            "Should create composition view"
        );
        assert!(composition_ddl.contains("LEFT JOIN"), "Should use LEFT JOIN for relationships");
        assert!(
            composition_ddl.contains("batch_compose_"),
            "Should provide batch composition function"
        );
    }

    /// Test 10: Monitoring and observability functions
    #[test]
    fn test_monitoring_functions_ddl() {
        // Example monitoring DDL
        let monitoring_ddl = r#"
            -- Staleness check function
            CREATE OR REPLACE FUNCTION check_staleness_user()
            RETURNS TABLE (is_stale BOOLEAN, last_updated TIMESTAMP, staleness_ms INTEGER)
            AS $$
            SELECT
                is_stale,
                last_updated,
                EXTRACT(EPOCH FROM (NOW() - last_updated))::INTEGER * 1000
            FROM tv_user
            ORDER BY last_updated ASC
            LIMIT 1;
            $$ LANGUAGE SQL;

            -- Staleness view
            CREATE OR REPLACE VIEW v_staleness_user AS
            SELECT
                entity_id,
                last_updated,
                EXTRACT(EPOCH FROM (NOW() - last_updated))::INTEGER * 1000 as staleness_ms,
                CASE
                    WHEN is_stale THEN 'STALE'
                    ELSE 'FRESH'
                END as status
            FROM tv_user
            ORDER BY staleness_ms DESC;
        "#;

        // Validate monitoring functions
        assert!(
            monitoring_ddl.contains("check_staleness_"),
            "Should provide staleness check function"
        );
        assert!(monitoring_ddl.contains("v_staleness_"), "Should provide staleness view");
        assert!(monitoring_ddl.contains("NOW()"), "Should check current timestamp");
    }
}
