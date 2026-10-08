# Request locale (`[locale]`)

Every request resolves to one locale that your deployment declares, whether it is
anonymous or authenticated and whatever transport it arrives on. SQL reads the resolved
locale as the `fraiseql.locale` setting on read transactions, and the result cache keeps
each locale's answers apart.

```toml
# fraiseql.toml
[locale]
default  = "en-US"
allowed  = ["en-US", "en-GB", "fr", "fr-FR", "de-DE"]
fallback = { "fr-CA" = "fr-FR" }   # optional; every target must be allowed
resolve  = [                        # optional; this is the default order
  { argument = "locale" },          # GraphQL `extensions.locale`, REST `?locale=`
  { header = "Accept-Language" },   # q-values honoured
  # { enrichment = "user_locale" }, # a field of the enriched identity
]
```

## Tags

`allowed`, `default` and `fallback` hold BCP 47 language tags in canonical casing:
`fr`, `fr-FR`, `zh-Hant-TW`, `es-419`. Extensions and private-use subtags are refused.
A tag becomes SQL text (a localized field's fallback literal, a collation name), so
`fraiseql compile` refuses a malformed tag and the server refuses it again when it loads
`schema.compiled.json`.

## Resolution

The sources in `resolve` are tried in order. A source offers candidates: an
`Accept-Language` header offers several, best q-value first; other sources offer one.
The first candidate that matches an allowed tag wins. A candidate matches:

1. an allowed tag, case-insensitively;
2. otherwise, its `fallback` entry;
3. otherwise, each shorter prefix of the tag, under the same two rules (`fr-BE` → `fr`).

A source with no match, a malformed value, `*`, `q=0`, or an `Accept-Language` value
longer than 1024 bytes falls through to the next source. When no source matches, the
request gets `default`. The resolved locale is therefore always an allowed tag, and a
request value never reaches SQL.

## From the user's profile

An `enrichment` source reads a field of the resolved identity (`[identity.enrichment]`):

```toml
[identity.enrichment]
enabled = true
query   = "SELECT coalesce(locale, '') AS locale FROM tb_user WHERE sub = $sub"
map     = { locale = "user_locale" }
```

The server refuses to boot when a `[locale]` enrichment source names a field that `map`
does not produce, or when no resolver is enabled. The resolver treats a `NULL` mapped
column as a denial of the identity, so a user without a stored locale must map to a
non-`NULL` value (an empty string matches nothing and falls through).

## Sorting: the locale's collation

With `[locale]`, an ordering on a text field sorts under the request locale's ICU
collation, `"<tag>-x-icu"`: Canadian French orders accents from the end of the word
(`côte` before `coté`), Swedish puts `Ä` after `Z`. This applies to list queries, relay
connections, REST `?sort=`, exports, gRPC reads, aggregate `orderBy` on a text dimension,
and window `ORDER BY`. Numbers, dates, IDs and enum values sort by their own type.

The server checks at boot that every allowed locale has its collation in the database
(`pg_collation`) and refuses to start, naming the missing ones, when it doesn't. A
PostgreSQL built with ICU provides them for the locales ICU knows.

Paging a relay connection with `after`/`before` under an `orderBy` on another field than
its cursor column is refused for now (#1521). Request the first page with `orderBy`.

## In SQL: reads only

A read transaction carries the locale as `fraiseql.locale`:

```sql
SELECT current_setting('fraiseql.locale', true);   -- 'fr-FR'
```

A write transaction never carries it. A trigger, or a projection refresh running inside
the write, sees `NULL`, so a stored projection cannot depend on who wrote it. When a write
needs the locale, pass it as data.

`fraiseql.locale` is reserved. Declaring it in `[[session_variables.variables]]` is
refused.

## Changing it

`[locale]` is fixed at boot. A schema reload that changes it is refused, as a change to
`security` is, so restart the server to apply it.
