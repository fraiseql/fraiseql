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
//! # Where an embedded level's rows come from
//!
//! Either from its own view, correlated to the parent row ([`EmbedSource::Correlated`]),
//! or from the parent row itself ([`EmbedSource::Materialised`]): the objects a view has
//! already embedded in its document, read one by one as the level's rows. The second is
//! how a GraphQL selection into a nested type is gated — its type's predicate filters the
//! elements the parent's view materialised, exactly as it filters rows of the type's own
//! view — without a join the schema may not be able to express.
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

use fraiseql_error::FraiseQLError;

use crate::{OrderByClause, ScalarFieldType, WhereClause};

/// The refusal a composed read of `view` gets from an adapter that cannot compose.
///
/// One constructor for both places that refuse — the trait's default
/// `execute_composed_with_session` and the engine, which refuses from
/// `supports_composed_reads()` before it would call it — so the two cannot word it
/// differently. `Unsupported` is a `501`.
#[must_use]
pub fn composed_read_unsupported(view: &str) -> FraiseQLError {
    FraiseQLError::Unsupported {
        message: format!(
            "Embedding related resources into a read of '{view}' needs a composed read, which \
             this database adapter does not implement"
        ),
    }
}

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
    /// A keyset page in place of an offset one: how a Relay connection pages. Only the
    /// root can carry one, and not with an `offset`; the renderer refuses anything else.
    pub keyset:       Option<ComposedKeyset>,
    /// What this level's document is allowed to carry.
    pub keys:         LevelKeys,
    /// The levels embedded into each row of this one.
    pub embeds:       Vec<ComposedEmbed>,
}

/// A root level paged by keyset, as a Relay connection pages.
///
/// The rows past `cursor` on `cursor_column`, in `order_by` then `cursor_column`, forward
/// or backward, `limit` of them — `RelayDatabaseAdapter::execute_relay_page`'s page. A
/// backward page is read descending and returned in ascending `cursor_column` order, as
/// the relay page is, so a connection answers the same page whichever of the two reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedKeyset {
    /// The native column the page is keyed on (`relay_cursor_column`).
    pub cursor_column: String,
    /// The cursor the page starts past: `after` when forward, `before` when backward.
    pub cursor:        Option<super::RelayCursor>,
    /// `true` for `first`/`after`, `false` for `last`/`before`.
    pub forward:       bool,
}

/// Which keys of a stored document a composed level returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelKeys {
    /// The whole document. The root's shape: it is projected afterwards exactly as a flat
    /// read of the same query is, so it reads what a flat read reads.
    Whole,
    /// The whole document, less these keys.
    ///
    /// The root's shape when some of its stored keys are embedded in their own place:
    /// the embedded value is the gated one, and the stored one — which the level's
    /// predicate never filtered — does not leave the database.
    Without(Vec<String>),
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

/// An embedded level and where its rows come from.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedEmbed {
    /// The key the embedded value is written under in the parent row's `embeds` object.
    pub output_key: String,
    /// Array, object or count.
    pub shape:      EmbedShape,
    /// The level's rows: its own view's, correlated, or its parent document's.
    pub source:     EmbedSource,
    /// The embedded level itself.
    pub level:      ComposedLevel,
}

/// Where an embedded level's rows come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedSource {
    /// Rows of the level's view whose `target_key` equals the parent row's `parent_key`.
    Correlated {
        /// The stored path on the **embedded** row that must equal the parent's key.
        target_key: Vec<String>,
        /// The stored path on the **parent** row it is compared with.
        parent_key: Vec<String>,
        /// The declared scalar type both keys are compared as — the cast a filter on the
        /// embedded key would take, so the correlation matches the rows a predicate
        /// `target_key = <the parent's value>` matched when it was a flat sub-read.
        key_type:   ScalarFieldType,
    },
    /// The value the parent row's own document holds under the first of `keys` it has:
    /// each object element of it for [`EmbedShape::Many`], the object itself for
    /// [`EmbedShape::One`]. The level's view is not read.
    ///
    /// The level's predicate is evaluated against each element as if it were a row of the
    /// level's view, so it may only read the element's document: a native-column condition
    /// has no column to read here, and is refused rather than rendered. A count, an
    /// ordering and a projection are refused too — the elements keep the order the parent
    /// stored them in.
    Materialised {
        /// The parent's stored spellings of the key, in the order they are tried.
        keys: Vec<String>,
    },
}
