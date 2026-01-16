//! Vector query builder for pgvector similarity search (Phase 11.2).
//!
//! This module provides SQL query generation for pgvector operations including:
//! - Similarity search with configurable distance metrics
//! - Vector insert and upsert operations
//! - Proper parameter binding for vector data
//!
//! # Example
//!
//! ```
//! use fraiseql_core::utils::vector::{VectorQueryBuilder, VectorSearchQuery};
//! use fraiseql_core::schema::DistanceMetric;
//!
//! let builder = VectorQueryBuilder::new();
//! let query = VectorSearchQuery {
//!     table: "documents".to_string(),
//!     embedding_column: "embedding".to_string(),
//!     select_columns: vec!["id".to_string(), "content".to_string()],
//!     distance_metric: DistanceMetric::Cosine,
//!     limit: 10,
//!     where_clause: None,
//!     order_by: None,
//!     include_distance: false,
//!     offset: None,
//! };
//!
//! let (sql, _params) = builder.similarity_search(&query, &[0.1, 0.2, 0.3]);
//! assert!(sql.contains("documents"));
//! ```

use crate::schema::{DistanceMetric, VectorConfig};
use serde::{Deserialize, Serialize};

/// A SQL parameter value for vector queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorParam {
    /// A vector embedding (array of floats).
    Vector(Vec<f32>),
    /// An integer value.
    Int(i64),
    /// A string value.
    String(String),
    /// A JSON value.
    Json(serde_json::Value),
}

impl VectorParam {
    /// Convert to SQL literal string for debugging.
    #[must_use]
    pub fn to_sql_literal(&self) -> String {
        match self {
            VectorParam::Vector(v) => {
                let values: Vec<String> = v.iter().map(std::string::ToString::to_string).collect();
                format!("'[{}]'::vector", values.join(","))
            }
            VectorParam::Int(i) => i.to_string(),
            VectorParam::String(s) => format!("'{}'", s.replace('\'', "''")),
            VectorParam::Json(j) => format!("'{j}'::jsonb"),
        }
    }
}

/// Configuration for a similarity search query.
#[derive(Debug, Clone)]
pub struct VectorSearchQuery {
    /// Table or view to query.
    pub table: String,
    /// Column containing the vector embedding.
    pub embedding_column: String,
    /// Columns to select (empty = all).
    pub select_columns: Vec<String>,
    /// Distance metric to use.
    pub distance_metric: DistanceMetric,
    /// Maximum number of results.
    pub limit: u32,
    /// Optional WHERE clause (without "WHERE" keyword).
    pub where_clause: Option<String>,
    /// Optional additional ORDER BY clause (applied after distance ordering).
    pub order_by: Option<String>,
    /// Whether to include the distance score in results.
    pub include_distance: bool,
    /// Optional offset for pagination.
    pub offset: Option<u32>,
}

impl Default for VectorSearchQuery {
    fn default() -> Self {
        Self {
            table: String::new(),
            embedding_column: "embedding".to_string(),
            select_columns: Vec::new(),
            distance_metric: DistanceMetric::Cosine,
            limit: 10,
            where_clause: None,
            order_by: None,
            include_distance: false,
            offset: None,
        }
    }
}

impl VectorSearchQuery {
    /// Create a new search query for a table.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            ..Default::default()
        }
    }

    /// Set the embedding column.
    #[must_use]
    pub fn with_embedding_column(mut self, column: impl Into<String>) -> Self {
        self.embedding_column = column.into();
        self
    }

    /// Set the columns to select.
    #[must_use]
    pub fn with_select_columns(mut self, columns: Vec<String>) -> Self {
        self.select_columns = columns;
        self
    }

    /// Set the distance metric.
    #[must_use]
    pub fn with_distance_metric(mut self, metric: DistanceMetric) -> Self {
        self.distance_metric = metric;
        self
    }

    /// Set the result limit.
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Set a WHERE clause filter.
    #[must_use]
    pub fn with_where(mut self, clause: impl Into<String>) -> Self {
        self.where_clause = Some(clause.into());
        self
    }

    /// Include distance score in results.
    #[must_use]
    pub fn with_distance_score(mut self) -> Self {
        self.include_distance = true;
        self
    }

    /// Set pagination offset.
    #[must_use]
    pub fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Configuration for a vector insert/upsert operation.
#[derive(Debug, Clone)]
pub struct VectorInsertQuery {
    /// Table to insert into.
    pub table: String,
    /// Columns to insert (in order).
    pub columns: Vec<String>,
    /// Name of the vector column.
    pub vector_column: String,
    /// Whether to upsert (ON CONFLICT DO UPDATE).
    pub upsert: bool,
    /// Conflict column(s) for upsert.
    pub conflict_columns: Vec<String>,
    /// Columns to update on conflict (empty = all non-conflict columns).
    pub update_columns: Vec<String>,
    /// Whether to return inserted IDs.
    pub returning: Option<String>,
}

