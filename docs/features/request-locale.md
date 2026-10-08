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
