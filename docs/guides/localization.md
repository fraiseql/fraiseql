# Localization: localized fields and the request locale

FraiseQL serves text in several languages without per-language columns, per-request joins,
or client-side fallback. A **localized field** is stored once as a map of locale to label and
returned to each client as a plain `String` in the locale of its request. The worked example
is `examples/localized-catalog/`, and every code block below is copied from it (a test fails
when they drift).

## Declare the locales

The project's `fraiseql.toml` names the locales the deployment serves, a fallback between
them, and how a request picks one:

```toml
[locale]
allowed = ["en-US", "fr", "fr-CA", "fr-FR", "de-DE"]
default = "en-US"
fallback = {"fr-CA" = "fr-FR"}
```

A request reads a localized field through its locale's chain: the locale, its explicit
fallbacks, its allowed truncations, then the default (`fr-CA → fr-FR → fr → en-US`). The
resolution order, collation and cache behaviour are described in
[Request locale](../features/request-locale.md).

## Declare a localized field

```python
@fraiseql.type(sql_source="tv_product")
class Product:
    """A product of the shared catalog. Its name is stored in every language at once."""

    id: ID
    sku: str
    name: fraiseql.Localized[str]
```

`data->'name'` holds `{"fr-FR": "Pomme", "en-US": "Apple", …}`. Clients see `name: String`.
The TypeScript SDK spells it `localized: true` on the field (or `"Localized<string>"`). The
compiler refuses `localized` on anything but a `String`, and on any schema without `[locale]`.

A selection may read another locale for one field, `name(locale: "de-DE")`, or every allowed
label, `nameTranslations { locale value }` (in `allowed`'s order; a key outside `allowed` is
never listed). A stored plain string reads as itself in every locale, which is how rows
written before a field became localized keep their value.

A label can be missing for a locale. A field declared non-null (`name: String!`) whose chain
finds no label is a field error, as any missing non-null value is
([non-null completion](../features/non-null-completion.md)): declare the field nullable
(`fraiseql.Localized[str] | None`) when a row may lack a label.

## Filter, sort, index

`where: {name: {eq: "Pomme"}}` compares the request locale's label, and `orderBy: {name: ASC}`
sorts it under the locale's ICU collation, with unlabelled rows last. `fraiseql compile`
prints a `CREATE INDEX CONCURRENTLY` per allowed locale on exactly the key those queries
read, collation included, so the planner can use it.

`fraiseql doctor --against-db` plans a probe for each (field, locale) and names the pairs no
index serves, so an equivalent index under another name counts.

An aggregate or window grouped, partitioned, sorted or filtered by a localized dimension reads
its label in the request locale: two rows sharing a French label are one group in `fr-FR`
and two in `en-US` if their English labels differ. A fact-table measure cannot be localized
(a label is text, and a measure is a number aggregated). A subscription filter on a
localized field is not supported yet (#1525), and a federation `@key` never is (#1526);
each is refused at compile.

A field gate covers the translations sibling too: `nameTranslations` is gated as `name` is
(`requires_scope`, `on_deny`, `authorize`). A masked field's sibling reads `[]`; a refused
one refuses the read; the field authorizer is asked about `name`.

## Write a localized field

```python
@fraiseql.mutation(sql_source="fn_rename_product", operation="update")
def rename_product(id: ID, name: fraiseql.Localized[str]) -> Product:
    """Set one or more of a product's labels; the others are kept."""
```

A localized argument or input field is a `LocalizedInput`: `{value: "Pomme"}` is the request
locale's label, `{translations: [{locale: "fr-FR", value: "Pomme"}]}` sets several, and a
`null` value removes a locale. Over REST and MCP the value is a string (the request locale)
or the map. Every locale must be allowed, once; anything else is refused before any
statement. The SQL function receives the map, and merging it is the function's job:

```sql
    UPDATE tv_product
       SET data = jsonb_set(data, '{name}', jsonb_strip_nulls((data->'name') || p_name))
     WHERE id = p_id;
```

## Locale is data, not session

FraiseQL sets `fraiseql.locale` on read transactions only. A write, and any projection
refreshed inside it, never sees one, so nothing stored depends on who wrote it. Data that must
read in one language per tenant uses a **stored** locale, with a plain join:

```sql
           'product_label', COALESCE(p.data->'name'->>t.locale, p.data->'name'->>'en-US')
```

A view reading `current_setting('fraiseql.locale')` would serve whichever session last
materialized it. pg_tviews refuses such a definition (pg_tviews#193), and FraiseQL cannot see
one at compile time, so check the catalog after your migrations:

```sql
SELECT 'view' AS kind, schemaname || '.' || viewname AS name
FROM pg_views
WHERE definition ILIKE '%fraiseql.locale%'
```

The full query (views, materialized views, indexes, trigger functions) is
`examples/localized-catalog/sql/check_locale_free_projections.sql`; it should return no rows.
A guard that asserts the setting is unset must treat `''` like NULL: a pooled connection that
set it in an earlier transaction reads `''` afterwards.

```sql
    IF coalesce(current_setting('fraiseql.locale', true), '') <> '' THEN
```
