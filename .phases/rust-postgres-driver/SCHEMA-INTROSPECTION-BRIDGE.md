# Schema Introspection Bridge - Python ↔ Rust Communication

**Document**: Schema registry communication patterns
**Created**: 2025-12-18
**Critical**: YES - Handles type system boundary
**Part of**: Phase 1 (Foundation)

---

## Overview

The schema introspection bridge connects FraiseQL's Python schema registry with Rust's type system, enabling dynamic query building with full type information.

```
Python (schema registry)
    ↓ (Pydantic models)
PyO3 conversion layer
    ↓ (Type conversion)
Rust (type-safe queries)
    ↓ (Query execution)
PostgreSQL
```

---

## Current Architecture

### Python Side (src/fraiseql/db.py)

```python
from pydantic import BaseModel
from typing import Dict, List, Optional

class ColumnDefinition(BaseModel):
    name: str
    pg_type: str  # PostgreSQL type name
    nullable: bool
    is_json: bool

class TableSchema(BaseModel):
    name: str
    columns: Dict[str, ColumnDefinition]
    primary_key: Optional[str]
```

---

### Required Rust Side (fraiseql_rs/src/schema/mod.rs) - NEW

```rust
//! Schema registry bridge between Python and Rust
//!
//! Converts Python Pydantic models to Rust type information

use pyo3::prelude::*;
use std::collections::HashMap;

/// Rust representation of a column definition
#[derive(Clone, Debug)]
pub struct ColumnDefinition {
    pub name: String,
    pub pg_type: String,
    pub nullable: bool,
    pub is_json: bool,
}

impl ColumnDefinition {
    /// Convert from Python dict
    pub fn from_python(py_dict: &PyDict) -> PyResult<Self> {
        Ok(ColumnDefinition {
            name: py_dict.get_item("name")?.extract()?,
            pg_type: py_dict.get_item("pg_type")?.extract()?,
            nullable: py_dict.get_item("nullable")?.extract()?,
            is_json: py_dict.get_item("is_json")?.extract()?,
        })
    }

    /// Convert to Python dict
    pub fn to_python(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("name", &self.name)?;
        dict.set_item("pg_type", &self.pg_type)?;
        dict.set_item("nullable", self.nullable)?;
        dict.set_item("is_json", self.is_json)?;
        Ok(dict.into())
    }
}

/// Rust representation of table schema
#[derive(Clone, Debug)]
pub struct TableSchema {
    pub name: String,
    pub columns: HashMap<String, ColumnDefinition>,
    pub primary_key: Option<String>,
}

impl TableSchema {
    /// Convert from Python dict
    pub fn from_python(py_dict: &PyDict) -> PyResult<Self> {
        let columns_py = py_dict.get_item("columns")?;
        let mut columns = HashMap::new();

        if let Ok(py_dict_cols) = columns_py.downcast::<PyDict>() {
            for (key, value) in py_dict_cols.iter() {
                let key_str: String = key.extract()?;
                let col_def = ColumnDefinition::from_python(
                    value.downcast::<PyDict>()?
                )?;
                columns.insert(key_str, col_def);
            }
        }

        Ok(TableSchema {
            name: py_dict.get_item("name")?.extract()?,
            columns,
            primary_key: py_dict.get_item("primary_key")?.extract().ok(),
        })
    }

    /// Convert to Python dict
    pub fn to_python(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);

        // Convert columns
        let cols_dict = PyDict::new(py);
        for (name, col) in &self.columns {
            cols_dict.set_item(name, col.to_python(py)?)?;
        }

        dict.set_item("name", &self.name)?;
        dict.set_item("columns", cols_dict)?;
        if let Some(pk) = &self.primary_key {
            dict.set_item("primary_key", pk)?;
        } else {
            dict.set_item("primary_key", py.None())?;
        }

        Ok(dict.into())
    }

    /// Get column by name
    pub fn get_column(&self, name: &str) -> Option<&ColumnDefinition> {
        self.columns.get(name)
    }

    /// Check if column is nullable
    pub fn is_nullable(&self, column_name: &str) -> bool {
        self.columns
            .get(column_name)
            .map(|col| col.nullable)
            .unwrap_or(false)
    }

    /// Check if column is JSONB
    pub fn is_json_column(&self, column_name: &str) -> bool {
        self.columns
            .get(column_name)
            .map(|col| col.is_json)
            .unwrap_or(false)
    }
}

/// Schema registry for all tables
#[derive(Clone, Debug)]
pub struct SchemaRegistry {
    schemas: HashMap<String, TableSchema>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        SchemaRegistry {
            schemas: HashMap::new(),
        }
    }

    /// Register a table schema
    pub fn register(&mut self, schema: TableSchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Get schema by table name
    pub fn get_schema(&self, table_name: &str) -> Option<&TableSchema> {
        self.schemas.get(table_name)
    }

    /// Get mutable schema by table name
    pub fn get_schema_mut(&mut self, table_name: &str) -> Option<&mut TableSchema> {
        self.schemas.get_mut(table_name)
    }

    /// Convert from Python list of dicts
    pub fn from_python(py_list: &PyList) -> PyResult<Self> {
        let mut registry = SchemaRegistry::new();

        for item in py_list.iter() {
            let py_dict = item.downcast::<PyDict>()?;
            let schema = TableSchema::from_python(py_dict)?;
            registry.register(schema);
        }

        Ok(registry)
    }

    /// Convert to Python list of dicts
    pub fn to_python(&self, py: Python) -> PyResult<Py<PyList>> {
        let list = PyList::new(py, &[]);

        for (_name, schema) in &self.schemas {
            list.append(schema.to_python(py)?)?;
        }

        Ok(list.into())
    }

    /// Get all table names
    pub fn table_names(&self) -> Vec<String> {
        self.schemas.keys().cloned().collect()
    }
}
```

