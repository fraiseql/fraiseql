//! A second initialisation over a current schema takes no lock a live write holds.
//!
//! Every replica runs its stores' schema initialisation at boot while the others serve
//! traffic. Statements such as `ALTER TABLE … ADD COLUMN IF NOT EXISTS`, `ENABLE ROW LEVEL
//! SECURITY`, `CREATE POLICY` or `CREATE INDEX IF NOT EXISTS` take a `SHARE` or
//! `ACCESS EXCLUSIVE` table lock even when they change nothing, so a boot over a current
//! schema would block the running replicas' reads or writes. A store guards each such
//! statement behind a catalog check; [`second_init_waits_on_nothing`] is the test that the
//! guards hold.
//!
//! The tables the live write holds are read from `pg_class`, every table in the store's
//! schemas, not from a list: a statement added later without a guard fails the test whether
//! or not anyone listed its table.
//!
//! Each test runs on a database of its own ([`fresh_database`]): the stores name their
//! schemas (`auth.`, `core.`, `_system.`), so a sibling test's boot in the shared database
//! would take the same tables this test holds.
//!
//! A guard that skips too much passes that test: a statement that never runs takes no lock.
//! [`boot_restores_the_pinned_shape`] is the other half. It pins what a boot builds (every
//! column, index, table's row-level security, policy and trigger) in a fixture, takes from a
//! current schema what each guard skips on, and asserts a boot builds the pinned shape again.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test helper, a panic is the test's failure report

use std::{
    collections::BTreeSet,
    fmt::Debug,
    future::Future,
    path::Path,
    str::FromStr,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use crate::Service;

/// How long the second initialisation may run before the test gives up on it.
const BOOT_DEADLINE: Duration = Duration::from_mins(1);

/// How often the test reads `pg_locks` while the second initialisation runs.
const POLL: Duration = Duration::from_millis(10);

/// A database created empty for one test, on the harness's PostgreSQL.
pub struct FreshDatabase {
    name:    String,
    options: PgConnectOptions,
    service: Service,
}

impl FreshDatabase {
    /// A new pool on this database: a modelled restart sees the same tables.
    ///
    /// # Panics
    ///
    /// If the database refuses the connection.
    pub async fn pool(&self) -> PgPool {
        PgPoolOptions::new()
            .max_connections(4)
            .connect_with(self.options.clone())
            .await
            .unwrap()
    }

    /// A URL to this database: the harness's URL with the database name swapped, for a
    /// helper that connects by URL or swaps the role.
    ///
    /// # Panics
    ///
    /// If the harness's URL has no `scheme://authority` part.
    #[must_use]
    pub fn url(&self) -> String {
        let url = self.service.url();
        let authority = url.find("://").unwrap() + 3;
        let path = url[authority..].find('/').map_or(url.len(), |slash| authority + slash);
        let query = url[path..].find('?').map_or("", |mark| &url[path + mark..]);
        format!("{}/{}{query}", &url[..path], self.name)
    }

    /// Drop the database, reporting a failure. A test that panics before calling this
    /// still loses its database: [`Drop`] drops it on the way out.
    ///
    /// # Panics
    ///
    /// If the server refuses the connection or the `DROP DATABASE`.
    pub async fn drop(mut self) {
        drop_database(self.service.url(), &self.name).await.unwrap();
        self.name.clear();
    }
}

impl Drop for FreshDatabase {
    /// Drop the database of a test that did not: one that panicked, or returned early.
    /// `Drop` cannot await and may run on a runtime's own thread, so the statement runs
    /// on a thread of its own with a runtime of its own; a failure is not reported, as a
    /// panic here would abort an unwinding test.
    fn drop(&mut self) {
        if self.name.is_empty() {
            return;
        }
        let url = self.service.url().to_string();
        let name = std::mem::take(&mut self.name);
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build()
            else {
                return;
            };
            drop(runtime.block_on(drop_database(&url, &name)));
        });
        drop(cleanup.join());
    }
}

/// `DROP DATABASE … WITH (FORCE)` on the server `url` names, ending the sessions the test's
/// pools still hold on it.
async fn drop_database(url: &str, name: &str) -> Result<(), sqlx::Error> {
    let admin = PgPoolOptions::new().max_connections(1).connect(url).await?;
    // Reason: a name [`fresh_database`] built and checked.
    let dropped =
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)")))
            .execute(&admin)
            .await;
    admin.close().await;
    dropped.map(drop)
}

