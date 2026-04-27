//! Tests for S3Backend.
//!
//! Note: These tests require a running S3-compatible service (S3, MinIO, etc.).
//! To run with MinIO locally:
//!
//! ```bash
//! docker run -d -p 9000:9000 -p 9001:9001 minio/minio server /data
//! export AWS_ACCESS_KEY_ID=minioadmin
//! export AWS_SECRET_ACCESS_KEY=minioadmin
//! cargo test -p fraiseql-storage --lib --features aws-s3 s3::tests
//! ```

use crate::backend::S3Backend;

/// Helper to skip tests if S3 service is not configured.
fn skip_if_no_s3() -> Option<()> {
    if std::env::var("S3_ENDPOINT").is_err() && std::env::var("AWS_ENDPOINT_URL").is_err() {
        return None;
    }
    Some(())
}

/// Helper to create an S3Backend for testing.
fn create_test_backend() -> S3Backend {
    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        .ok();

    // Use a unique bucket name for tests to avoid conflicts
    let bucket = format!("test-{}", uuid::Uuid::new_v4());

    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    rt.block_on(async { S3Backend::new(&bucket, None, endpoint.as_deref()).await })
}

#[test]
fn test_s3_backend_struct_creation() {
    // This test just verifies that the S3Backend struct can be created
    // It doesn't require external services
    let _backend = create_test_backend();
    // If we get here, the test passed (no panic)
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_put_and_get_roundtrip() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let key = "test-file.txt";
        let data = b"Hello, S3 world!";
        let content_type = "text/plain";

        // Upload test data
        let result = backend.upload(key, data, content_type).await;
        assert!(result.is_ok(), "upload should succeed");
        assert_eq!(result.unwrap(), key);

        // Download and verify
        let downloaded = backend.download(key).await;
        assert!(downloaded.is_ok(), "download should succeed");
        assert_eq!(downloaded.unwrap(), data);
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_get_nonexistent_returns_not_found() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let result = backend.download("nonexistent-key.txt").await;
        assert!(result.is_err(), "download of nonexistent key should fail");

        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("not found") || err_msg.contains("404"),
            "error should indicate not found: {}",
            err_msg
        );
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_delete_removes_object() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let key = "to-delete.txt";
        let data = b"temporary file";

        // Upload
        backend
            .upload(key, data, "text/plain")
            .await
            .expect("upload succeeds");

        // Verify exists
        let exists = backend.exists(key).await.expect("exists check succeeds");
        assert!(exists, "file should exist after upload");

        // Delete
        let delete_result = backend.delete(key).await;
        assert!(delete_result.is_ok(), "delete should succeed");

        // Verify deleted
        let exists_after = backend.exists(key).await.expect("exists check succeeds");
        assert!(!exists_after, "file should not exist after delete");
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_list_with_prefix() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        // Upload files with different prefixes
        backend
            .upload("avatars/user1.jpg", b"avatar 1", "image/jpeg")
            .await
            .expect("upload 1");
        backend
            .upload("avatars/user2.jpg", b"avatar 2", "image/jpeg")
            .await
            .expect("upload 2");
        backend
            .upload("documents/doc.pdf", b"pdf content", "application/pdf")
            .await
            .expect("upload 3");

        // List with "avatars/" prefix
        let result = backend.list("avatars/", None, 100).await.expect("list succeeds");

        assert_eq!(result.objects.len(), 2, "should have 2 items under avatars/");
        assert!(
            result.objects.iter().any(|o| o.key == "avatars/user1.jpg"),
            "should include avatars/user1.jpg"
        );
        assert!(
            result.objects.iter().any(|o| o.key == "avatars/user2.jpg"),
            "should include avatars/user2.jpg"
        );
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_list_pagination() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        // Upload 5 objects
        for i in 0..5 {
            let key = format!("file{:02}.txt", i);
            backend
                .upload(&key, format!("data {}", i).as_bytes(), "text/plain")
                .await
                .expect("upload succeeds");
        }

        // First page with limit=2
        let page1 = backend
            .list("", None, 2)
            .await
            .expect("list page 1 succeeds");
        assert_eq!(page1.objects.len(), 2, "first page should have 2 items");

        let cursor1 = page1.next_cursor.expect("first page should have cursor");

        // Second page using cursor
        let page2 = backend
            .list("", Some(&cursor1), 2)
            .await
            .expect("list page 2 succeeds");
        assert_eq!(page2.objects.len(), 2, "second page should have 2 items");

        // Verify pages don't overlap
        assert_ne!(
            page1.objects[0].key, page2.objects[0].key,
            "pages should have different objects"
        );

        // Third page (remaining items)
        let cursor2 = page2.next_cursor.expect("second page should have cursor");
        let page3 = backend
            .list("", Some(&cursor2), 2)
            .await
            .expect("list page 3 succeeds");
        assert_eq!(page3.objects.len(), 1, "third page should have 1 item");
        assert!(page3.next_cursor.is_none(), "last page should have no cursor");
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_exists_true_and_false() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let key = "existence-test.txt";

        // Before upload, should not exist
        let exists_before = backend
            .exists(key)
            .await
            .expect("exists check succeeds");
        assert!(!exists_before, "should not exist before upload");

        // Upload
        backend
            .upload(key, b"test", "text/plain")
            .await
            .expect("upload succeeds");

        // After upload, should exist
        let exists_after = backend
            .exists(key)
            .await
            .expect("exists check succeeds");
        assert!(exists_after, "should exist after upload");

        // Non-existent key should return false, not error
        let not_exist = backend
            .exists("definitely-does-not-exist.txt")
            .await
            .expect("exists check on non-existent key should not error");
        assert!(!not_exist, "non-existent key should return false");
    });
}

#[test]
#[ignore] // Requires MinIO to be running
fn test_s3_large_object_streaming() {
    let Some(()) = skip_if_no_s3() else {
        return;
    };

    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        // Create 10MB of data to test streaming
        let large_data = vec![42u8; 10 * 1024 * 1024];
        let key = "large-file.bin";

        // Upload large object
        let upload_result = backend
            .upload(key, &large_data, "application/octet-stream")
            .await;
        assert!(upload_result.is_ok(), "large upload should succeed");

        // Download and verify size
        let downloaded = backend
            .download(key)
            .await
            .expect("large download should succeed");
        assert_eq!(
            downloaded.len(),
            large_data.len(),
            "downloaded size should match uploaded size"
        );
        assert_eq!(
            downloaded, large_data,
            "downloaded content should match uploaded content"
        );
    });
}

#[test]
fn test_s3_key_validation() {
    let backend = create_test_backend();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        // Test empty key
        let result = backend.upload("", b"data", "text/plain").await;
        assert!(result.is_err(), "empty key should be rejected");

        // Test path traversal
        let result = backend.upload("../etc/passwd", b"data", "text/plain").await;
        assert!(result.is_err(), "path traversal should be rejected");

        // Test absolute path
        let result = backend.upload("/etc/passwd", b"data", "text/plain").await;
        assert!(result.is_err(), "absolute path should be rejected");
    });
}
