//! What a transport asks to have embedded into the rows of a direct read.
//!
//! The REST `?select=posts(id,author(name)),posts.count` in engine terms: a tree of
//! relationships to follow from the read's return type, the fields to take at each
//! level, and the counts to take beside them. Nothing here says *how* a level is read —
//! that is [`Executor::execute_query_composed`](crate::runtime::Executor::execute_query_composed)'s
//! decision, and it composes every level into the parent's statement.

/// A relationship to embed into each row of the level above it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmbedSelection {
    /// The relationship's name on the parent type.
    pub relationship: String,
    /// The key the embedded value is written under — the relationship's name, or the
    /// client's rename.
    pub output_key:   String,
    /// The target type's fields to return, in the order they were asked for.
    pub fields:       Vec<String>,
    /// The client's filter on the related rows, in the `{field: {op: value}}` shape a
    /// `where` argument takes. Composed with the target's own security predicate the
    /// way a filter on a flat read of the target is, so it can only narrow.
    pub filter:       Option<serde_json::Value>,
    /// The page each parent row gets of related rows. Capped by `max_page_size`, as any
    /// read's page is.
    pub limit:        Option<u32>,
    /// Relationships to embed into each related row.
    pub embeds:       Vec<EmbedSelection>,
    /// Relationships to count for each related row.
    pub counts:       Vec<CountSelection>,
}

/// A relationship whose related rows are counted, for each row of the level above it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CountSelection {
    /// The relationship's name on the parent type.
    pub relationship: String,
    /// The key the count is written under.
    pub output_key:   String,
    /// The client's filter on the counted rows — the same filter an embed of the same
    /// relationship reads, so a list and its total cannot disagree (#1285).
    pub filter:       Option<serde_json::Value>,
}