---

## Python-Rust Integration

### Python Function to Pass Schema

```python
# src/fraiseql/db.py

from _fraiseql_rs import register_schema

def initialize_database():
    """Initialize Rust database layer with schema information"""

    # Get current schema from registry
    schema_list = SchemaRegistry.get_all_schemas()  # Python

    # Pass to Rust
    register_schema(py.compile("""
        import _fraiseql_rs
        schemas = [
            {
                'name': 'users',
                'columns': {...},
                'primary_key': 'id'
            },
            ...
        ]
        _fraiseql_rs.register_schema(schemas)
    """))
```

### Rust Function to Register Schema

```rust
#[pyfunction]
fn register_schema(schema_list: &PyList) -> PyResult<()> {
    let registry = SchemaRegistry::from_python(schema_list)?;

    // Store in Arc<Mutex<SchemaRegistry>> accessible to queries
    // See Phase 1 for integration

    Ok(())
}
```

---

## Type Mapping

### PostgreSQL → Rust Type Conversion

```rust
pub enum PgType {
    // Numeric
    Int2,      // int2
    Int4,      // int4
    Int8,      // int8
    Float4,    // float4
    Float8,    // float8
    Numeric,   // numeric/decimal

    // String
    Text,      // text, varchar
    Varchar,   // varchar with length
    Char,      // char(n)

    // Binary
    Bytea,     // bytea

    // Date/Time
    Timestamp, // timestamp without time zone
    TimestampTz, // timestamp with time zone
    Date,      // date
    Time,      // time without time zone
    TimeTz,    // time with time zone

    // Boolean
    Bool,      // boolean

    // JSON
    Json,      // json
    Jsonb,     // jsonb (MOST CRITICAL)

    // UUID
    Uuid,      // uuid

    // Arrays
    Int4Array,
    TextArray,

    // Other
    Unknown,
}

impl PgType {
    /// Parse from PostgreSQL type name
    pub fn from_pg_type_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "int2" | "smallint" => PgType::Int2,
            "int4" | "integer" => PgType::Int4,
            "int8" | "bigint" => PgType::Int8,
            "float4" | "real" => PgType::Float4,
            "float8" | "double precision" => PgType::Float8,
            "numeric" | "decimal" => PgType::Numeric,

            "text" | "varchar" => PgType::Text,
            "char" => PgType::Char,

            "bytea" => PgType::Bytea,

            "timestamp" => PgType::Timestamp,
            "timestamp with time zone" => PgType::TimestampTz,
            "date" => PgType::Date,
            "time" => PgType::Time,
            "time with time zone" => PgType::TimeTz,

            "boolean" | "bool" => PgType::Bool,

            "json" => PgType::Json,
            "jsonb" => PgType::Jsonb,

            "uuid" => PgType::Uuid,

            "integer[]" | "int4[]" => PgType::Int4Array,
            "text[]" => PgType::TextArray,

            _ => PgType::Unknown,
        }
    }

    /// Get Rust type representation
    pub fn rust_type(&self) -> &'static str {
        match self {
            PgType::Int2 => "i16",
            PgType::Int4 => "i32",
            PgType::Int8 => "i64",
            PgType::Float4 => "f32",
            PgType::Float8 => "f64",
            PgType::Numeric => "BigDecimal",

            PgType::Text | PgType::Varchar | PgType::Char => "String",
            PgType::Bytea => "Vec<u8>",

            PgType::Timestamp | PgType::TimestampTz | PgType::Date => "SystemTime",
            PgType::Time | PgType::TimeTz => "Duration",

            PgType::Bool => "bool",

            PgType::Json | PgType::Jsonb => "serde_json::Value",
            PgType::Uuid => "uuid::Uuid",

            PgType::Int4Array => "Vec<i32>",
            PgType::TextArray => "Vec<String>",

            PgType::Unknown => "String",
        }
    }
}
```

