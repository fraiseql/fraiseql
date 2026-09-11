//! Index advice for the column a page is tie-broken on (#1307).
//!
//! Since #1287 a client sort over a non-unique key is tie-broken by the entity
//! identity, which is what makes a page a slice of a sequence rather than of an
//! unordered set. The order is correct either way; what it costs depends on the
//! indexes, and only the schema author can create those. Measured on a 200k-row
//! table, `ORDER BY status, pk_invoice`:
//!
//! | indexes | plan |
//! |---|---|
//! | `(status)` and `(pk_invoice)` separately | `Incremental Sort`, presorted key `status` |
//! | `(status, pk_invoice)` | `Index Only Scan`, no sort |
//!
//! This module is deliberately pure: it takes a query, the base relation, and the
//! relation's indexes, and returns what is missing. The catalog reads live in
//! `fraiseql_db`, so the rule that decides what counts as advice is testable
//! without a database — which matters because the required test leg is `--lib`.

use fraiseql_core::schema::{PaginationOrder, QueryDefinition};
use fraiseql_db::postgres::IndexInfo;

/// What an author is missing, for one query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Advice {
    /// Nothing leads with the tie-break key, so **every** page of this query
    /// sorts the whole relation — not an incremental sort, a full one. The
    /// larger of the two costs, and the more common.
    TieBreakUnindexed {
        /// The key a page is ordered by.
        tie_break: String,
    },
    /// A sortable column is indexed, but the tie-break does not follow it in that
    /// index, so a client sorting on it gets an incremental sort on every page.
    MissingComposite {
        /// The sortable columns whose index does not carry the tie-break.
        sort_keys: Vec<String>,
        /// The key that must follow each of them.
        tie_break: String,
    },
}

/// One query's findings, with the DDL that resolves them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryAdvice {
    /// The GraphQL query field.
    pub query:    String,
    /// The relation the indexes belong to — the view's base table, not the view.
    pub relation: String,
    /// What is missing.
    pub advice:   Advice,
    /// `CREATE INDEX` statements, in the order they should be applied.
    pub ddl:      Vec<String>,
}

/// The key a page of `query` is ordered by, as SQL.
///
/// `None` means the query declared no page order — the escape hatch for a view
/// carrying its own `ORDER BY` — and a query that orders nothing has no index to
/// be missing.
#[must_use]
pub fn tie_break_key(query: &QueryDefinition) -> Option<String> {
    match query.pagination_order.as_ref()? {
        PaginationOrder::Column(column) => Some(column.clone()),
        PaginationOrder::JsonIdentity => Some(format!("({} ->> 'id')", query.jsonb_column)),
    }
}

/// Compare two index keys as Postgres renders them.
///
/// `pg_get_indexdef` prints an expression key as `(data ->> 'id'::text)` while the
/// compiled schema knows it as `data->>'id'`. Both reduce to the same token once
/// whitespace and the redundant `::text` cast are removed; matching on the raw
/// strings would report every expression index as absent and advise creating one
/// that already exists.
fn keys_match(a: &str, b: &str) -> bool {
    fn normalize(key: &str) -> String {
        key.to_lowercase()
            .replace("::text", "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }
    normalize(a) == normalize(b)
}

/// Whether any index has `key` as its **first** key.
///
/// Leading position is what matters: an index on `(status, pk_invoice)` orders by
/// `pk_invoice` only within a `status`, so it does not serve `ORDER BY pk_invoice`.
fn leads_with(indexes: &[IndexInfo], key: &str) -> bool {
    indexes.iter().any(|ix| ix.keys.first().is_some_and(|k| keys_match(k, key)))
}

/// Whether any index opens with `first` then `second`.
fn leads_with_pair(indexes: &[IndexInfo], first: &str, second: &str) -> bool {
    indexes.iter().any(|ix| {
        ix.keys.len() >= 2 && keys_match(&ix.keys[0], first) && keys_match(&ix.keys[1], second)
    })
}

/// A key as it must appear inside `CREATE INDEX (...)`.
///
/// A bare column goes in as-is; an expression needs its own parentheses, which
/// `tie_break_key` already supplies.
fn as_ddl_key(key: &str) -> String {
    if key.contains("->>") {
        format!("({key})")
    } else {
        key.to_string()
    }
}

/// What `query` is missing on `relation`, given that relation's indexes.
///
/// Returns at most two findings: the tie-break being unindexed outright, and the
/// sortable columns whose index does not carry it. They are reported separately
/// because the remedies differ and the first is strictly the larger cost.
///
/// The sortable set is the query's `native_columns` — a client can only sort on a
/// field that maps to a real column, and only a real column can lead a plain
/// index. It is consulted only when the query exposes `orderBy`; without it no
/// client ordering exists to be tie-broken.
#[must_use]
pub fn advise(query: &QueryDefinition, relation: &str, indexes: &[IndexInfo]) -> Vec<QueryAdvice> {
    let Some(tie_break) = tie_break_key(query) else {
        return Vec::new();
    };

    let mut findings = Vec::new();

    if !leads_with(indexes, &tie_break) {
        findings.push(QueryAdvice {
            query:    query.name.clone(),
            relation: relation.to_string(),
            advice:   Advice::TieBreakUnindexed {
                tie_break: tie_break.clone(),
            },
            ddl:      vec![format!(
                "CREATE INDEX ON {relation} ({});",
                as_ddl_key(&tie_break)
            )],
        });
    }

    if query.auto_params.has_order_by {
        let mut sort_keys: Vec<String> = query
            .native_columns
            .values()
            .filter(|column| !keys_match(column, &tie_break))
            .filter(|column| leads_with(indexes, column))
            .filter(|column| !leads_with_pair(indexes, column, &tie_break))
            .cloned()
            .collect();
        // `native_columns` is a HashMap, so its iteration order is not stable and
        // an unsorted report would differ between runs of the same schema.
        sort_keys.sort_unstable();
        sort_keys.dedup();

        if !sort_keys.is_empty() {
            let ddl = sort_keys
                .iter()
                .map(|k| format!("CREATE INDEX ON {relation} ({k}, {});", as_ddl_key(&tie_break)))
                .collect();
            findings.push(QueryAdvice {
                query: query.name.clone(),
                relation: relation.to_string(),
                advice: Advice::MissingComposite {
                    sort_keys,
                    tie_break,
                },
                ddl,
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests;