impl Default for VectorInsertQuery {
    fn default() -> Self {
        Self {
            table: String::new(),
            columns: Vec::new(),
            vector_column: "embedding".to_string(),
            upsert: false,
            conflict_columns: vec!["id".to_string()],
            update_columns: Vec::new(),
            returning: Some("id".to_string()),
        }
    }
}

impl VectorInsertQuery {
    /// Create a new insert query.
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            ..Default::default()
        }
    }

    /// Set the columns to insert.
    #[must_use]
    pub fn with_columns(mut self, columns: Vec<String>) -> Self {
        self.columns = columns;
        self
    }

    /// Set the vector column name.
    #[must_use]
    pub fn with_vector_column(mut self, column: impl Into<String>) -> Self {
        self.vector_column = column.into();
        self
    }

    /// Enable upsert mode.
    #[must_use]
    pub fn with_upsert(mut self, conflict_columns: Vec<String>) -> Self {
        self.upsert = true;
        self.conflict_columns = conflict_columns;
        self
    }

    /// Set columns to update on conflict.
    #[must_use]
    pub fn with_update_columns(mut self, columns: Vec<String>) -> Self {
        self.update_columns = columns;
        self
    }

    /// Set the RETURNING clause.
    #[must_use]
    pub fn with_returning(mut self, column: impl Into<String>) -> Self {
        self.returning = Some(column.into());
        self
    }
}

/// Builder for pgvector SQL queries.
///
/// This struct generates SQL for vector similarity search and manipulation
/// operations using `PostgreSQL`'s `pgvector` extension.
#[derive(Debug, Clone, Default)]
pub struct VectorQueryBuilder {
    /// Parameter placeholder style ($1 vs ?).
    placeholder_style: PlaceholderStyle,
}

/// Style of parameter placeholders in generated SQL.
#[derive(Debug, Clone, Copy, Default)]
pub enum PlaceholderStyle {
    /// `PostgreSQL` style: `$1`, `$2`, `$3`
    #[default]
    Dollar,
    /// MySQL/SQLite style: `?`, `?`, `?`
    QuestionMark,
}

impl VectorQueryBuilder {
    /// Create a new vector query builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder with question mark placeholders.
    #[must_use]
    pub fn with_question_marks() -> Self {
        Self {
            placeholder_style: PlaceholderStyle::QuestionMark,
        }
    }

    /// Generate a parameter placeholder.
    fn placeholder(&self, index: usize) -> String {
        match self.placeholder_style {
            PlaceholderStyle::Dollar => format!("${index}"),
            PlaceholderStyle::QuestionMark => "?".to_string(),
        }
    }

    /// Build a similarity search query.
    ///
    /// Generates SQL like:
    /// ```sql
    /// SELECT id, content, (embedding <=> $1::vector) AS distance
    /// FROM documents
    /// WHERE metadata->>'type' = 'article'
    /// ORDER BY embedding <=> $1::vector
    /// LIMIT 10
    /// ```
    ///
    /// # Arguments
    /// * `query` - The search query configuration
    /// * `query_embedding` - The embedding vector to search for
    ///
    /// # Returns
    /// A tuple of (SQL string, parameter values)
    #[must_use]
    pub fn similarity_search(
        &self,
        query: &VectorSearchQuery,
        query_embedding: &[f32],
    ) -> (String, Vec<VectorParam>) {
        let mut params = Vec::new();
        let mut param_idx = 1;

        // Add the query embedding as the first parameter
        params.push(VectorParam::Vector(query_embedding.to_vec()));
        let embedding_placeholder = format!("{}::vector", self.placeholder(param_idx));
        param_idx += 1;

        let distance_op = query.distance_metric.operator();

        // Build SELECT clause
        let select_clause = if query.select_columns.is_empty() {
            if query.include_distance {
                format!(
                    "*, ({} {} {}) AS distance",
                    query.embedding_column, distance_op, embedding_placeholder
                )
            } else {
                "*".to_string()
            }
        } else {
            let cols = query.select_columns.join(", ");
            if query.include_distance {
                format!(
                    "{}, ({} {} {}) AS distance",
                    cols, query.embedding_column, distance_op, embedding_placeholder
                )
            } else {
                cols
            }
        };

        // Build WHERE clause
        let where_clause = if let Some(ref clause) = query.where_clause {
            format!("\nWHERE {clause}")
        } else {
            String::new()
        };

        // Build ORDER BY clause (always order by distance for similarity search)
        let order_clause = format!(
            "\nORDER BY {} {} {}",
            query.embedding_column, distance_op, embedding_placeholder
        );

        // Build LIMIT clause
        let limit_clause = format!("\nLIMIT {}", self.placeholder(param_idx));
        params.push(VectorParam::Int(i64::from(query.limit)));
        param_idx += 1;

        // Build OFFSET clause
        let offset_clause = if let Some(offset) = query.offset {
            let clause = format!("\nOFFSET {}", self.placeholder(param_idx));
            params.push(VectorParam::Int(i64::from(offset)));
            clause
        } else {
            String::new()
        };

        let sql = format!(
            "SELECT {}\nFROM {}{}{}{}{}",
            select_clause, query.table, where_clause, order_clause, limit_clause, offset_clause
        );

        (sql, params)
    }

