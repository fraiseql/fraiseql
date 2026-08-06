use super::{SAMPLE_SCHEMA_SQL, SAMPLE_SEED_SQL};

/// The scripts minus their `--` line comments: the header prose explains the
/// hazard by naming `gen_random_uuid()`, and prose must not satisfy — or trip —
/// a check about executed SQL.
fn statements(sql: &str) -> String {
    sql.lines()
        .map(|line| line.split_once("--").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The property the whole module exists to guarantee: re-applying the seed must
/// be a no-op. It is only a no-op while every row has a fixed id — a
/// `gen_random_uuid()` id makes the conflict clause dead and each application
/// appends another copy (#996).
#[test]
fn seed_is_idempotent_by_construction() {
    let seed = statements(SAMPLE_SEED_SQL);
    assert!(
        !seed.contains("gen_random_uuid()"),
        "sample seed generates ids at insert time — its ON CONFLICT clauses can never \
         fire, so every application duplicates the seed (#996)"
    );
    let inserts = seed.matches("INSERT INTO").count();
    let guards = seed.matches("ON CONFLICT (id) DO NOTHING").count();
    assert_eq!(
        inserts, guards,
        "every INSERT in the sample seed must carry ON CONFLICT (id) DO NOTHING \
         ({inserts} inserts, {guards} guarded)"
    );
}

#[test]
fn schema_is_idempotent() {
    let schema = statements(SAMPLE_SCHEMA_SQL);
    for (stmt, guard) in [
        ("CREATE TABLE", "CREATE TABLE IF NOT EXISTS"),
        ("CREATE INDEX", "CREATE INDEX IF NOT EXISTS"),
    ] {
        assert_eq!(
            schema.matches(stmt).count(),
            schema.matches(guard).count(),
            "every `{stmt}` in the sample schema must be `{guard}` — consumers apply it \
             on every provisioning"
        );
    }
}
