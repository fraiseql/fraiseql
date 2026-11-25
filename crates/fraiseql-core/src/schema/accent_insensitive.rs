//! `[accent_insensitive]`: the filters that compare accent- and case-insensitively.

use fraiseql_db::where_clause::UnaccentFunction;
use serde::{Deserialize, Serialize};

/// `[accent_insensitive]`, as compiled and as loaded.
///
/// Declaring it publishes the `ai*` string operators (`aicontains`, `aistartswith`,
/// `aiendswith`, `aieq`, `ailike`, `nailike`, `airegex`, `nairegex`) on every string
/// filter, each folding both of its sides through `function`. Without it, the operators
/// are neither published nor accepted. The server never creates the function:
/// `fraiseql compile --emit-ddl` writes it, and `fraiseql compile --database` checks it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccentInsensitiveConfig {
    /// The `IMMUTABLE` `text → text` function both sides are folded through, schema
    /// qualified and lower case. Default `fraiseql.unaccent`.
    #[serde(default)]
    #[schemars(with = "String")]
    pub function: UnaccentFunction,
}