/// A database of its own for the test tagged `tag`, created empty; `None` when no
/// PostgreSQL is available, so the test skips.
///
/// The name carries the tag, the process and a counter, so parallel tests and parallel
/// test binaries never share one.
///
/// # Panics
///
/// If the server refuses the connection or the `CREATE DATABASE`.
pub async fn fresh_database(tag: &str) -> Option<FreshDatabase> {
    Some(database_on(crate::postgres().await?, tag).await)
}

/// [`fresh_database`] on the server `service` reaches.
///
/// # Panics
///
/// As [`fresh_database`].
async fn database_on(service: Service, tag: &str) -> FreshDatabase {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
    let name = format!(
        "fq_boot_{tag}_{}_{}_{nanos}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    assert!(
        name.len() < 64
            && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "a database tag must be short lowercase ASCII: {tag:?}"
    );
    let admin = PgPoolOptions::new().max_connections(1).connect(service.url()).await.unwrap();
    // Reason: a name built above from a checked tag and numbers, not caller input.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    let options = PgConnectOptions::from_str(service.url()).unwrap().database(&name);
    FreshDatabase {
        name,
        options,
        service,
    }
}

/// A relation lock some session waits for, as `pg_locks` shows it.
#[derive(sqlx::FromRow)]
struct Wait {
    relation: String,
    mode:     String,
}