    /// Build a single vector insert query.
    ///
    /// Generates SQL like:
    /// ```sql
    /// INSERT INTO documents (id, content, embedding)
    /// VALUES ($1, $2, $3::vector)
    /// RETURNING id
    /// ```
    #[must_use]
    pub fn insert_one(
        &self,
        query: &VectorInsertQuery,
        values: &[VectorParam],
    ) -> (String, Vec<VectorParam>) {
        let columns = query.columns.join(", ");

        let placeholders: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ph = self.placeholder(i + 1);
                if matches!(v, VectorParam::Vector(_)) {
                    format!("{ph}::vector")
                } else {
                    ph
                }
            })
            .collect();

        let values_clause = placeholders.join(", ");

        let returning_clause = if let Some(ref col) = query.returning {
            format!("\nRETURNING {col}")
        } else {
            String::new()
        };

        let sql = if query.upsert {
            let conflict_cols = query.conflict_columns.join(", ");

            // Determine which columns to update
            let update_cols: Vec<&String> = if query.update_columns.is_empty() {
                // Update all non-conflict columns
                query
                    .columns
                    .iter()
                    .filter(|c| !query.conflict_columns.contains(c))
                    .collect()
            } else {
                query.update_columns.iter().collect()
            };

            let update_clause: String = update_cols
                .iter()
                .map(|c| format!("{c} = EXCLUDED.{c}"))
                .collect::<Vec<_>>()
                .join(", ");

            format!(
                "INSERT INTO {} ({})\nVALUES ({})\nON CONFLICT ({}) DO UPDATE SET {}{}",
                query.table, columns, values_clause, conflict_cols, update_clause, returning_clause
            )
        } else {
            format!(
                "INSERT INTO {} ({})\nVALUES ({}){}",
                query.table, columns, values_clause, returning_clause
            )
        };

        (sql, values.to_vec())
    }

    /// Build a batch vector insert query.
    ///
    /// Generates SQL like:
    /// ```sql
    /// INSERT INTO documents (id, content, embedding)
    /// VALUES
    ///   ($1, $2, $3::vector),
    ///   ($4, $5, $6::vector),
    ///   ($7, $8, $9::vector)
    /// RETURNING id
    /// ```
    #[must_use]
    pub fn insert_batch(
        &self,
        query: &VectorInsertQuery,
        rows: &[Vec<VectorParam>],
    ) -> (String, Vec<VectorParam>) {
        if rows.is_empty() {
            return (String::new(), Vec::new());
        }

        let columns = query.columns.join(", ");
        let cols_per_row = query.columns.len();

        let mut all_params = Vec::new();
        let mut values_clauses = Vec::new();

        for (row_idx, row) in rows.iter().enumerate() {
            let base_idx = row_idx * cols_per_row + 1;
            let placeholders: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let ph = self.placeholder(base_idx + i);
                    if matches!(v, VectorParam::Vector(_)) {
                        format!("{ph}::vector")
                    } else {
                        ph
                    }
                })
                .collect();

            values_clauses.push(format!("({})", placeholders.join(", ")));
            all_params.extend(row.clone());
        }

        let returning_clause = if let Some(ref col) = query.returning {
            format!("\nRETURNING {col}")
        } else {
            String::new()
        };

        let sql = format!(
            "INSERT INTO {} ({})\nVALUES\n  {}{}",
            query.table,
            columns,
            values_clauses.join(",\n  "),
            returning_clause
        );

        (sql, all_params)
    }

    /// Build a query to create a vector index.
    ///
    /// Generates SQL like:
    /// ```sql
    /// CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)
    /// ```
    #[must_use]
    pub fn create_index(&self, config: &VectorConfig, table: &str, column: &str) -> Option<String> {
        config
            .index_type
            .index_sql(table, column, config.distance_metric)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_search_basic() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents")
            .with_embedding_column("embedding")
            .with_limit(10);

        let embedding = vec![0.1, 0.2, 0.3];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("SELECT *"));
        assert!(sql.contains("FROM documents"));
        assert!(sql.contains("ORDER BY embedding <=>"));
        assert!(sql.contains("$1::vector"));
        assert!(sql.contains("LIMIT $2"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_similarity_search_with_columns() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_select_columns(vec!["id".to_string(), "content".to_string()])
            .with_distance_score()
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("SELECT id, content,"));
        assert!(sql.contains("AS distance"));
        assert!(sql.contains("LIMIT $2"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_similarity_search_with_where() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents")
            .with_where("metadata->>'type' = 'article'")
            .with_limit(10);

        let embedding = vec![0.1, 0.2, 0.3];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("WHERE metadata->>'type' = 'article'"));
    }

    #[test]
    fn test_similarity_search_with_offset() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents")
            .with_limit(10)
            .with_offset(20);

        let embedding = vec![0.1, 0.2];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("OFFSET $3"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_similarity_search_l2_distance() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_distance_metric(DistanceMetric::L2)
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("<->"));
    }

    #[test]
    fn test_similarity_search_inner_product() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_distance_metric(DistanceMetric::InnerProduct)
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("<#>"));
    }

    #[test]
    fn test_insert_one_basic() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents").with_columns(vec![
            "id".to_string(),
            "content".to_string(),
            "embedding".to_string(),
        ]);

        let values = vec![
            VectorParam::String("doc1".to_string()),
            VectorParam::String("Hello world".to_string()),
            VectorParam::Vector(vec![0.1, 0.2, 0.3]),
        ];

        let (sql, params) = builder.insert_one(&query, &values);

        assert!(sql.contains("INSERT INTO documents (id, content, embedding)"));
        assert!(sql.contains("VALUES ($1, $2, $3::vector)"));
        assert!(sql.contains("RETURNING id"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_insert_upsert() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents")
            .with_columns(vec![
                "id".to_string(),
                "content".to_string(),
                "embedding".to_string(),
            ])
            .with_upsert(vec!["id".to_string()]);

        let values = vec![
            VectorParam::String("doc1".to_string()),
            VectorParam::String("Hello world".to_string()),
            VectorParam::Vector(vec![0.1, 0.2, 0.3]),
        ];

        let (sql, _) = builder.insert_one(&query, &values);

        assert!(sql.contains("ON CONFLICT (id) DO UPDATE SET"));
        assert!(sql.contains("content = EXCLUDED.content"));
        assert!(sql.contains("embedding = EXCLUDED.embedding"));
    }

    #[test]
    fn test_insert_batch() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents")
            .with_columns(vec!["id".to_string(), "embedding".to_string()]);

        let rows = vec![
            vec![
                VectorParam::String("doc1".to_string()),
                VectorParam::Vector(vec![0.1, 0.2]),
            ],
            vec![
                VectorParam::String("doc2".to_string()),
                VectorParam::Vector(vec![0.3, 0.4]),
            ],
        ];

        let (sql, params) = builder.insert_batch(&query, &rows);

        assert!(sql.contains("INSERT INTO documents (id, embedding)"));
        assert!(sql.contains("($1, $2::vector)"));
        assert!(sql.contains("($3, $4::vector)"));
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn test_insert_batch_empty() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents").with_columns(vec!["id".to_string()]);

        let (sql, params) = builder.insert_batch(&query, &[]);

        assert!(sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_create_index_hnsw() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::openai();

        let sql = builder.create_index(&config, "documents", "embedding");

        assert_eq!(
            sql,
            Some("CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)".to_string())
        );
    }

    #[test]
    fn test_create_index_ivfflat() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::new(1536)
            .with_index(crate::schema::VectorIndexType::IvfFlat)
            .with_distance(DistanceMetric::L2);

        let sql = builder.create_index(&config, "docs", "vec");

        assert_eq!(
            sql,
            Some("CREATE INDEX ON docs USING ivfflat (vec vector_l2_ops)".to_string())
        );
    }

    #[test]
    fn test_create_index_none() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::new(1536).with_index(crate::schema::VectorIndexType::None);

        let sql = builder.create_index(&config, "documents", "embedding");

        assert_eq!(sql, None);
    }

    #[test]
    fn test_vector_param_to_sql_literal() {
        let vec_param = VectorParam::Vector(vec![0.1, 0.2, 0.3]);
        assert_eq!(vec_param.to_sql_literal(), "'[0.1,0.2,0.3]'::vector");

        let int_param = VectorParam::Int(42);
        assert_eq!(int_param.to_sql_literal(), "42");

        let str_param = VectorParam::String("hello".to_string());
        assert_eq!(str_param.to_sql_literal(), "'hello'");

        let str_param_escape = VectorParam::String("it's a test".to_string());
        assert_eq!(str_param_escape.to_sql_literal(), "'it''s a test'");
    }

    #[test]
    fn test_question_mark_placeholders() {
        let builder = VectorQueryBuilder::with_question_marks();
        let query = VectorSearchQuery::new("docs").with_limit(10);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("?::vector"));
        assert!(!sql.contains("$1"));
    }
}
