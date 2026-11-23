//! #1551: `/metrics` reports the refresh counters `tviews.stats` keeps for each TVIEW the
//! schema reads, and says when the database cannot serve them.
//!
//! The unpreloaded case needs a database where `pg_tviews` was installed and a TVIEW
//! created while the library was preloaded, then restarted without it: the extension
//! refuses to install without the preload, so that is the only way to reach the state.
//! `docker/pg-tviews/unpreloaded-entrypoint.sh` builds it, in the `pg_tviews` leg and
//! through `tools/pg-tviews-unpreloaded.sh up` locally, for
//! `TVIEWS_UNPRELOADED_DATABASE_URL`. Without it that case skips, except where
//! `FRAISEQL_TEST_REQUIRE_TVIEWS_UNPRELOADED` is set (the leg, `make test-pg-tviews`),
//! where it fails.

use fraiseql_core::schema::{CompiledSchema, TypeDefinition};

use super::{metrics, run, serve, serve_schema};

/// The value of `metric` for `tview`, if `/metrics` reports one.
fn sample(rendered: &str, metric: &str, tview: &str) -> Option<f64> {
    let prefix = format!("{metric}{{tview=\"{tview}\"}} ");
    rendered.lines().find_map(|line| line.strip_prefix(&prefix)?.parse().ok())
}

#[tokio::test]
async fn a_refresh_counts_on_the_tview_it_wrote() {
    let Some(server) = serve().await else {
        return;
    };
    run(&server, "updatePost", "counted once").await;
    let before = metrics(&server).await;
    let written_before = sample(&before, "fraiseql_tview_rows_written_total", "p1391.tv_post")
        .unwrap_or_else(|| panic!("no rows_written for tv_post: {before}"));

    run(&server, "updatePost", "counted twice").await;
    let after = metrics(&server).await;
    let written_after =
        sample(&after, "fraiseql_tview_rows_written_total", "p1391.tv_post").unwrap();
    assert!(written_after > written_before, "{written_before} -> {written_after}: {after}");

    assert!(after.contains("\nfraiseql_tview_stats_available 1\n"), "{after}");
    assert!(after.contains("# TYPE fraiseql_tview_rows_written_total counter\n"), "{after}");
    for metric in [
        "fraiseql_tview_view_recomputes_total",
        "fraiseql_tview_full_refreshes_total",
        "fraiseql_tview_refresh_seconds_total",
        "fraiseql_tview_stats_reset_timestamp_seconds",
        "fraiseql_tview_stats_untracked",
    ] {
        for tview in ["p1391.tv_post", "p1391.tv_user"] {
            assert!(sample(&after, metric, tview).is_some(), "{metric} {tview}: {after}");
        }
    }
    assert_eq!(sample(&after, "fraiseql_tview_stats_untracked", "p1391.tv_post"), Some(0.0));
}

#[tokio::test]
async fn without_the_preload_the_scrape_says_the_counters_are_unavailable() {
    let Some(url) = fraiseql_test_support::tviews::tviews_unpreloaded_database_url() else {
        return;
    };
    // The rig's whole point: a change to it that handed back a preloaded server would turn
    // this case into a second copy of the preloaded one.
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    let preloaded: String =
        client.query_one("SHOW shared_preload_libraries", &[]).await.unwrap().get(0);
    assert!(!preloaded.contains("pg_tviews"), "the rig preloads pg_tviews: {preloaded}");
    let installed: bool = client
        .query_one("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_tviews')", &[])
        .await
        .unwrap()
        .get(0);
    assert!(installed, "the rig has no pg_tviews installed");

    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition::new("Item", "p1551.v_item"));
    let server = serve_schema(url, schema).await;

    let rendered = metrics(&server).await;
    assert!(
        sample(&rendered, "fraiseql_tview_dead_tuples", "p1551.tv_item").is_some(),
        "the health series needs no preload: {rendered}"
    );
    assert!(rendered.contains("\nfraiseql_tview_stats_available 0\n"), "{rendered}");
    for metric in [
        "fraiseql_tview_rows_written_total",
        "fraiseql_tview_stats_untracked",
    ] {
        assert!(!rendered.contains(&format!("{metric}{{")), "{metric}: {rendered}");
    }
}
