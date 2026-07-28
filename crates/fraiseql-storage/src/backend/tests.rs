//! Tests for storage backend list operations.

#[cfg(test)]
mod backend_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::indexing_slicing)] // Reason: test fixtures index into known-shape collections; OOB indices correctly fail the test
    #![allow(missing_docs)] // Reason: test functions are self-describing

    use tempfile::TempDir;

    use crate::backend::LocalBackend;

    /// Helper to create a `LocalBackend` backed by a temp directory.
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
        let result = backend.list("avatars/", None, 100).await.expect("list avatars");
        assert_eq!(result.objects.len(), 2, "should match 2 files under avatars/");
        assert!(
            result.objects.iter().any(|o| o.key == "avatars/a.jpg"),
            "should include avatars/a.jpg"
        );
        assert!(
            result.objects.iter().any(|o| o.key == "avatars/b.jpg"),
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
            backend.upload(&key, b"data", "text/plain").await.expect("upload");
        }

        // First page: limit=2
        let page1 = backend.list("", None, 2).await.expect("first page");
        assert_eq!(page1.objects.len(), 2, "first page should have 2 items");
        let cursor1 = page1.next_cursor.expect("should have next cursor");

        // Second page using cursor
        let page2 = backend.list("", Some(&cursor1), 2).await.expect("second page");
        assert_eq!(page2.objects.len(), 2, "second page should have 2 items");
        assert!(page1.objects[1].key != page2.objects[0].key, "pages should not overlap");

        // Third page should have last item and no cursor
        let cursor2 = page2.next_cursor.expect("should have cursor for page 3");
        let page3 = backend.list("", Some(&cursor2), 2).await.expect("third page");
        assert_eq!(page3.objects.len(), 1, "third page should have 1 item");
        assert!(page3.next_cursor.is_none(), "last page should have no cursor");
    }

    #[tokio::test]
    async fn test_list_no_matching_prefix() {
        let (backend, _tmpdir) = temp_backend();

        // Upload some files
        backend.upload("foo/bar.txt", b"data", "text/plain").await.expect("upload");

        // List with non-matching prefix
        let result = backend
            .list("nonexistent/", None, 100)
            .await
            .expect("list returns success for missing prefix");
        assert!(result.objects.is_empty(), "non-matching prefix should return empty list");
        assert!(result.next_cursor.is_none(), "empty result should have no cursor");
    }

    #[tokio::test]
    async fn test_list_object_info_fields() {
        let (backend, _tmpdir) = temp_backend();

        backend.upload("test.txt", b"hello world", "text/plain").await.expect("upload");

        let result = backend.list("", None, 100).await.expect("list");
        assert_eq!(result.objects.len(), 1);

        let obj = &result.objects[0];
        assert_eq!(obj.key, "test.txt");
        assert_eq!(obj.size, 11, "size should match data length");
        // LocalBackend defaults to application/octet-stream since filesystem doesn't store
        // content-type
        assert_eq!(obj.content_type, "application/octet-stream");
        assert!(!obj.etag.is_empty(), "etag should be populated");
        assert!(!obj.last_modified.is_empty(), "last_modified should be populated");
    }
}