---

## Usage in WHERE Clause Building

```rust
// Phase 2: WHERE clause builder uses schema info

pub struct WhereBuilder {
    schema: Arc<SchemaRegistry>,
}

impl WhereBuilder {
    pub fn build_where_clause(&self, table: &str, filter: &Filter) -> Result<String> {
        let schema = self.schema
            .get_schema(table)
            .ok_or("Table not found")?;

        // Use schema to type-check filter operators
        self.build_filter(filter, schema)
    }

    fn validate_filter_type(&self, column: &str, op: &str, value: &Value, schema: &TableSchema) -> Result<()> {
        let col_def = schema.get_column(column)
            .ok_or("Column not found")?;

        // Type checking happens here
        match col_def.pg_type.as_str() {
            "int4" | "int8" => {
                if !matches!(value, Value::Number(_)) {
                    return Err("Type mismatch: expected number".into());
                }
            }
            "text" => {
                if !matches!(value, Value::String(_)) {
                    return Err("Type mismatch: expected string".into());
                }
            }
            "jsonb" => {
                // JSONB can be queried flexibly
            }
            _ => {}
        }

        Ok(())
    }
}
```

---

## Error Handling

```rust
#[derive(Debug)]
pub enum SchemaError {
    TableNotFound(String),
    ColumnNotFound { table: String, column: String },
    TypeMismatch { column: String, expected: String, got: String },
    RegistryNotInitialized,
}

impl From<SchemaError> for PyErr {
    fn from(err: SchemaError) -> PyErr {
        match err {
            SchemaError::TableNotFound(table) => {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Table not found: {}", table)
                )
            }
            SchemaError::ColumnNotFound { table, column } => {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("Column not found: {}.{}", table, column)
                )
            }
            SchemaError::TypeMismatch { column, expected, got } => {
                PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    format!("Type mismatch for column {}: expected {}, got {}",
                        column, expected, got)
                )
            }
            SchemaError::RegistryNotInitialized => {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Schema registry not initialized"
                )
            }
        }
    }
}
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_definition_conversion() {
        let py = pyo3::Python::acquire_gil();
        let py_dict = PyDict::new(py.python());
        py_dict.set_item("name", "user_id").unwrap();
        py_dict.set_item("pg_type", "int4").unwrap();
        py_dict.set_item("nullable", false).unwrap();
        py_dict.set_item("is_json", false).unwrap();

        let col = ColumnDefinition::from_python(&py_dict).unwrap();
        assert_eq!(col.name, "user_id");
        assert_eq!(col.pg_type, "int4");
    }

    #[test]
    fn test_pg_type_parsing() {
        assert_eq!(PgType::from_pg_type_name("int4"), PgType::Int4);
        assert_eq!(PgType::from_pg_type_name("jsonb"), PgType::Jsonb);
        assert_eq!(PgType::from_pg_type_name("text"), PgType::Text);
    }
}
```

---

## Integration with Phase 1

1. **Foundation (Phase 1)**:
   - Register schema during pool initialization
   - Store in Arc<Mutex<SchemaRegistry>>
   - Make available to all queries

2. **Query Execution (Phase 2)**:
   - Pass schema to WHERE builder
   - Type-check filter operators
   - Generate type-safe SQL

3. **Streaming (Phase 3)**:
   - Use schema for result type conversion
   - Ensure camelCase transformation respects types

---

## Next Steps

1. Implement schema module in Phase 1
2. Test schema registration in integration tests
3. Verify type checking in Phase 2 WHERE builder
4. Reference in all query execution code

---

**Last Updated**: 2025-12-18
