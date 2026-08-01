//! Operator-based SQL generation system
//!
//! This module provides type-safe operator abstractions for building WHERE clauses,
//! ORDER BY clauses, and query modifiers (LIMIT/OFFSET) without raw SQL strings.
//!
//! # ⚠️ The `WhereOperator` generator is not usable end to end (#877)
//!
//! [`generate_where_operator_sql`] emits `$N` placeholders, but this crate
//! speaks only the Postgres **simple query** protocol — there is no
//! Parse/Bind/Execute encoder, `QueryBuilder` has no method that accepts an
//! operator or a parameter map, and a `$N` placeholder sent through
//! `where_sql` fails at the server with `there is no parameter $1`. The
//! function is deprecated until the crate either implements the extended
//! query protocol or renders operator values as safely quoted literals; use
//! [`QueryBuilder::where_sql`](crate::client::QueryBuilder::where_sql) with an
//! inline predicate instead. [`OrderByClause`] and the [`Field`] helpers are
//! unaffected — they render plain SQL fragments.
//!
//! # Operator Coverage
//!
//! - **Comparison**: Eq, Neq, Gt, Gte, Lt, Lte
//! - **Array**: In, Nin, Contains, `ArrayContains`, `ArrayContainedBy`, `ArrayOverlaps`
//! - **Array Length**: `LenEq`, `LenGt`, `LenGte`, `LenLt`, `LenLte`
//! - **String**: Contains, Icontains, Startswith, Endswith, Like, Ilike
//! - **Null**: `IsNull`
//! - **Vector Distance**: `L2Distance`, `CosineDistance`, `InnerProduct`, `JaccardDistance`
//! - **Full-Text Search**: Matches, `PlainQuery`, `PhraseQuery`, `WebsearchQuery`
//! - **Network**: `IsIPv4`, `IsIPv6`, `IsPrivate`, `IsLoopback`, `IsMulticast`, `IsLinkLocal`, `IsDocumentation`, `IsCarrierGrade`, `InSubnet`, `ContainsSubnet`, `ContainsIP`, `IPRangeOverlap`

pub mod field;
pub mod order_by;
pub mod sql_gen;
pub mod where_operator;

pub use field::{Field, Value};
pub use order_by::{Collation, FieldSource, NullsHandling, OrderByClause, SortOrder};
#[allow(deprecated)]
// Reason: re-export keeps the deprecated path importable; the deprecation fires at use sites
pub use sql_gen::generate_where_operator_sql;
pub use where_operator::WhereOperator;
