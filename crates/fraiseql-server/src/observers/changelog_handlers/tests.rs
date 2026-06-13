use super::*;

#[test]
fn changelog_entry_response_serializes() {
    let entry = ChangelogEntryResponse {
        cursor:            42,
        id:                "550e8400-e29b-41d4-a716-446655440000".to_string(),
        org_id:            Some("acme".to_string()),
        user_id:           None,
        object_type:       "Order".to_string(),
        object_id:         "123".to_string(),
        modification_type: "INSERT".to_string(),
        status:            None,
        object_data:       serde_json::json!({"op": "c", "after": {"id": 1}}),
        metadata:          None,
        created_at:        None,
    };
    let json = serde_json::to_value(&entry).expect("serialize");
    assert_eq!(json["cursor"], 42);
    assert_eq!(json["object_type"], "Order");
}

#[test]
fn changelog_list_response_serializes() {
    let response = ChangelogListResponse {
        entries:     vec![],
        next_cursor: None,
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert!(json["next_cursor"].is_null());
    assert_eq!(json["entries"].as_array().expect("entries array").len(), 0);
}

#[test]
fn checkpoint_response_serializes() {
    let response = CheckpointResponse {
        listener_id: "my_app".to_string(),
        last_cursor: 100,
        updated_at:  Some("2026-01-01T00:00:00Z".to_string()),
    };
    let json = serde_json::to_value(&response).expect("serialize");
    assert_eq!(json["last_cursor"], 100);
}

#[test]
fn save_checkpoint_request_deserializes() {
    let json = r#"{"last_cursor": 42}"#;
    let req: SaveCheckpointRequest = serde_json::from_str(json).expect("deserialize");
    assert_eq!(req.last_cursor, 42);
}

#[test]
fn default_changelog_limit_is_100() {
    assert_eq!(default_changelog_limit(), 100);
}

#[test]
fn max_limit_is_1000() {
    assert_eq!(MAX_CHANGELOG_LIMIT, 1_000);
}

// ── H28: tail query ───────────────────────────────────────────────────────────

#[test]
fn changelog_query_latest_flag_parses_and_defaults_false() {
    let q: ChangelogQuery =
        serde_json::from_value(serde_json::json!({ "object_type": null, "latest": true }))
            .expect("deserialize latest=true");
    assert!(q.latest);

    let q: ChangelogQuery = serde_json::from_value(serde_json::json!({ "object_type": null }))
        .expect("deserialize without latest");
    assert!(!q.latest, "latest must default to false");
}

/// Tail query against a seeded changelog: `latest=true` returns exactly the
/// newest entry and echoes its cursor as `next_cursor`, so a consumer can
/// checkpoint at the real tail. Skips when `DATABASE_URL` is unset.
///
/// The tail-ordering contract (`ORDER BY pk DESC LIMIT 1`) is verified directly
/// so the test is independent of the `object_id` column type. `fetch_changelog`
/// is additionally exercised end-to-end when this database uses the REST
/// changelog contract (text `object_id`); deployments that use the observer
/// runtime's UUID `object_id` contract skip that leg (the handler decodes
/// `object_id` as text).
#[tokio::test]
async fn latest_returns_newest_cursor_against_seeded_changelog() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return; // no database configured — runs in the integration leg
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("connect");
    let obj = "H28TailTest";

    sqlx::query("DELETE FROM core.tb_entity_change_log WHERE object_type = $1")
        .bind(obj)
        .execute(&pool)
        .await
        .expect("clean");
    for _ in 0..3 {
        sqlx::query(
            "INSERT INTO core.tb_entity_change_log \
                 (object_type, modification_type, object_id, object_data) \
             VALUES ($1, 'INSERT', gen_random_uuid(), '{}'::jsonb)",
        )
        .bind(obj)
        .execute(&pool)
        .await
        .expect("insert");
    }
    let max: (i64,) = sqlx::query_as(
        "SELECT MAX(pk_entity_change_log) FROM core.tb_entity_change_log \
                        WHERE object_type = $1",
    )
    .bind(obj)
    .fetch_one(&pool)
    .await
    .expect("max");

    // Tail-ordering contract, independent of the object_id column type.
    let tail: (i64,) = sqlx::query_as(
        "SELECT pk_entity_change_log FROM core.tb_entity_change_log \
         WHERE object_type = $1 ORDER BY pk_entity_change_log DESC LIMIT 1",
    )
    .bind(obj)
    .fetch_one(&pool)
    .await
    .expect("tail");
    assert_eq!(tail.0, max.0, "tail query returns the newest cursor");

    // End-to-end through fetch_changelog when object_id is the REST text type.
    let object_id_type: (String,) = sqlx::query_as(
        "SELECT data_type FROM information_schema.columns \
         WHERE table_schema = 'core' AND table_name = 'tb_entity_change_log' \
           AND column_name = 'object_id'",
    )
    .fetch_one(&pool)
    .await
    .expect("probe object_id type");
    if object_id_type.0 == "text" {
        let query = ChangelogQuery {
            after_cursor: 0,
            limit:        100,
            object_type:  Some(obj.to_string()),
            latest:       true,
        };
        let resp = fetch_changelog(&pool, &query).await.expect("fetch_changelog");
        assert_eq!(resp.entries.len(), 1, "latest returns exactly one entry");
        assert_eq!(resp.next_cursor, Some(max.0), "tail cursor is the newest pk");
        assert_eq!(resp.entries[0].cursor, max.0);
    }

    sqlx::query("DELETE FROM core.tb_entity_change_log WHERE object_type = $1")
        .bind(obj)
        .execute(&pool)
        .await
        .expect("cleanup");
}
