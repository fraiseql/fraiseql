//! The time a listing reports, stated as text so the assertions do not depend on the
//! date-time library behind [`ObjectInfo::last_modified`].
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // Reason: test code, panics acceptable

use std::time::{Duration, UNIX_EPOCH};

use tempfile::TempDir;

use super::{local::LocalBackend, types::ObjectInfo};

type Instant = fraiseql_time::Timestamp;

fn instant(text: &str) -> Instant {
    text.parse().expect("an RFC 3339 instant")
}

fn listed(last_modified: Option<Instant>) -> ObjectInfo {
    ObjectInfo {
        key: "a.txt".to_string(),
        size: 5,
        content_type: "text/plain".to_string(),
        etag: "e".to_string(),
        last_modified,
    }
}

/// `last_modified` is written in UTC with a `Z` and its fraction in 3, 6 or 9 digits, and
/// left out when the backend reported no time.
#[test]
fn a_listed_time_is_written_as_before() {
    for (at, written) in [
        (Some("2025-11-20T12:00:00.12Z"), Some("2025-11-20T12:00:00.120Z")),
        (Some("2025-11-20T12:00:00Z"), Some("2025-11-20T12:00:00Z")),
        (Some("2025-11-20T12:00:00.000001Z"), Some("2025-11-20T12:00:00.000001Z")),
        (None, None),
    ] {
        let json = serde_json::to_value(listed(at.map(instant))).unwrap();
        assert_eq!(json.get("last_modified").and_then(|v| v.as_str()), written, "{at:?}");
    }
}

/// A listing read back takes the text chrono's deserializer took: any offset, a space or
/// `t` before the time, `UTC`, surrounding space; `null` or no field is no time.
#[test]
fn a_listed_time_is_read_as_before() {
    for (text, read) in [
        (
            serde_json::json!("2025-11-20 12:00:00.12+02:00"),
            Some("2025-11-20T10:00:00.120Z"),
        ),
        (serde_json::json!("2025-11-20t12:00:00 UTC"), Some("2025-11-20T12:00:00Z")),
        (serde_json::json!(" 2025-11-20T12:00:00+0530 "), Some("2025-11-20T06:30:00Z")),
        (serde_json::Value::Null, None),
    ] {
        let mut json = serde_json::to_value(listed(None)).unwrap();
        json["last_modified"] = text.clone();
        let info: ObjectInfo = serde_json::from_value(json).unwrap();
        let written = serde_json::to_value(&info).unwrap();
        assert_eq!(written.get("last_modified").and_then(|v| v.as_str()), read, "{text}");
    }
    let mut json = serde_json::to_value(listed(None)).unwrap();
    for refused in ["2025-11-20", "2025-11-20T12:00Z", "10000-01-01T00:00:00Z"] {
        json["last_modified"] = serde_json::json!(refused);
        assert!(serde_json::from_value::<ObjectInfo>(json.clone()).is_err(), "{refused}");
    }
}

/// The local listing's etag is the FNV-1a hash of `"{size}-{mtime}"`, the mtime written
/// `2025-11-20T12:00:00.120+00:00`: an etag a client cached still matches.
#[tokio::test]
async fn a_local_etag_hashes_the_mtime_as_written_before() {
    let dir = TempDir::new().unwrap();
    let backend = LocalBackend::new(dir.path().to_str().unwrap());
    backend.upload("a.txt", b"hello", "text/plain").await.unwrap();
    let modified = UNIX_EPOCH + Duration::new(1_763_640_000, 120_000_000);
    std::fs::File::options()
        .write(true)
        .open(dir.path().join("a.txt"))
        .and_then(|file| file.set_modified(modified))
        .unwrap();
    let listing = backend.list("", None, 10).await.unwrap();
    let object = &listing.objects[0];
    assert_eq!(object.etag, "bfa143e79bc27f63");
    assert_eq!(object.last_modified, Some(instant("2025-11-20T12:00:00.120Z")));
}
