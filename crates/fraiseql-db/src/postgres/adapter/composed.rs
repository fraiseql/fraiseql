//! Rendering a [`ComposedLevel`] as one PostgreSQL statement.
//!
//! # The shape, and why each part is where it is
//!
//! A level is read as
//!
//! ```text
//! SELECT jsonb_build_object('d', <document>, 'e', jsonb_build_object(<embeds>)) AS data,
//!        _lN."_o" AS "_o"
//! FROM (SELECT data, row_number() OVER (ORDER BY <order>) AS "_o"
//!       FROM <view> WHERE <predicate> [AND <correlation>]
//!       ORDER BY "_o" LIMIT $n OFFSET $m) AS _lN
//! LEFT JOIN LATERAL (<embed>) AS _lN_e0 ON true …
//! ```
//!
//! * **The page is taken before anything is embedded.** The `LIMIT` sits in the derived table the
//!   `LATERAL`s join against, so an embed is computed for the rows of the page and for no others.
//!   Written as one flat `SELECT … LEFT JOIN LATERAL … ORDER BY … LIMIT`, the planner would be free
//!   to evaluate every embed for every matching parent row before it cut the page.
//! * **An embed's `ORDER BY` and `LIMIT` are inside its own `LATERAL`,** so each parent row gets
//!   its own page of related rows — which is what a sub-read per parent row returned, and what the
//!   cost estimate charges (`DirectReadProjection`).
//! * **Order travels as a number, `"_o"`.** `jsonb_agg(… ORDER BY "_o")` and the root's outer
//!   `ORDER BY "_o"` preserve the level's ordering through the joins; the order of rows out of a
//!   derived table is otherwise not something SQL promises.
//! * **The correlation is the only reference across levels**, and the only qualified column
//!   reference in the statement. Everything the where generator renders is unqualified `data`,
//!   which resolves to the innermost relation — the level's own view — so a level's security
//!   predicate cannot bind to its parent's row by accident.
//!
//! Aliases are numbered across the whole statement rather than per depth, so a correlation
//! names exactly one relation however deep it sits.

use std::fmt::Write;

use fraiseql_error::Result;

use super::super::where_generator::PostgresWhereGenerator;
use crate::{
    dialect::{PostgresDialect, SqlDialect},
    identifier::quote_postgres_identifier,
    order_by::{Tiebreak, render_order_by_columns},
    path_escape::escape_postgres_jsonb_segment,
    traits::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, EmbedShape,
        LevelKeys,
    },
    types::{DatabaseType, QueryParam},
};

/// The column each level carries its ordinal in.
const ORDINAL: &str = "\"_o\"";

/// Render `root` as one statement returning one JSONB column, one row per root row, in
/// the root's order.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if a WHERE clause or an ordering fails to render
/// (an unsupported operator, an invalid field name).
pub(in super::super) fn build_composed_select_sql(
    root: &ComposedLevel,
) -> Result<(String, Vec<QueryParam>)> {
    let mut renderer = Renderer::default();
    let level = renderer.level(root, None)?;
    Ok((
        format!("SELECT _r.data FROM ({level}) AS _r ORDER BY _r.{ORDINAL}"),
        renderer.params,
    ))
}

/// The statement's parameters and alias counter, threaded through the recursion.
#[derive(Default)]
struct Renderer {
    params:  Vec<QueryParam>,
    aliases: usize,
}

impl Renderer {
    /// The next statement-wide relation alias.
    fn alias(&mut self) -> String {
        let alias = format!("_l{}", self.aliases);
        self.aliases += 1;
        alias
    }

    /// Bind `param` and return its placeholder.
    fn bind(&mut self, param: QueryParam) -> String {
        self.params.push(param);
        format!("${}", self.params.len())
    }

