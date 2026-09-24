//! The call shape of a read that composes embedded levels into one statement.
//!
//! A REST `?select=posts(id,comments(id))` used to be answered by a parent read and one
//! sub-read per parent row per level. [`ComposedLevel`] is that request as **one**
//! statement: each embedded level is a correlated subquery joined `LATERAL`, carrying its
//! own filter, ordering and page, so the database returns the whole tree in a single
//! round trip and under a single snapshot.
//!
//! Every value here is already resolved. Which rows a level may read (its security
//! predicate), which keys it may return, and in what order are decided by the engine
//! before this type is built; an adapter renders what it is given and decides nothing.
//!
//! # What a composed read returns
//!
//! Every row, at every level, is the object
//!
//! ```text
//! { "d": <the level's document>, "e": { <output_key>: <embedded value>, … } }
//! ```
//!
//! rather than the document with the embeds merged into it. A relationship may share its
//! name with a stored key, and the engine projects the document before it attaches the
//! embeds; keeping the two apart means neither can overwrite the other on the way.

use crate::{OrderByClause, ScalarFieldType, WhereClause};

/// The key under which a composed row carries its level's document.
pub const COMPOSED_DOCUMENT_KEY: &str = "d";

/// The key under which a composed row carries its embedded values.
pub const COMPOSED_EMBEDS_KEY: &str = "e";

/// One level of a composed read: a relation, filtered, ordered and paged, with the levels
/// embedded beneath it.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedLevel {
    /// The view this level reads.
    pub view:         String,
    /// The `SELECT` expression for this level's document, in place of `data`.
    ///
    /// Only the root carries one, and only for a value the stored document does not
    /// hold — a `nearest` distance (#959). The expression is evaluated against this
    /// level's own relation.
    pub projection:   Option<String>,
    /// Security conditions AND-ed with the client filter, exactly as a flat read of the
    /// same query would compose them. The correlation to the parent is **not** in here:
    /// it is a column reference, not a value, and the renderer adds it.
    pub where_clause: Option<WhereClause>,
    /// This level's ordering, or `None` for the relation's own order.
    pub order_by:     Option<Vec<OrderByClause>>,
    /// This level's page size.
    pub limit:        Option<u32>,
    /// This level's page offset.
    pub offset:       Option<u32>,
    /// What this level's document is allowed to carry.
    pub keys:         LevelKeys,
    /// The levels embedded into each row of this one.
    pub embeds:       Vec<ComposedEmbed>,
}

/// Which keys of a stored document a composed level returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelKeys {
    /// The whole document. The root's shape: it is projected afterwards exactly as a flat
    /// read of the same query is, so it reads what a flat read reads.
    Whole,
    /// Only these keys, and these keys as `null`.
    ///
    /// An embedded level's shape. A key the caller may not see never leaves the
    /// database: `masked` keys are written as SQL `NULL`, and a key in neither list is
    /// not returned at all.
    Only {
        /// Stored keys returned with their values, when the document has them.
        kept:   Vec<String>,
        /// Stored keys returned as `null` whatever the document holds.
        masked: Vec<String>,
    },
}

/// How an embedded level is attached to each row of its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedShape {
    /// A JSON array of the related rows, `[]` when there are none.
    Many,
    /// The one related row, or `null`.
    ///
    /// The join key is unique on the target — a to-one relationship whose key is not is
    /// refused when the schema loads — so there is at most one row to return.
    One,
    /// The number of related rows. The level's page, ordering, keys and embeds do not
    /// apply to a count.
    Count,
}

/// An embedded level and the correlation that attaches it to its parent's rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedEmbed {
    /// The key the embedded value is written under in the parent row's `embeds` object.
    pub output_key: String,
    /// Array, object or count.
    pub shape:      EmbedShape,
    /// The stored path on the **embedded** row that must equal the parent's key.
    pub target_key: Vec<String>,
    /// The stored path on the **parent** row it is compared with.
    pub parent_key: Vec<String>,
    /// The declared scalar type both keys are compared as — the cast a filter on the
    /// embedded key would take, so the correlation matches the rows a predicate
    /// `target_key = <the parent's value>` matched when it was a flat sub-read.
    pub key_type:   ScalarFieldType,
    /// The embedded level itself.
    pub level:      ComposedLevel,
}
