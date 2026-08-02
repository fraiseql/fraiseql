//! #823/#822 — a new user's first run, end to end, against live `PostgreSQL`.
//!
//! Pass 3 found there was **no path through the scaffold**: `fraiseql init`
//! generated views with plain columns and no `data` JSONB column, so every
//! declared query failed with `column "data" does not exist` (#823), and the
//! printed next step (`fraiseql compile fraiseql.toml`) could not succeed on
//! the very project init had just generated (#822).
//!
//! Two orthogonal pins:
//!
//! 1. [`printed_next_steps_all_succeed`] executes **what the tool prints** — every line under "Next
//!    steps:" is interpreted and run. A printed command that fails, or a printed line the
//!    interpreter does not understand, fails the test. This keeps the printed text and reality from
//!    drifting apart.
//! 2. [`scaffolded_project_serves_its_first_query`] executes **what the scaffold is** — applies the
//!    generated DDL to a scratch database, compiles the generated `schema.json`, seeds one row, and
//!    runs a GraphQL query through the real runtime (`fraiseql query` → `Executor` +
//!    `PostgresAdapter`). This is the exact read path the server issues.
//!
//! Self-skips when no `DATABASE_URL` is set (inert in the database-free leg).
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{path::Path, process::Command};

use tempfile::TempDir;
use tokio_postgres::NoTls;

/// Scratch database name, unique to this suite. The suite runs with
/// `--test-threads=1` in the integration leg; the name keeps it isolated from
/// the shared fixtures either way (#936).
const SCRATCH_DB: &str = "fraiseql_first_run_e2e";

const fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_fraiseql-cli")
}

/// Derive the scratch-database URL from the admin URL by swapping the db name.
fn scratch_url(admin_url: &str) -> String {
    let (base, _db) = admin_url.rsplit_once('/').expect("DATABASE_URL has a /dbname suffix");
    format!("{base}/{SCRATCH_DB}")
}

/// (Re)create the scratch database. Returns a client connected to it.
async fn recreate_scratch(admin_url: &str) -> tokio_postgres::Client {
    let (admin, conn) = tokio_postgres::connect(admin_url, NoTls).await.expect("admin connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .await
        .expect("drop scratch db");
    admin
        .batch_execute(&format!("CREATE DATABASE {SCRATCH_DB}"))
        .await
        .expect("create scratch db");

    let url = scratch_url(admin_url);
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.expect("scratch connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

async fn drop_scratch(admin_url: &str) {
    if let Ok((admin, conn)) = tokio_postgres::connect(admin_url, NoTls).await {
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let _ = admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
            .await;
    }
}

/// Run the CLI binary with `args` in `dir`, with `DATABASE_URL` pointed at the
/// scratch database. Returns (success, stdout, stderr).
fn run_cli(dir: &Path, db_url: &str, args: &[String]) -> (bool, String, String) {
    let output = Command::new(cli_bin())
        .args(args)
        .current_dir(dir)
        .env("DATABASE_URL", db_url)
        .output()
        .expect("failed to spawn fraiseql-cli");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Minimal shell-style splitter: whitespace-separated, honouring single and
/// double quotes. Enough for the command lines `init` prints; anything fancier
/// in the printed steps should fail the interpreter loudly rather than be
/// half-understood here.
fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            },
            (Some(_) | None, _) => cur.push(c),
        }
    }
    assert!(quote.is_none(), "unbalanced quote in printed step: {line}");
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// Extract the indented command lines printed after "Next steps:".
fn parse_next_steps(stdout: &str) -> Vec<String> {
    let mut steps = Vec::new();
    let mut in_steps = false;
    for line in stdout.lines() {
        if line.trim_start().starts_with("Next steps:") {
            in_steps = true;
            continue;
        }
        if in_steps {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            steps.push(trimmed.to_string());
        }
    }
    assert!(!steps.is_empty(), "init printed no 'Next steps:' block:\n{stdout}");
    steps
}

/// Apply one SQL file to the scratch database via the simple-query protocol
/// (dollar-quoted function bodies survive; matches `psql -f`).
async fn apply_sql_file(client: &tokio_postgres::Client, path: &Path) {
    let sql = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    client
        .batch_execute(&sql)
        .await
        .unwrap_or_else(|e| panic!("scaffold DDL {} must apply cleanly: {e}", path.display()));
}

/// All scaffolded SQL files in apply order (path sort matches the numbered
/// layout convention: `01_write` before `02_read` before `03_functions`).
fn scaffold_sql_files(project_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![project_dir.join("db")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("scaffold db/ dir readable") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "sql") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "scaffold produced no SQL files under db/");
    files
}

