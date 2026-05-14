//! Tests for storage backend list operations.

#[cfg(test)]
mod backend_tests {
    use crate::backend::LocalBackend;
    use tempfile::TempDir;

    /// Helper to create a LocalBackend backed by a temp directory.
    fn temp_backend() -> (LocalBackend, TempDir) {
        let tmpdir = TempDir::new().expect("create tempdir");
        let backend = LocalBackend::new(tmpdir.path().to_str().unwrap());
        (backend, tmpdir)
    }

    #[tokio::test]
    async fn test_list_empty_prefix() {
        let (backend, _tmpdir) = temp_backend();
        let result = backend.list("", None, 100).await.expect("list succeeds");
        assert!(result.objects.is_empty(), "empty backend should have no objects");
        assert!(result.next_cursor.is_none(), "empty result should have no cursor");
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let (backend, _tmpdir) = temp_backend();

        // Upload files: avatars/a.jpg, avatars/b.jpg, documents/doc.pdf
        backend
            .upload("avatars/a.jpg", b"jpeg data a", "image/jpeg")
            .await
            .expect("upload a");
        backend
            .upload("avatars/b.jpg", b"jpeg data b", "image/jpeg")
            .await
            .expect("upload b");
        backend
            .upload("documents/doc.pdf", b"pdf data", "application/pdf")
            .await
            .expect("upload c");

        // List with "avatars/" prefix should return 2 files
        let result = backend
            .list("avatars/", None, 100)
            .await
            .expect("list avatars");
        assert_eq!(result.objects.len(), 2, "should match 2 files under avatars/");
        assert!(
            result
                .objects
                .iter()
                .any(|o| o.key == "avatars/a.jpg"),
            "should include avatars/a.jpg"
        );
        assert!(
            result
                .objects
                .iter()
                .any(|o| o.key == "avatars/b.jpg"),
            "should include avatars/b.jpg"
        );
        assert!(result.next_cursor.is_none(), "all results fit in one page");
    }

    #[tokio::test]
    async fn test_list_cursor_pagination() {
        let (backend, _tmpdir) = temp_backend();

        // Upload 5 files
        for i in 0..5 {
            let key = format!("file{}.txt", i);
            backend
                .upload(&key, b"data", "text/plain")
                .await
                .expect("upload");
        }

        // First page: limit=2
        let page1 = backend
            .list("", None, 2)
            .await
            .expect("first page");
        assert_eq!(page1.objects.len(), 2, "first page should have 2 items");
        let cursor1 = page1.next_cursor.expect("should have next cursor");

        // Second page using cursor
        let page2 = backend
            .list("", Some(&cursor1), 2)
            .await
            .expect("second page");
        assert_eq!(page2.objects.len(), 2, "second page should have 2 items");
        assert!(
            page1.objects[1].key != page2.objects[0].key,
            "pages should not overlap"
        );

        // Third page should have last item and no cursor
        let cursor2 = page2.next_cursor.expect("should have cursor for page 3");
        let page3 = backend
            .list("", Some(&cursor2), 2)
            .await
            .expect("third page");
        assert_eq!(page3.objects.len(), 1, "third page should have 1 item");
        assert!(page3.next_cursor.is_none(), "last page should have no cursor");
    }

    #[tokio::test]
    async fn test_list_no_matching_prefix() {
        let (backend, _tmpdir) = temp_backend();

        // Upload some files
        backend
            .upload("foo/bar.txt", b"data", "text/plain")
            .await
            .expect("upload");

        // List with non-matching prefix
        let result = backend
            .list("nonexistent/", None, 100)
            .await
            .expect("list returns success for missing prefix");
        assert!(
            result.objects.is_empty(),
            "non-matching prefix should return empty list"
        );
        assert!(result.next_cursor.is_none(), "empty result should have no cursor");
    }

    #[tokio::test]
    async fn test_list_object_info_fields() {
        let (backend, _tmpdir) = temp_backend();

        backend
            .upload("test.txt", b"hello world", "text/plain")
            .await
            .expect("upload");

        let result = backend
            .list("", None, 100)
            .await
            .expect("list");
        assert_eq!(result.objects.len(), 1);

        let obj = &result.objects[0];
        assert_eq!(obj.key, "test.txt");
        assert_eq!(obj.size, 11, "size should match data length");
        // LocalBackend defaults to application/octet-stream since filesystem doesn't store content-type
        assert_eq!(obj.content_type, "application/octet-stream");
        assert!(!obj.etag.is_empty(), "etag should be populated");
        assert!(!obj.last_modified.is_empty(), "last_modified should be populated");
    }
}
