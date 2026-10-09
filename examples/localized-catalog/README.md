# Localized catalog

A product catalog shared by many tenants, whose names exist in several languages. Each name
is **stored** once, as a map of locale to label, and **returned** to every client as a plain
string in the locale of its request (#1512, #1513).

```
schema.py                            Python SDK authoring (exports catalog.json)
catalog.json                         the export, committed; check_examples.sh keeps it fresh
fraiseql.toml                        includes catalog.json; [locale]: locales, fallback, order
sql/01_schema.sql                    tables, the tenant order view, the rename function
sql/check_locale_free_projections.sql  the projection gate (see below)
```

The example is driven end to end by
`crates/fraiseql-server/tests/example_localized_catalog_e2e_pg.rs` against PostgreSQL 18.

## The request locale

`fraiseql.toml` declares the locales the deployment serves and how a request picks one: an
explicit `locale` argument, then the browser's `Accept-Language`, then the locale stored for
the caller's tenant. The last needs the server's identity enrichment:

```toml
# server config
[identity.enrichment]
enabled = true
query   = "SELECT locale FROM tb_tenant WHERE sub = $sub"
map     = { locale = "tenant_locale" }
```

The enrichment is fail-closed: a caller whose subject has no row is refused. The example
gives visitors a guest tenant.

## Localized fields

`Product.name` is `fraiseql.Localized[str]`. A query reads `name` as the request locale's
label, through the fallback chain (`fr-CA → fr-FR → fr → en-US`); `where` and `orderBy` on it
compare and sort that label under the locale's collation. `name(locale: "de-DE")` reads one
field in another allowed locale, and `nameTranslations { locale value }` lists every label.

`renameProduct(id, name: {value: "…"})` writes a label for the request locale;
`{translations: [{locale, value}]}` writes several, and a `null` value removes one. Over REST
the value is a string (the request locale) or the map itself. Either way `fn_rename_product`
receives a map and merges it into the stored one.

`fraiseql compile` prints the expression index each allowed locale's filter or sort reads,
and `fraiseql doctor --against-db` names any that is missing.

## Locale is data, not session

FraiseQL sets `fraiseql.locale` on **read** transactions only. A write, and any projection
refreshed inside it, never sees a locale, so nothing stored can depend on who wrote it.
Tenant-scoped data that must read in one language uses a **stored** locale:
`v_tenant_order` renders each order's product label in the tenant's `locale` column with a
plain join. A view reading `current_setting('fraiseql.locale')` instead would serve whichever
session last materialized it; pg_tviews refuses such a definition (pg_tviews#193).

Two checks keep that true, and both are run by the example's test:

- `assert_write_has_no_locale()`, a trigger on `tv_product`, refuses a write that carries a
  locale. Unset reads as NULL on a fresh connection and as `''` on a pooled one, and both
  mean "no locale".
- `sql/check_locale_free_projections.sql` lists every view, materialized view, index and
  trigger function that reads the setting. Run it after your migrations; it should return
  no rows.

## Running it

```bash
make db-up
createdb localized_catalog && psql -d localized_catalog -f sql/01_schema.sql
fraiseql compile fraiseql.toml
```

Then serve `schema.compiled.json` with the enrichment above and an HS256 or OIDC issuer whose
tokens carry `sub` and `tenant_id`.