/// Run `init` twice on `db`, the second time while a live write holds every table in
/// `schemas`, and fail if the second run waits on any of them.
///
/// The live write is an open transaction holding `ROW EXCLUSIVE` (the lock an `INSERT`,
/// `UPDATE` or `DELETE` takes) on every ordinary and partitioned table `pg_class` lists in
/// `schemas` once the first run has created them. A statement that needs `SHARE` or
/// stronger on one of those tables waits for it; the failure names the table, the mode and
/// the statement.
///
/// Waits are read from `pg_locks` rather than cut short by `lock_timeout`: a store that
/// first takes a lock of its own, not a table's, may wait on that one for as long as it
/// likes.
///
/// # Panics
///
/// If either run fails, if `schemas` hold no table after the first run (a holder that
/// holds nothing would pass every store), if the second run waits on a held table, or if
/// it does not finish within a minute.
pub async fn second_init_waits_on_nothing<F, Fut, E>(db: &FreshDatabase, schemas: &[&str], init: F)
where
    F: Fn(PgPool) -> Fut,
    Fut: Future<Output = Result<(), E>> + Send + 'static,
    E: Debug + Send + 'static,
{
    let first = db.pool().await;
    init(first.clone())
        .await
        .unwrap_or_else(|e| panic!("the first initialisation failed: {e:?}"));
    first.close().await;

    let watch = db.pool().await;
    let mut live = watch.begin().await.unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p') ORDER BY 1",
    )
    .bind(schemas)
    .fetch_all(&mut *live)
    .await
    .unwrap();
    for table in &tables {
        // Reason: a name `format('%I.%I')` quoted above, read from the catalog.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("LOCK TABLE {table} IN ROW EXCLUSIVE MODE")))
            .execute(&mut *live)
            .await
            .unwrap();
    }
    let held: Vec<i64> = sqlx::query_scalar(
        "SELECT relation::bigint FROM pg_locks \
         WHERE pid = pg_backend_pid() AND locktype = 'relation' AND granted \
           AND mode = 'RowExclusiveLock' AND database = \
               (SELECT oid FROM pg_database WHERE datname = current_database())",
    )
    .fetch_all(&mut *live)
    .await
    .unwrap();
    assert!(
        !held.is_empty(),
        "the live write holds no table in {schemas:?}: the test would pass any store"
    );

    let second = db.pool().await;
    let booting = tokio::spawn(init(second.clone()));
    let started = tokio::time::Instant::now();
    let mut waits: Vec<Wait> = Vec::new();
    while !booting.is_finished() && started.elapsed() < BOOT_DEADLINE {
        waits = sqlx::query_as(
            "SELECT relation::regclass::text AS relation, mode FROM pg_locks \
             WHERE NOT granted AND locktype = 'relation' AND relation::bigint = ANY($1)",
        )
        .bind(&held)
        .fetch_all(&watch)
        .await
        .unwrap();
        if !waits.is_empty() {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    let finished = booting.is_finished();
    live.rollback().await.unwrap();
    let boot = booting.await.unwrap();
    second.close().await;
    watch.close().await;

    let waited: Vec<String> =
        waits.iter().map(|w| format!("{} on {}", w.mode, w.relation)).collect();
    assert!(
        waited.is_empty(),
        "a second initialisation over a current schema waited on a live write \
         ({} tables held): {waited:#?}",
        tables.len()
    );
    assert!(finished, "the second initialisation did not finish within {BOOT_DEADLINE:?}");
    boot.unwrap_or_else(|e| panic!("the second initialisation failed: {e:?}"));
}

/// The ordinary and partitioned tables in `schemas`, by OID.
async fn tables_in(pool: &PgPool, schemas: &[&str]) -> BTreeSet<i64> {
    sqlx::query_scalar(
        "SELECT c.oid::bigint FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = ANY($1) AND c.relkind IN ('r', 'p')",
    )
    .bind(schemas)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .collect()
}

/// What a boot built on `tables`, one line per object, sorted.
///
/// Every column with its type, nullability and default; every index with its definition;
/// each table's row-level security; every policy with its whole definition; every trigger.
async fn shape(pool: &PgPool, tables: &[i64]) -> Vec<String> {
    const QUERIES: &[&str] = &[
        "SELECT format('column %s.%s %s %s', c.oid::regclass, a.attname, \
                format_type(a.atttypid, a.atttypmod), \
                CASE WHEN a.attnotnull THEN 'not null' ELSE 'null' END) \
                || coalesce(' default ' || pg_get_expr(d.adbin, d.adrelid), '') \
         FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid \
         LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
         WHERE c.oid::bigint = ANY($1) AND a.attnum > 0 AND NOT a.attisdropped",
        "SELECT format('index %s', pg_get_indexdef(i.indexrelid)) FROM pg_index i \
         WHERE i.indrelid::bigint = ANY($1)",
        "SELECT format('row level security %s %s', c.oid::regclass, \
                CASE WHEN c.relrowsecurity THEN 'on' ELSE 'off' END) \
         FROM pg_class c WHERE c.oid::bigint = ANY($1)",
        "SELECT format('policy %s %s %s to %s for %s using %s check %s', p.polrelid::regclass, \
                p.polname, CASE WHEN p.polpermissive THEN 'permissive' ELSE 'restrictive' END, \
                (SELECT array_agg(CASE WHEN r = 0 THEN 'public' ELSE r::regrole::text END \
                                  ORDER BY 1) FROM unnest(p.polroles) r), \
                CASE p.polcmd WHEN 'r' THEN 'select' WHEN 'a' THEN 'insert' WHEN 'w' THEN 'update' \
                    WHEN 'd' THEN 'delete' ELSE 'all' END, \
                coalesce(pg_get_expr(p.polqual, p.polrelid), '-'), \
                coalesce(pg_get_expr(p.polwithcheck, p.polrelid), '-')) \
         FROM pg_policy p WHERE p.polrelid::bigint = ANY($1)",
        "SELECT format('trigger %s', pg_get_triggerdef(t.oid)) FROM pg_trigger t \
         WHERE t.tgrelid::bigint = ANY($1) AND NOT t.tgisinternal",
    ];
    let mut shape = BTreeSet::new();
    for query in QUERIES {
        // Reason: one of this module's own catalog queries.
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(*query))
            .bind(tables)
            .fetch_all(pool)
            .await
            .unwrap();
        shape.extend(rows);
    }
    shape.into_iter().collect()
}

