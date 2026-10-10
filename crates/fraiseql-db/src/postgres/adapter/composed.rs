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
//! * **A materialised level's rows are named `data` too.** Its elements are read from the parent's
//!   document (`jsonb_array_elements(_lN.data->'key') … AS _mK(data, "_o")`), so the same
//!   unqualified predicate binds to the element — never to the parent it came from. The parent
//!   reference is the source expression, qualified, and the only one.
//!
//! Aliases are numbered across the whole statement rather than per depth, so a correlation
//! names exactly one relation however deep it sits.

use std::fmt::Write;

use fraiseql_error::{FraiseQLError, Result};

use super::super::where_generator::PostgresWhereGenerator;
use crate::{
    dialect::{PostgresDialect, SqlDialect},
    identifier::quote_postgres_identifier,
    order_by::{Tiebreak, render_order_by_columns},
    path_escape::escape_postgres_jsonb_segment,
    traits::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedKeyset, ComposedLevel,
        EmbedShape, EmbedSource, LevelKeys,
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
    let level = renderer.level(root, LevelFrom::View(None))?;
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
        if let Some(keyset) = level.keyset.as_ref() {
            return self.keyset_page(level, keyset, correlation);
        }
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

    /// The root's relation paged by keyset, exactly as the relay page reads it
    /// (`relay::run_relay_page`): past the cursor on the cursor column, ordered by the
    /// level's ordering then that column (`Tiebreak::None` — nothing may sit between the
    /// sort key and the column the next page resumes from, #1287), `limit` rows. A backward
    /// page is read descending and numbered in ascending cursor order, so the statement
    /// returns it the way the relay page does.
    fn keyset_page(
        &mut self,
        level: &ComposedLevel,
        keyset: &ComposedKeyset,
        correlation: Option<&Correlation<'_>>,
    ) -> Result<String> {
        if correlation.is_some() || level.offset.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "only the root of a composed read is paged by keyset, and not with an \
                     offset ('{}')",
                    level.view
                ),
                path:    None,
            });
        }
        // Refuses a relevance ordering (#1284); each key rendered as the `ORDER BY` renders it.
        let keys = crate::keyset::keyset_keys(level.order_by.as_deref())?;

        let document =
            crate::keyset::with_sort_keys(level.projection.as_deref().unwrap_or("data"), &keys);
        let column = quote_postgres_identifier(&keyset.cursor_column);

        let mut conditions = Vec::new();
        let where_sql = self.where_sql(level, None)?;
        if let Some(predicate) = where_sql.strip_prefix(" WHERE ") {
            conditions.push(format!("({predicate})"));
        }
        // #1521: past the cursor row's sort-key values, then its position.
        if let Some(cursor) = &keyset.cursor {
            let (position, placeholder) = crate::keyset::position_param(&cursor.position);
            let (predicate, key_params, _) = crate::keyset::keyset_predicate(
                &keys,
                &cursor.sort_keys,
                &column,
                placeholder,
                keyset.forward,
                self.params.len() + 1,
            )?;
            self.params.extend(key_params.into_iter().map(QueryParam::Text));
            self.params.push(position);
            conditions.push(predicate);
        }
        let where_sql = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let order = crate::keyset::keyset_order(&keys, &column, keyset.forward);
        let limit = match level.limit {
            Some(limit) => format!(" LIMIT {}", self.bind(QueryParam::BigInt(i64::from(limit)))),
            None => String::new(),
        };
        let view = quote_postgres_identifier(&level.view);

        Ok(if keyset.forward {
            format!(
                "SELECT {document} AS data, row_number() OVER (ORDER BY {order}) AS {ORDINAL} \
                 FROM {view}{where_sql} ORDER BY {ORDINAL}{limit}"
            )
        } else {
            // Read reversed from the cursor, then numbered in the requested order.
            let (key_columns, outer_keys) = crate::keyset::carried_keys(&keys, "_k", "_k");
            let outer_order = crate::keyset::keyset_order(&outer_keys, "_k._cursor", true);
            format!(
                "SELECT _k.data AS data, row_number() OVER (ORDER BY {outer_order}) AS {ORDINAL} \
                 FROM (SELECT {document} AS data, {column} AS _cursor{key_columns} FROM \
                 {view}{where_sql} ORDER BY {order}{limit}) AS _k"
            )
        })
    }

    /// A materialised level's rows: the parent document's elements under `keys`, as a
    /// relation with the columns a view's page has — `data` and `"_o"`.
    fn materialised_page(
        &mut self,
        level: &ComposedLevel,
        parent_alias: &str,
        keys: &[String],
        shape: EmbedShape,
    ) -> Result<String> {
        if level.order_by.is_some() || level.projection.is_some() || level.keyset.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "a level read from its parent's document keeps the order it was stored \
                     in and carries no projection ('{}')",
                    level.view
                ),
                path:    None,
            });
        }
        if level.where_clause.as_ref().is_some_and(reads_a_column) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "a level read from its parent's document has no columns, so its predicate \
                     may only read the document ('{}')",
                    level.view
                ),
                path:    None,
            });
        }

        if keys.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!("a materialised level names no key ('{}')", level.view),
                path:    None,
            });
        }
        let tried = keys
            .iter()
            .map(|k| format!("{parent_alias}.data->{}", literal(k)))
            .collect::<Vec<_>>()
            .join(", ");
        let stored = format!("COALESCE({tried})");
        let source = self.alias();
        let from = match shape {
            // A non-array value materialises no rows rather than failing the statement.
            EmbedShape::Many => format!(
                "jsonb_array_elements(CASE WHEN jsonb_typeof({stored}) = 'array' THEN {stored} \
                 ELSE '[]'::jsonb END) WITH ORDINALITY AS {source}(data, {ORDINAL})"
            ),
            EmbedShape::One => {
                format!("(SELECT {stored} AS data, 1::bigint AS {ORDINAL}) AS {source}")
            },
            EmbedShape::Count => {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "a level read from its parent's document cannot be counted ('{}')",
                        level.view
                    ),
                    path:    None,
                });
            },
        };

        // Only objects are rows: a `null` or a scalar element has no document to gate.
        let mut where_sql = self.where_sql(level, None)?;
        where_sql = if where_sql.is_empty() {
            " WHERE jsonb_typeof(data) = 'object'".to_string()
        } else {
            format!("{where_sql} AND jsonb_typeof(data) = 'object'")
        };
        let mut sql = format!("SELECT data, {ORDINAL} FROM {from}{where_sql} ORDER BY {ORDINAL}");
        if let Some(limit) = level.limit {
            let p = self.bind(QueryParam::BigInt(i64::from(limit)));
            // Reason (expect below): fmt::Write for String is infallible.
            write!(sql, " LIMIT {p}").expect("write to String");
        }
        if let Some(offset) = level.offset {
            let p = self.bind(QueryParam::BigInt(i64::from(offset)));
            write!(sql, " OFFSET {p}").expect("write to String");
        }
        Ok(sql)
    }

    /// A level: its page, its document, and its embeds joined `LATERAL`.
    fn level(&mut self, level: &ComposedLevel, from: LevelFrom<'_>) -> Result<String> {
        let alias = self.alias();
        let page = match from {
            LevelFrom::View(correlation) => self.page(level, correlation)?,
            LevelFrom::Parent {
                alias: parent,
                keys,
                shape,
            } => self.materialised_page(level, parent, keys, shape)?,
        };
        let document = document_sql(&alias, &level.keys);

        let mut joins = String::new();
        let mut embeds = Vec::with_capacity(level.embeds.len());
        for (i, embed) in level.embeds.iter().enumerate() {
            let embed_alias = format!("{alias}_e{i}");
            let sql = self.embed(embed, &alias)?;
            // Reason (expect below): fmt::Write for String is infallible.
            write!(joins, " LEFT JOIN LATERAL ({sql}) AS {embed_alias} ON true")
                .expect("write to String");
            embeds.push(format!("{}, {embed_alias}.v", literal(&embed.output_key)));
        }

        Ok(format!(
            "SELECT jsonb_build_object({}, {document}, {}, {}) AS data, \
             {alias}.{ORDINAL} AS {ORDINAL} FROM ({page}) AS {alias}{joins}",
            literal(COMPOSED_DOCUMENT_KEY),
            literal(COMPOSED_EMBEDS_KEY),
            crate::projection_generator::jsonb_object_sql(&embeds),
        ))
    }

    /// One embedded level, as the subquery its parent joins `LATERAL`. Returns one row
    /// with one column, `v`.
    fn embed(&mut self, embed: &ComposedEmbed, parent_alias: &str) -> Result<String> {
        let correlation;
        let from = match &embed.source {
            EmbedSource::Correlated {
                target_key,
                parent_key,
                key_type,
            } => {
                correlation = Correlation {
                    target_key,
                    parent_alias,
                    parent_key,
                    key_type: *key_type,
                };
                LevelFrom::View(Some(&correlation))
            },
            EmbedSource::Materialised { keys } => LevelFrom::Parent {
                alias: parent_alias,
                keys,
                shape: embed.shape,
            },
        };
        match embed.shape {
            EmbedShape::Many => {
                let level = self.level(&embed.level, from)?;
                Ok(format!(
                    "SELECT COALESCE(jsonb_agg(_c.data ORDER BY _c.{ORDINAL}), '[]'::jsonb) AS v \
                     FROM ({level}) AS _c"
                ))
            },
            EmbedShape::One => {
                // No row joins as NULL (`LEFT JOIN … ON true`), which `jsonb_build_object`
                // writes as JSON `null` — the to-one absent value.
                let level = self.level(&embed.level, from)?;
                Ok(format!(
                    "SELECT _c.data AS v FROM ({level}) AS _c ORDER BY _c.{ORDINAL} LIMIT 1"
                ))
            },
            EmbedShape::Count => {
                let LevelFrom::View(correlation) = from else {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "a level read from its parent's document cannot be counted ('{}')",
                            embed.level.view
                        ),
                        path:    None,
                    });
                };
                let where_sql = self.where_sql(&embed.level, correlation)?;
                Ok(format!(
                    "SELECT COUNT(*) AS v FROM {}{where_sql}",
                    quote_postgres_identifier(&embed.level.view)
                ))
            },
        }
    }
}