/// Key-space safety: distinct keys must never resolve to the same object.
///
/// #813: `validate_key` rejected only `..` and a leading separator, so `a/./b`,
/// `a//b` and `a/b/` were three distinct metadata keys that the local backend
/// resolved to one file. Per-object ownership is keyed on the metadata string,
/// so an alias let the overwrite gate see `existing == None` and take the
/// any-authenticated-user create branch over another owner's bytes.
#[cfg(test)]
mod key_validation {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(missing_docs)] // Reason: test functions are self-describing

    use std::collections::HashMap;

    use tempfile::TempDir;

    use crate::backend::{LocalBackend, validate_key};

    /// Every spelling that aliases onto the plain key `docs/secret.txt`.
    const ALIASES_OF_DOCS_SECRET: &[&str] = &[
        "docs/./secret.txt", // single-dot segment
        "docs//secret.txt",  // empty segment
        "./docs/secret.txt", // leading dot segment
        "docs/a/../secret.txt", /* traversal that lands back inside — already rejected, kept
                              * for coverage */
    ];

    #[test]
    fn rejects_every_alias_of_a_plain_key() {
        for alias in ALIASES_OF_DOCS_SECRET {
            assert!(
                validate_key(alias).is_err(),
                "{alias:?} resolves to the same file as \"docs/secret.txt\" and must be rejected"
            );
        }
    }

    #[test]
    fn rejects_trailing_and_leading_separators() {
        for key in ["docs/secret.txt/", "/docs/secret.txt", "docs/secret.txt//"] {
            assert!(validate_key(key).is_err(), "{key:?} must be rejected");
        }
    }

    #[test]
    fn rejects_encoded_traversal() {
        // axum percent-decodes once, so these arrive literally only via double
        // encoding — but any backend that decodes the key again (Azure builds a
        // URL out of it) would turn them back into path syntax.
        for key in [
            "docs/%2e/secret.txt",
            "docs/%2E%2E/secret.txt",
            "docs%2Fsecret.txt",
        ] {
            assert!(validate_key(key).is_err(), "{key:?} encodes path syntax and must be rejected");
        }
    }

    #[test]
    fn rejects_control_characters_and_nul() {
        for key in [
            "docs/sec\0ret.txt",
            "docs/sec\nret.txt",
            "docs/sec\u{7f}ret.txt",
        ] {
            assert!(
                validate_key(key).is_err(),
                "{key:?} contains a control byte and must be rejected"
            );
        }
    }

    #[test]
    fn rejects_backslash_anywhere() {
        for key in ["docs\\secret.txt", "\\docs/secret.txt", "docs/sec\\ret.txt"] {
            assert!(validate_key(key).is_err(), "{key:?} must be rejected");
        }
    }

    #[test]
    fn rejects_segments_that_collapse_on_a_case_insensitive_or_trailing_dot_filesystem() {
        // A segment with a trailing dot or surrounding whitespace names the same
        // file as the trimmed segment on Windows and on some network filesystems.
        for key in ["docs/secret.txt.", "docs /secret.txt", "docs/ secret.txt"] {
            assert!(validate_key(key).is_err(), "{key:?} must be rejected");
        }
    }

    #[test]
    fn accepts_ordinary_keys() {
        for key in [
            "file.txt",
            "path/to/file.txt",
            "uploads/2024/01/image.png",
            "invoices/ACME-2026-001.pdf",
            "a.b.c/d-e_f/g h.txt",
        ] {
            assert!(validate_key(key).is_ok(), "{key:?} is an ordinary key and must be accepted");
        }
    }

    /// Bounded exhaustive proof of the property the aliasing bug violated:
    /// **key → resolved path is injective**. Two accepted keys never name one file.
    #[test]
    fn accepted_keys_map_injectively_onto_paths() {
        const SEGMENTS: &[&str] = &[
            "a", "b", ".", "..", "", "a.", " a", "a ", "%2e", "%2f", "a\\b",
        ];

        let tmp = TempDir::new().unwrap();
        let backend = LocalBackend::new(tmp.path().to_str().unwrap());

        let mut seen: HashMap<std::path::PathBuf, String> = HashMap::new();
        let mut accepted = 0_usize;

        // All 1-, 2- and 3-segment keys over the alphabet above.
        let mut keys: Vec<String> = SEGMENTS.iter().map(|s| (*s).to_owned()).collect();
        for a in SEGMENTS {
            for b in SEGMENTS {
                keys.push(format!("{a}/{b}"));
                for c in SEGMENTS {
                    keys.push(format!("{a}/{b}/{c}"));
                }
            }
        }

        for key in &keys {
            if validate_key(key).is_err() {
                continue;
            }
            accepted += 1;
            let path = backend.key_path(key).expect("an accepted key must resolve to a path");
            let collision = seen.insert(path.clone(), key.clone());
            assert!(
                collision.is_none(),
                "keys {collision:?} and {key:?} both resolve to {path:?} — key→path is not \
                 injective"
            );
        }

        assert!(
            accepted > 0,
            "the generator must accept at least some keys, or it proves nothing"
        );
    }
}

