use super::*;

async fn exists(db: &FreshDatabase, name: &str) -> bool {
    let admin = PgPoolOptions::new().max_connections(1).connect(db.service.url()).await.unwrap();
    let found = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
        .bind(name)
        .fetch_one(&admin)
        .await
        .unwrap();
    admin.close().await;
    found
}

/// A test that panics, or returns, before awaiting [`FreshDatabase::drop`] still loses
/// its database: the next run reads no leftover from it.
///
/// The database is made on the witness's server, which the witness keeps up: with no
/// `DATABASE_URL` each [`fresh_database`] spawns a server of its own, which would go with
/// the database and leave nothing to look on.
#[tokio::test]
async fn a_database_left_undropped_is_dropped_on_the_way_out() {
    let Some(witness) = fresh_database("witness").await else {
        return;
    };
    let db = database_on(Service::from_url(witness.service.url().to_owned()), "unwound").await;
    let name = db.name.clone();
    let _held = db.pool().await;
    let by_url = PgPoolOptions::new().max_connections(1).connect(&db.url()).await.unwrap();
    let reached: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&by_url)
        .await
        .unwrap();
    by_url.close().await;
    assert_eq!(reached, name, "the URL names another database");
    assert!(exists(&witness, &name).await, "{name} was never created");
    std::mem::drop(db);
    let gone = !exists(&witness, &name).await;
    witness.drop().await;
    assert!(gone, "{name} outlived the value that owned it");
}