/// Statements that take from `tables` what each guard skips on: every index no constraint
/// owns, row-level security, every trigger, every column `scripts` add with `ALTER TABLE …
/// ADD COLUMN`, and every policy's definition (each is replaced by one under the same name
/// that admits nothing).
async fn drift(pool: &PgPool, tables: &[i64], scripts: &[&str]) -> Vec<String> {
    let mut statements: Vec<String> = sqlx::query_scalar(
        "SELECT format('DROP INDEX %s', i.indexrelid::regclass) FROM pg_index i \
         WHERE i.indrelid::bigint = ANY($1) \
           AND NOT EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = i.indexrelid) \
         UNION ALL \
         SELECT format('ALTER TABLE %s DISABLE ROW LEVEL SECURITY', c.oid::regclass) \
         FROM pg_class c WHERE c.oid::bigint = ANY($1) AND c.relrowsecurity \
         UNION ALL \
         SELECT format('DROP TRIGGER %I ON %s', t.tgname, t.tgrelid::regclass) FROM pg_trigger t \
         WHERE t.tgrelid::bigint = ANY($1) AND NOT t.tgisinternal \
         UNION ALL \
         SELECT format('DROP POLICY %I ON %s; CREATE POLICY %I ON %s USING (false)', \
                p.polname, p.polrelid::regclass, p.polname, p.polrelid::regclass) \
         FROM pg_policy p WHERE p.polrelid::bigint = ANY($1)",
    )
    .bind(tables)
    .fetch_all(pool)
    .await
    .unwrap();
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT c.oid::regclass::text FROM pg_class c WHERE c.oid::bigint = ANY($1)",
    )
    .bind(tables)
    .fetch_all(pool)
    .await
    .unwrap();
    for script in scripts {
        for line in script.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let ["ALTER", "TABLE", table, "ADD", "COLUMN", column, ..] = words[..] {
                if column != "IF" && names.iter().any(|name| name == table) {
                    statements.push(format!("ALTER TABLE {table} DROP COLUMN {column}"));
                }
            }
        }
    }
    statements
}

/// A policy on one of a store's tables, as the catalog holds it.
#[derive(Clone, sqlx::FromRow)]
struct Policy {
    relation:     String,
    name:         String,
    /// The policy's name with a suffix: where the name variant moves it.
    moved:        String,
    /// A table in the policy's schema with the policy's table's columns: where the table
    /// variant moves it.
    elsewhere:    String,
    /// The policy's table's name in another schema: where the schema variant moves it.
    other_schema: String,
    permissive:   bool,
    /// `PUBLIC`, or the role names, comma-separated.
    roles:        String,
    cmd:          String,
    qual:         Option<String>,
    with_check:   Option<String>,
}

/// One policy changed in one attribute: the statements that make the change, and those that
/// remove what the change left beside the store's tables once a boot has run.
struct Variant {
    attribute: &'static str,
    apply:     Vec<String>,
    cleanup:   Vec<String>,
}

impl Policy {
    fn create_on(&self, relation: &str) -> String {
        let using = self.qual.as_ref().map_or_default(|q| format!(" USING ({q})"));
        let check = self.with_check.as_ref().map_or_default(|c| format!(" WITH CHECK ({c})"));
        format!(
            "CREATE POLICY {name} ON {relation} AS {kind} FOR {cmd} TO {roles}{using}{check}",
            name = self.name,
            kind = if self.permissive {
                "PERMISSIVE"
            } else {
                "RESTRICTIVE"
            },
            cmd = self.cmd,
            roles = self.roles,
        )
    }

    fn replaced_by(&self, attribute: &'static str, other: &Self) -> Variant {
        Variant {
            attribute,
            apply: vec![
                format!("DROP POLICY {} ON {}", self.name, self.relation),
                other.create_on(&self.relation),
            ],
            cleanup: Vec::new(),
        }
    }

    /// The policy dropped, and created under its name on `relation`, a copy of its table.
    fn moved_to(&self, attribute: &'static str, relation: &str, cleanup: &str) -> Variant {
        Variant {
            attribute,
            apply: vec![
                format!("CREATE SCHEMA IF NOT EXISTS {ELSEWHERE}"),
                format!("CREATE TABLE {relation} (LIKE {})", self.relation),
                self.create_on(relation),
                format!("DROP POLICY {} ON {}", self.name, self.relation),
            ],
            cleanup: vec![cleanup.to_string()],
        }
    }

