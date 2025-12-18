//! Test fixtures and sample data
//!
//! Provides pre-built test data for consistent testing across phases

use serde_json::{json, Value};

/// Sample table schema for testing
pub struct SampleSchema;

impl SampleSchema {
    /// Create users table for testing
    pub fn users_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            email VARCHAR(255) UNIQUE NOT NULL,
            age INT,
            is_active BOOLEAN DEFAULT true,
            metadata JSONB DEFAULT '{}',
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }

    /// Create posts table for testing
    pub fn posts_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS posts (
            id SERIAL PRIMARY KEY,
            user_id INT REFERENCES users(id),
            title VARCHAR(255) NOT NULL,
            content TEXT,
            tags JSONB DEFAULT '[]',
            published BOOLEAN DEFAULT false,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }

    /// Create products table for testing (with complex JSONB)
    pub fn products_table_sql() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS products (
            id SERIAL PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            price DECIMAL(10, 2),
            attributes JSONB DEFAULT '{}',
            inventory JSONB DEFAULT '{"stock": 0}',
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#
    }
}

/// Sample data for testing
pub struct SampleData;

impl SampleData {
    /// Insert sample users
    pub fn insert_users_sql() -> &'static str {
        r#"
        INSERT INTO users (name, email, age, metadata)
        VALUES
            ('Alice', 'alice@example.com', 30, '{"role": "admin"}'),
            ('Bob', 'bob@example.com', 25, '{"role": "user"}'),
            ('Charlie', 'charlie@example.com', 35, '{"role": "user", "verified": true}')
        ON CONFLICT DO NOTHING;
        "#
    }

    /// Insert sample posts
    pub fn insert_posts_sql() -> &'static str {
        r#"
        INSERT INTO posts (user_id, title, content, tags, published)
        VALUES
            (1, 'First Post', 'Hello World', '["rust", "postgres"]', true),
            (1, 'Second Post', 'Async Rust', '["async", "rust"]', true),
            (2, 'Draft Post', 'Work in progress', '["draft"]', false)
        ON CONFLICT DO NOTHING;
        "#
    }

    /// Insert sample products
    pub fn insert_products_sql() -> &'static str {
        r#"
        INSERT INTO products (name, price, attributes, inventory)
        VALUES
            ('Laptop', 999.99, '{"brand": "Dell", "specs": {"cpu": "i7", "ram": "16GB"}}', '{"stock": 5, "warehouse": "A"}'),
            ('Mouse', 29.99, '{"brand": "Logitech", "color": "black"}', '{"stock": 50, "warehouse": "B"}'),
            ('Keyboard', 79.99, '{"brand": "Mechanical", "switches": "Blue"}', '{"stock": 0, "warehouse": "C"}')
        ON CONFLICT DO NOTHING;
        "#
    }
}

/// JSON value builders for WHERE clause testing
pub struct JsonTestValues;

impl JsonTestValues {
    pub fn simple_object() -> Value {
        json!({"key": "value", "number": 42})
    }

    pub fn nested_object() -> Value {
        json!({
            "user": {
                "name": "Alice",
                "contact": {
                    "email": "alice@example.com",
                    "phone": "+1-555-0123"
                }
            }
        })
    }

    pub fn array_value() -> Value {
        json!(["item1", "item2", "item3"])
    }

    pub fn mixed_types() -> Value {
        json!({
            "string": "text",
            "number": 123,
            "boolean": true,
            "null": null,
            "array": [1, 2, 3],
            "object": {"nested": "value"}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_schema_valid() {
        let sql = SampleSchema::users_table_sql();
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("users"));
    }

    #[test]
    fn test_json_test_values() {
        let obj = JsonTestValues::simple_object();
        assert!(obj.get("key").is_some());
        assert_eq!(obj.get("number").and_then(|v| v.as_i64()), Some(42));
    }
}