    /// `WHERE …` for a level: its own predicate, AND-ed with its correlation to the
    /// parent row when it has one. Empty when there is neither.
    fn where_sql(
        &mut self,
        level: &ComposedLevel,
        correlation: Option<&Correlation<'_>>,
    ) -> Result<String> {
        let mut conditions = Vec::new();
        if let Some(clause) = level.where_clause.as_ref() {
            let generator = PostgresWhereGenerator::new(PostgresDialect);
            let (sql, params) = generator.generate_with_param_offset(clause, self.params.len())?;
            self.params.extend(params.into_iter().map(QueryParam::from));
            conditions.push(sql);
        }
        if let Some(correlation) = correlation {
            conditions.push(correlation.sql());
        }
        Ok(if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        })
    }

    /// The level's own relation, filtered, numbered in its order, and paged.
    fn page(
        &mut self,
        level: &ComposedLevel,
        correlation: Option<&Correlation<'_>>,
    ) -> Result<String> {
        let document = level.projection.as_deref().unwrap_or("data");
        let mut sql = String::new();
        let where_sql = self.where_sql(level, correlation)?;

        // `Tiebreak::Identity`: the level is paged, so its order has to be total or the page
        // is a slice of no sequence (#1287) — the same rule a flat read of it follows.
        let window = match render_order_by_columns(
            level.order_by.as_deref(),
            DatabaseType::PostgreSQL,
            self.params.len() + 1,
            Tiebreak::Identity,
        )? {
            Some(rendered) => {
                self.params.extend(rendered.params.into_iter().map(QueryParam::Text));
                format!("ORDER BY {}", rendered.columns)
            },
            None => String::new(),
        };

        // Reason (expect below): fmt::Write for String is infallible.
        write!(
            sql,
            "SELECT {document} AS data, row_number() OVER ({window}) AS {ORDINAL} FROM {}{where_sql} \
             ORDER BY {ORDINAL}",
            quote_postgres_identifier(&level.view),
        )
        .expect("write to String");
        if let Some(limit) = level.limit {
            let p = self.bind(QueryParam::BigInt(i64::from(limit)));
            write!(sql, " LIMIT {p}").expect("write to String");
        }
        if let Some(offset) = level.offset {
            let p = self.bind(QueryParam::BigInt(i64::from(offset)));
            write!(sql, " OFFSET {p}").expect("write to String");
        }
        Ok(sql)
    }

    /// A level: its page, its document, and its embeds joined `LATERAL`.
    fn level(
        &mut self,
        level: &ComposedLevel,
        correlation: Option<&Correlation<'_>>,
    ) -> Result<String> {
        let alias = self.alias();
        let page = self.page(level, correlation)?;
        let document = document_sql(&alias, &level.keys);

        let mut joins = String::new();
        let mut embeds = Vec::with_capacity(level.embeds.len());
        for (i, embed) in level.embeds.iter().enumerate() {
            let embed_alias = format!("{alias}_e{i}");
            let correlation = Correlation {
                target_key:   &embed.target_key,
                parent_alias: &alias,
                parent_key:   &embed.parent_key,
                key_type:     embed.key_type,
            };
            let sql = self.embed(embed, &correlation)?;
            // Reason (expect below): fmt::Write for String is infallible.
            write!(joins, " LEFT JOIN LATERAL ({sql}) AS {embed_alias} ON true")
                .expect("write to String");
            embeds.push(format!("{}, {embed_alias}.v", literal(&embed.output_key)));
        }

        Ok(format!(
            "SELECT jsonb_build_object({}, {document}, {}, jsonb_build_object({})) AS data, \
             {alias}.{ORDINAL} AS {ORDINAL} FROM ({page}) AS {alias}{joins}",
            literal(COMPOSED_DOCUMENT_KEY),
            literal(COMPOSED_EMBEDS_KEY),
            embeds.join(", "),
        ))
    }

    /// One embedded level, as the subquery its parent joins `LATERAL`. Returns one row
    /// with one column, `v`.
    fn embed(&mut self, embed: &ComposedEmbed, correlation: &Correlation<'_>) -> Result<String> {
        match embed.shape {
            EmbedShape::Many => {
                let level = self.level(&embed.level, Some(correlation))?;
                Ok(format!(
                    "SELECT COALESCE(jsonb_agg(_c.data ORDER BY _c.{ORDINAL}), '[]'::jsonb) AS v \
                     FROM ({level}) AS _c"
                ))
            },
            EmbedShape::One => {
                // No row joins as NULL (`LEFT JOIN … ON true`), which `jsonb_build_object`
                // writes as JSON `null` — the to-one absent value.
                let level = self.level(&embed.level, Some(correlation))?;
                Ok(format!(
                    "SELECT _c.data AS v FROM ({level}) AS _c ORDER BY _c.{ORDINAL} LIMIT 1"
                ))
            },
            EmbedShape::Count => {
                let where_sql = self.where_sql(&embed.level, Some(correlation))?;
                Ok(format!(
                    "SELECT COUNT(*) AS v FROM {}{where_sql}",
                    quote_postgres_identifier(&embed.level.view)
                ))
            },
        }
    }
}

/// A level's document, read off its page alias.
fn document_sql(alias: &str, keys: &LevelKeys) -> String {
    match keys {
        LevelKeys::Whole => format!("{alias}.data"),
        LevelKeys::Only { kept, masked } => {
            // The kept keys are read with `jsonb_each` rather than named one by one, so a
            // key the document does not hold stays absent instead of arriving as `null`:
            // the projector tells "absent" from "null" when it picks between a field's
            // stored spellings.
            let kept_sql = if kept.is_empty() {
                "'{}'::jsonb".to_string()
            } else {
                let list = kept.iter().map(|k| literal(k)).collect::<Vec<_>>().join(", ");
                format!(
                    "(SELECT COALESCE(jsonb_object_agg(_k.key, _k.value), '{{}}'::jsonb) \
                     FROM jsonb_each({alias}.data) AS _k WHERE _k.key IN ({list}))"
                )
            };
            if masked.is_empty() {
                return kept_sql;
            }
            // A masked key is written as SQL NULL, so the value it names is never read out
            // of the database for this caller.
            let masked_sql =
                masked.iter().map(|k| format!("{}, NULL", literal(k))).collect::<Vec<_>>();
            format!("({kept_sql} || jsonb_build_object({}))", masked_sql.join(", "))
        },
    }
}

/// A single-quoted SQL string literal.
fn literal(value: &str) -> String {
    format!("'{}'", escape_postgres_jsonb_segment(value))
}

/// The predicate that attaches an embedded row to its parent's.
struct Correlation<'a> {
    target_key:   &'a [String],
    parent_alias: &'a str,
    parent_key:   &'a [String],
    key_type:     crate::types::sql_hints::ScalarFieldType,
}

impl Correlation<'_> {
    /// `<embedded key> = <parent key>`, both sides cast as the key's declared type.
    ///
    /// The flat sub-read this replaces handed the parent's key *value* to the where
    /// generator as a typed predicate on the embedded key, which cast the column by the
    /// field's declared type (`(data->>'fk')::bigint = …`) and compared text for an
    /// identity. Casting both sides by the same rule keeps the same rows matching. A
    /// `NULL` parent key matches nothing — "no related row", as it was.
    fn sql(&self) -> String {
        let dialect = PostgresDialect;
        let target = dialect.json_extract_scalar("data", self.target_key);
        let parent =
            dialect.json_extract_scalar(&format!("{}.data", self.parent_alias), self.parent_key);
        format!(
            "{} = {}",
            dialect.cast_expr_as(&target, self.key_type),
            dialect.cast_expr_as(&parent, self.key_type),
        )
    }
}