    /// The policy changed in one attribute at a time, each change one a guard comparing the
    /// whole definition replaces: its kind, roles, command, either expression (changed where
    /// it has one, added where it has none), its name (the table holds the same definition
    /// under another name), its table (another table in its schema holds it under the same
    /// name) and its schema (a table of the same name in another schema holds it). An
    /// expression the command cannot carry (`USING` on an `INSERT` policy, `WITH CHECK` on
    /// a `SELECT` or `DELETE` one) is not added: PostgreSQL refuses it, so a guard's
    /// comparison of it is decided by the command's.
    fn variants(&self) -> Vec<Variant> {
        let other = |e: &str| {
            if e == "false" {
                "true".to_string()
            } else {
                "false".to_string()
            }
        };
        let roles = if self.roles == "PUBLIC" {
            "CURRENT_USER"
        } else {
            "PUBLIC"
        };
        let cmd = match (self.cmd.as_str(), &self.qual, &self.with_check) {
            ("ALL", _, None) => "SELECT",
            ("ALL", None, Some(_)) => "INSERT",
            ("ALL", Some(_), Some(_)) => "UPDATE",
            _ => "ALL",
        };
        let mut variants = vec![
            self.replaced_by(
                "permissive",
                &Self {
                    permissive: !self.permissive,
                    ..self.clone()
                },
            ),
            self.replaced_by(
                "roles",
                &Self {
                    roles: roles.into(),
                    ..self.clone()
                },
            ),
            self.replaced_by(
                "command",
                &Self {
                    cmd: cmd.into(),
                    ..self.clone()
                },
            ),
            Variant {
                attribute: "name",
                apply:     vec![format!(
                    "ALTER POLICY {} ON {} RENAME TO {}",
                    self.name, self.relation, self.moved
                )],
                cleanup:   vec![format!("DROP POLICY {} ON {}", self.moved, self.relation)],
            },
            self.moved_to("table", &self.elsewhere, &format!("DROP TABLE {}", self.elsewhere)),
            self.moved_to(
                "schema",
                &self.other_schema,
                &format!("DROP SCHEMA {ELSEWHERE} CASCADE"),
            ),
        ];
        // An expression the policy holds is flipped; one it lacks is added, where its
        // command can carry one.
        let using = self.qual.as_deref().map_or("false".to_string(), other);
        if self.qual.is_some() || self.cmd != "INSERT" {
            variants.push(self.replaced_by(
                "using",
                &Self {
                    qual: Some(using),
                    ..self.clone()
                },
            ));
        }
        let check = self.with_check.as_deref().map_or("false".to_string(), other);
        if self.with_check.is_some() || !matches!(self.cmd.as_str(), "SELECT" | "DELETE") {
            variants.push(self.replaced_by(
                "with check",
                &Self {
                    with_check: Some(check),
                    ..self.clone()
                },
            ));
        }
        variants
    }
}

/// The schema the schema variant moves a policy's table to.
const ELSEWHERE: &str = "fq_boot_elsewhere";

/// The policies on `tables`.
async fn policies(pool: &PgPool, tables: &[i64]) -> Vec<Policy> {
    sqlx::query_as(
        "SELECT p.polrelid::regclass::text AS relation, quote_ident(p.polname) AS name, \
                quote_ident(p.polname || '_moved') AS moved, \
                format('%I.%I', n.nspname, c.relname || '_elsewhere') AS elsewhere, \
                format('%I.%I', $2::text, c.relname) AS other_schema, \
                p.polpermissive AS permissive, \
                CASE WHEN p.polroles = '{0}' THEN 'PUBLIC' ELSE \
                    (SELECT string_agg(r::regrole::text, ', ') FROM unnest(p.polroles) r) \
                END AS roles, \
                CASE p.polcmd WHEN 'r' THEN 'SELECT' WHEN 'a' THEN 'INSERT' \
                    WHEN 'w' THEN 'UPDATE' WHEN 'd' THEN 'DELETE' ELSE 'ALL' END AS cmd, \
                pg_get_expr(p.polqual, p.polrelid) AS qual, \
                pg_get_expr(p.polwithcheck, p.polrelid) AS with_check \
         FROM pg_policy p JOIN pg_class c ON c.oid = p.polrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE p.polrelid::bigint = ANY($1) ORDER BY 1, 2",
    )
    .bind(tables)
    .bind(ELSEWHERE)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Run `statements` on `pool`, each named in the panic if it fails.
async fn execute(pool: &PgPool, statements: &[String]) {
    for statement in statements {
        // Reason: statements built by this module from catalog names and a store's scripts.
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.clone()))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

/// The lines of `built` that `rebuilt` lacks, and those it has beyond them.
fn difference<'a>(
    built: &'a [String],
    rebuilt: &'a [String],
) -> (Vec<&'a String>, Vec<&'a String>) {
    (
        built.iter().filter(|line| !rebuilt.contains(line)).collect(),
        rebuilt.iter().filter(|line| !built.contains(line)).collect(),
    )
}