/// Pin #822: every command `fraiseql init` prints under "Next steps:" must
/// succeed on the project it just generated. The interpreter understands the
/// small vocabulary the tool prints (`cd`, `fraiseql …`, `psql … -f <file>`,
/// `git …`); an un-interpretable printed line is itself a failure, so the
/// printed text cannot drift away from this test.
#[tokio::test]
async fn printed_next_steps_all_succeed() {
    let Some(admin_url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let scratch = recreate_scratch(&admin_url).await;
    let db_url = scratch_url(&admin_url);

    let temp = TempDir::new().unwrap();
    let (ok, stdout, stderr) =
        run_cli(temp.path(), &db_url, &["init".into(), "blog".into(), "--no-git".into()]);
    assert!(ok, "fraiseql init failed:\nstdout: {stdout}\nstderr: {stderr}");

    let mut cwd = temp.path().to_path_buf();
    for step in parse_next_steps(&stdout) {
        let words = split_command(&step);
        match words.first().map(String::as_str) {
            Some("cd") => {
                cwd = cwd.join(&words[1]);
                assert!(cwd.is_dir(), "printed `cd {}` targets a missing directory", words[1]);
            },
            Some("fraiseql") => {
                let args: Vec<String> = words[1..].to_vec();
                let (ok, out, err) = run_cli(&cwd, &db_url, &args);
                assert!(
                    ok,
                    "printed next step `{step}` failed on the project init generated:\n\
                     stdout: {out}\nstderr: {err}"
                );
            },
            Some("psql") => {
                // Apply every `-f <file>` in order through the same
                // simple-query protocol psql uses.
                let mut iter = words.iter();
                let mut applied = 0;
                while let Some(w) = iter.next() {
                    if w == "-f" {
                        let file = iter.next().expect("psql -f takes a file argument");
                        apply_sql_file(&scratch, &cwd.join(file)).await;
                        applied += 1;
                    }
                }
                assert!(applied > 0, "printed psql step has no -f <file>: {step}");
            },
            Some("git") => {}, // scaffold-side git; not part of the product contract
            _ => panic!(
                "printed next step `{step}` is not interpretable — either fix the printed \
                 text or teach this test's interpreter the new step shape"
            ),
        }
    }

    drop_scratch(&admin_url).await;
}

/// Pin #823: the scaffold must serve its first query. Applies the generated
/// DDL, compiles the generated `schema.json`, seeds one author + one post, and
/// executes `{ posts { title } }` through the real runtime — the exact
/// `SELECT data FROM v_post` read path the server issues.
#[tokio::test]
async fn scaffolded_project_serves_its_first_query() {
    let Some(admin_url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let scratch = recreate_scratch(&admin_url).await;
    let db_url = scratch_url(&admin_url);

    let temp = TempDir::new().unwrap();
    let (ok, stdout, stderr) =
        run_cli(temp.path(), &db_url, &["init".into(), "blog".into(), "--no-git".into()]);
    assert!(ok, "fraiseql init failed:\nstdout: {stdout}\nstderr: {stderr}");
    let project = temp.path().join("blog");

    // The scaffold's own DDL must apply cleanly to an empty database.
    for file in scaffold_sql_files(&project) {
        apply_sql_file(&scratch, &file).await;
    }

    // The generated schema.json must compile.
    let (ok, out, err) = run_cli(
        &project,
        &db_url,
        &[
            "compile".into(),
            "schema.json".into(),
            "-o".into(),
            "schema.compiled.json".into(),
        ],
    );
    assert!(
        ok,
        "compile of the scaffolded schema.json failed:\nstdout: {out}\nstderr: {err}"
    );

    // Seed one author and one post, exactly as a new user would.
    scratch
        .batch_execute(
            "INSERT INTO tb_author (identifier, name, email) VALUES ('ada', 'Ada', 'ada@x.io');",
        )
        .await
        .expect("seed author");
    scratch
        .batch_execute(
            "INSERT INTO tb_post (identifier, title, body, published, author_id) \
             SELECT 'hello-world', 'Hello World', 'First post.', true, id FROM tb_author \
             WHERE identifier = 'ada';",
        )
        .await
        .expect("seed post");

    // The first query a user would run, through the real executor + adapter.
    // `createdAt` pins the multi-word contract: the SDK exports camelCase field
    // names, so the views must emit camelCase JSONB keys — a snake_case view
    // key would return null here.
    let (ok, out, err) = run_cli(
        &project,
        &db_url,
        &[
            "query".into(),
            "{ posts { title createdAt } }".into(),
            "--database".into(),
            db_url.clone(),
        ],
    );
    assert!(
        ok,
        "the scaffold's first query failed — the generated views do not satisfy \
         the runtime's `data` JSONB contract (#823):\nstdout: {out}\nstderr: {err}"
    );
    assert!(
        out.contains("Hello World"),
        "query succeeded but did not return the seeded post title:\n{out}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(out.trim()).expect("query output must be JSON");
    let posts = parsed["data"]["posts"].as_array().expect("posts must be a list");
    assert!(
        posts.iter().all(|p| !p["createdAt"].is_null()),
        "createdAt must be populated — the view's JSONB keys must be camelCase, \
         matching the SDK's exported field names:\n{out}"
    );

    drop_scratch(&admin_url).await;
}

/// Pin #569 end to end: on a freshly authored stack — `fraiseql setup` (helpers
/// + change-log contract) + the scaffold's DDL — a scaffolded mutation executes,
/// commits, and writes its `core.tb_entity_change_log` outbox row.
#[tokio::test]
async fn scaffolded_mutation_creates_a_post() {
    let Some(admin_url) = fraiseql_test_support::try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let scratch = recreate_scratch(&admin_url).await;
    let db_url = scratch_url(&admin_url);

    let temp = TempDir::new().unwrap();
    let (ok, stdout, stderr) =
        run_cli(temp.path(), &db_url, &["init".into(), "blog".into(), "--no-git".into()]);
    assert!(ok, "fraiseql init failed:\nstdout: {stdout}\nstderr: {stderr}");
    let project = temp.path().join("blog");

    // The documented install path for the mutation prerequisites (#569).
    let (ok, out, err) = run_cli(&project, &db_url, &["setup".into()]);
    assert!(ok, "fraiseql setup failed:\nstdout: {out}\nstderr: {err}");

    for file in scaffold_sql_files(&project) {
        apply_sql_file(&scratch, &file).await;
    }

    let (ok, out, err) = run_cli(&project, &db_url, &["compile".into(), "schema.json".into()]);
    assert!(ok, "compile failed:\nstdout: {out}\nstderr: {err}");

    scratch
        .batch_execute(
            "INSERT INTO tb_author (identifier, name, email) VALUES ('ada', 'Ada', 'ada@x.io');",
        )
        .await
        .expect("seed author");
    let author_id: String = scratch
        .query_one("SELECT id::text FROM tb_author WHERE identifier = 'ada'", &[])
        .await
        .expect("author id")
        .get(0);

    let mutation = format!(
        "mutation {{ createPost(identifier: \"hello-world\", title: \"Hello World\", \
         body: \"First post.\", authorId: \"{author_id}\") {{ title published }} }}"
    );
    let (ok, out, err) = run_cli(
        &project,
        &db_url,
        &[
            "query".into(),
            mutation,
            "--database".into(),
            db_url.clone(),
        ],
    );
    assert!(
        ok,
        "the scaffold's first mutation failed on a freshly set-up stack (#569):\n\
         stdout: {out}\nstderr: {err}"
    );
    assert!(
        out.contains("Hello World"),
        "mutation did not return the created entity:\n{out}"
    );

    // The row committed, and the change-log outbox row was written in-txn.
    let posts: i64 = scratch
        .query_one("SELECT count(*) FROM tb_post WHERE identifier = 'hello-world'", &[])
        .await
        .expect("count posts")
        .get(0);
    assert_eq!(posts, 1, "the created post must be committed");
    let outbox: i64 = scratch
        .query_one("SELECT count(*) FROM core.tb_entity_change_log WHERE object_type = 'Post'", &[])
        .await
        .expect("count outbox rows")
        .get(0);
    assert_eq!(outbox, 1, "the mutation must write its change-log outbox row (#569)");

    drop_scratch(&admin_url).await;
}
