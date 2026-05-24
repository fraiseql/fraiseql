# FraiseQL Wave-2 follow-ups

Items deferred from a previous wave because they grew larger than the wave's
scope budget. Each entry pins (a) the original finding, (b) the reason it was
deferred, (c) the suggested wave-2 approach.

## F028 — Propagate `ViewName` through public API boundaries

**Deferred from:** Wave 7 (commit 4bf9a58b1)

**Reason deferred:** the Wave-7 commit landed the `ViewName(Arc<str>)`
newtype in `fraiseql-db` and migrated the cache **internal storage** to it
(`accessed_views: Box<[ViewName]>`, `view_index/list_index: DashMap<ViewName,
…>`). This gave us the F037 allocation win for free. However, the public
`invalidate_views(&[String])` signature was kept and the ~30 in-tree test/
bench sites that pass `vec!["v_user".to_string()]` were not migrated.

**Suggested follow-up:** flip `QueryResultCache::invalidate_views` and
`ResponseCache::invalidate_views` to `&[ViewName]` (or `&[impl AsRef<str>]`).
Migrate `CachedDatabaseAdapter::invalidate_views`, the cascade invalidator,
the admin route, and the bench/test files. Run-of-the-mill mechanical
rewrite, but ~30 call sites and the spec budget was 80.