/// Filesystem containment: a key that validates must still not reach outside
/// the backend root through a symlink planted inside it.
///
/// `validate_key` makes the lexical join safe; it cannot see the filesystem.
/// #813's suggested fix asks for the resolved path to be re-checked against the
/// root before any I/O, which is what these cover.
#[cfg(test)]
mod local_containment {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(missing_docs)] // Reason: test functions are self-describing

    use tempfile::TempDir;

    use crate::backend::LocalBackend;

    /// A backend root plus an "outside" sibling directory holding a victim file.
    ///
    /// Returns the guard last so the temp dir outlives the test.
    struct Rig {
        backend: LocalBackend,
        root:    std::path::PathBuf,
        outside: std::path::PathBuf,
        _guard:  TempDir,
    }

    fn rig() -> Rig {
        let guard = TempDir::new().unwrap();
        let root = guard.path().join("root");
        let outside = guard.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("victim.txt"), b"VICTIM").unwrap();
        let backend = LocalBackend::new(root.to_str().unwrap());
        Rig {
            backend,
            root,
            outside,
            _guard: guard,
        }
    }

    #[tokio::test]
    async fn upload_through_a_symlinked_directory_is_refused() {
        let rig = rig();
        // An operator (or an earlier upload) leaves a directory symlink in the root.
        std::os::unix::fs::symlink(&rig.outside, rig.root.join("link")).unwrap();

        let err = rig
            .backend
            .upload("link/victim.txt", b"PWNED", "text/plain")
            .await
            .expect_err("a write that resolves outside the root must be refused");
        assert!(err.to_string().contains("outside"), "unexpected error: {err}");

        assert_eq!(
            std::fs::read(rig.outside.join("victim.txt")).unwrap(),
            b"VICTIM",
            "the file outside the root must be untouched"
        );
    }

    #[tokio::test]
    async fn upload_through_a_symlinked_leaf_is_refused() {
        let rig = rig();
        std::os::unix::fs::symlink(rig.outside.join("victim.txt"), rig.root.join("victim.txt"))
            .unwrap();

        let err = rig
            .backend
            .upload("victim.txt", b"PWNED", "text/plain")
            .await
            .expect_err("a write through a symlinked leaf must be refused");
        assert!(err.to_string().contains("symlink"), "unexpected error: {err}");

        assert_eq!(
            std::fs::read(rig.outside.join("victim.txt")).unwrap(),
            b"VICTIM",
            "the file outside the root must be untouched"
        );
    }

    #[tokio::test]
    async fn download_through_a_symlink_is_refused() {
        let rig = rig();
        std::os::unix::fs::symlink(&rig.outside, rig.root.join("link")).unwrap();

        let err = rig
            .backend
            .download("link/victim.txt")
            .await
            .expect_err("a read that resolves outside the root must be refused");
        assert!(err.to_string().contains("outside"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn ordinary_nested_uploads_and_downloads_still_work() {
        let rig = rig();
        rig.backend.upload("a/b/c.txt", b"hello", "text/plain").await.unwrap();
        assert_eq!(rig.backend.download("a/b/c.txt").await.unwrap(), b"hello");
        assert!(rig.backend.exists("a/b/c.txt").await.unwrap());
        rig.backend.delete("a/b/c.txt").await.unwrap();
        assert!(!rig.backend.exists("a/b/c.txt").await.unwrap());
    }
}

/// The listing path joins a caller-supplied prefix rather than a key, so it
/// needs its own containment check.
#[cfg(test)]
mod local_list_containment {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(missing_docs)] // Reason: test functions are self-describing

    use tempfile::TempDir;

    use crate::backend::LocalBackend;

    #[tokio::test]
    async fn list_cannot_escape_the_root_via_prefix_or_symlink() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("victim.txt"), b"VICTIM").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let backend = LocalBackend::new(root.to_str().unwrap());

        assert!(
            backend.list("../outside", None, 100).await.is_err(),
            "a '..' prefix must not list outside the root"
        );
        assert!(
            backend.list("link", None, 100).await.is_err(),
            "a symlinked directory must not list outside the root"
        );
    }

    #[tokio::test]
    async fn exists_reports_false_rather_than_erroring_for_a_missing_parent() {
        let tmp = TempDir::new().unwrap();
        let backend = LocalBackend::new(tmp.path().to_str().unwrap());
        assert!(
            !backend.exists("never/created/f.txt").await.expect("must not error"),
            "a key under a directory that was never created simply does not exist"
        );
    }
}