/// Assert `shape` is the text pinned in `fixture`; where `FRAISEQL_BLESS` is set, write it
/// there instead.
fn assert_pinned(shape: &[String], fixture: &Path, when: &str) {
    let mut text = shape.join("\n");
    text.push('\n');
    if std::env::var_os("FRAISEQL_BLESS").is_some() {
        std::fs::write(fixture, &text).unwrap();
        return;
    }
    let pinned = std::fs::read_to_string(fixture).unwrap_or_else(|e| {
        panic!("{}: {e}; run with FRAISEQL_BLESS=1 to write it", fixture.display())
    });
    let pinned: BTreeSet<&str> = pinned.lines().collect();
    let built: BTreeSet<&str> = shape.iter().map(String::as_str).collect();
    let missing: Vec<_> = pinned.difference(&built).collect();
    let extra: Vec<_> = built.difference(&pinned).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{when}, the schema is not the one pinned in {}\nmissing: {missing:#?}\nextra: {extra:#?}",
        fixture.display()
    );
}

/// A boot builds the shape pinned in `fixture` on the tables it creates, and builds it again
/// over a schema that lost what each guard skips on.
///
/// The store's tables are those in `schemas` that the first boot creates, so tables an
/// earlier store made (a foreign key's target) are neither pinned nor taken. The second half
/// is what a guard that skips too much fails: an index, a column, a table's row-level
/// security, a trigger or a policy definition that a boot over a drifted schema leaves
/// missing or wrong. The columns to take are the ones `scripts` add.
///
/// # Panics
///
/// If either boot fails, if the first creates no table in `schemas`, if it does not build
/// the pinned shape, if the drift takes nothing (the test would pass any store), or if the
/// second boot does not build the pinned shape again.
pub async fn boot_restores_the_pinned_shape<F, Fut, E>(
    db: &FreshDatabase,
    schemas: &[&str],
    scripts: &[&str],
    fixture: &Path,
    init: F,
) where
    F: Fn(PgPool) -> Fut,
    Fut: Future<Output = Result<(), E>>,
    E: Debug,
{
    let pool = db.pool().await;
    let before = tables_in(&pool, schemas).await;
    init(pool.clone())
        .await
        .unwrap_or_else(|e| panic!("the first boot failed: {e:?}"));
    let tables: Vec<i64> = tables_in(&pool, schemas).await.difference(&before).copied().collect();
    assert!(!tables.is_empty(), "the first boot created no table in {schemas:?}");
    let built = shape(&pool, &tables).await;
    assert_pinned(&built, fixture, "after a first boot");

    let statements = drift(&pool, &tables, scripts).await;
    assert!(!statements.is_empty(), "nothing to drift: the test would pass any store");
    execute(&pool, &statements).await;
    let drifted = shape(&pool, &tables).await;
    assert_ne!(drifted, built, "the drift changed nothing");

    init(pool.clone())
        .await
        .unwrap_or_else(|e| panic!("the boot over a drifted schema failed: {e:?}"));
    let rebuilt = shape(&pool, &tables).await;
    // Against the first boot's shape, not the fixture: under `FRAISEQL_BLESS` the fixture is
    // being written, and a rebuild that differs must still fail.
    let (missing, extra) = difference(&built, &rebuilt);
    assert!(
        missing.is_empty() && extra.is_empty(),
        "a boot over a drifted schema did not build the first boot's shape\n\
         missing: {missing:#?}\nextra: {extra:#?}"
    );

    for policy in policies(&pool, &tables).await {
        for variant in policy.variants() {
            let what = format!(
                "policy {} on {} with another {}",
                policy.name, policy.relation, variant.attribute
            );
            execute(&pool, &variant.apply).await;
            assert_ne!(shape(&pool, &tables).await, built, "{what} changed nothing");
            init(pool.clone())
                .await
                .unwrap_or_else(|e| panic!("a boot over {what} failed: {e:?}"));
            execute(&pool, &variant.cleanup).await;
            let rebuilt = shape(&pool, &tables).await;
            let (missing, extra) = difference(&built, &rebuilt);
            assert!(
                missing.is_empty() && extra.is_empty(),
                "a boot over {what} did not build the first boot's shape\n\
                 missing: {missing:#?}\nextra: {extra:#?}"
            );
        }
    }
    pool.close().await;
}

#[cfg(test)]
mod tests;
