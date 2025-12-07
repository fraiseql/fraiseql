# Work Package: Fix Example Application READMEs

**Package ID:** WP-006
**Assignee Role:** Technical Writer - API/Examples (TW-API)
**Priority:** P0 - Critical
**Estimated Hours:** 4 hours
**Dependencies:** WP-001

---

## Objective

Fix contradiction where example READMEs use old naming (`users`, `posts`) but actual SQL files use trinity pattern (`tb_user`, `tb_post`).

---

## Files to Update

1. **`examples/blog_simple/README.md`** (lines 80-129)
   - Update schema documentation to use `tb_user`, `tb_post`, `tb_comment`
   - Add section explaining trinity pattern
   - Ensure README matches actual `db/setup.sql`

2. **`examples/mutations_demo/README.md`** (line 72)
   - Replace `users` → `tb_user`

---

## Acceptance Criteria

- [ ] READMEs match SQL files (no contradictions)
- [ ] Trinity pattern explained in context
- [ ] Links to `docs/core/trinity-pattern.md`
- [ ] No confusion between documentation and code

---

**Deadline:** End of Week 1