/// Where a level's rows come from, as the renderer threads it.
#[derive(Clone, Copy)]
enum LevelFrom<'a> {
    /// The level's own view, correlated to the parent row when it has one.
    View(Option<&'a Correlation<'a>>),
    /// The parent row's own document, under the first of `keys` it holds.
    Parent {
        alias: &'a str,
        keys:  &'a [String],
        shape: EmbedShape,
    },
}

/// Whether `clause` reads a column rather than the document — which a materialised
/// level, whose rows are documents and nothing else, cannot evaluate.
fn reads_a_column(clause: &crate::WhereClause) -> bool {
    use crate::WhereClause as W;
    match clause {
        W::Field { .. } => false,
        W::NativeField { .. } => true,
        W::And(all) | W::Or(all) => all.iter().any(reads_a_column),
        W::Not(inner)
        | W::Typed { inner, .. }
        | W::InHierarchy { inner, .. }
        | W::Localized { inner, .. } => reads_a_column(inner),
        W::Guarded { guard, inner, .. } => reads_a_column(guard) || reads_a_column(inner),
        // Its key is a document path; its predicate reads the related view inside its own
        // subquery (ruling AL).
        W::KeyIn { .. } => false,
    }
}

/// A level's document, read off its page alias.
fn document_sql(alias: &str, keys: &LevelKeys) -> String {
    match keys {
        LevelKeys::Whole => format!("{alias}.data"),
        LevelKeys::Without(dropped) if dropped.is_empty() => format!("{alias}.data"),
        LevelKeys::Without(dropped) => {
            let list = dropped.iter().map(|k| literal(k)).collect::<Vec<_>>().join(", ");
            format!("({alias}.data - ARRAY[{list}]::text[])")
        },
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
            format!(
                "({kept_sql} || {})",
                crate::projection_generator::jsonb_object_sql(&masked_sql)
            )
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
