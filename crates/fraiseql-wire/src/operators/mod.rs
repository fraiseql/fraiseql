//! Operator-based SQL generation system
//!
//! This module provides type-safe operator abstractions for building WHERE clauses,
//! ORDER BY clauses, and query modifiers (LIMIT/OFFSET) without raw SQL strings.
//!
//! # Design Philosophy
//!
//! fraiseql-wire maintains backward compatibility with the existing string-based API
//! while offering operator abstractions for type safety and auditability:
//!
//! ```ignore
//! // Old style (still works)
//! client.query("users")
//!     .where_sql("data->>'name' = 'John'")
//!     .execute()
//!     .await?;
//!
//! // New style (type-safe)
//! client.query("users")
//!     .where_operator(WhereOperator::Eq(
//!         Field::JsonbField("name".to_string()),
//!         Value::String("John".to_string()),
//!     ))
//!     .execute()
//!     .await?;
//! ```
//!
//! # Operator Coverage
//!
//! - **Comparison**: Eq, Neq, Gt, Gte, Lt, Lte
//! - **Array**: In, Nin, Contains, ArrayContains, ArrayContainedBy, ArrayOverlaps
//! - **Array Length**: LenEq, LenGt, LenGte, LenLt, LenLte
//! - **String**: Contains, Icontains, Startswith, Endswith, Like, Ilike
//! - **Null**: IsNull
//! - **Vector Distance**: L2Distance, CosineDistance, InnerProduct, JaccardDistance
//! - **Full-Text Search**: Matches, PlainQuery, PhraseQuery, WebsearchQuery
//! - **Network**: IsIPv4, IsIPv6, IsPrivate, IsLoopback, InSubnet, ContainsSubnet, ContainsIP, IPRangeOverlap

pub mod field;
pub mod order_by;
pub mod sql_gen;
pub mod where_operator;

pub use field::{Field, Value};
pub use order_by::{Collation, FieldSource, NullsHandling, OrderByClause, SortOrder};
pub use sql_gen::generate_where_operator_sql;
pub use where_operator::WhereOperator;
