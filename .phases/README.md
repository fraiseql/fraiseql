# FraiseQL .phases/

**Active:** [2026-05-31-release-train/](2026-05-31-release-train/) — master
orchestration for everything queued after v2.3.2 (Dagger CI migration as the
prerequisite, then the v2.4.0 tail, deferred-bug waves, and enhancements), laid
out for parallel git-worktree execution. **Start there.**

## Archive

Completed campaigns moved to [_archive/](_archive/).

## Shipped (previously-deferred, now merged to dev)

- [federation-docs-170/](federation-docs-170/) — issue #170 merged via PR #403
- [freebsd-148/](freebsd-148/) — issue #148 merged via PR #402

(Superseded sprint: [2026-05-20-sprint/](2026-05-20-sprint/).)

## Convention

- `.phases/<date>-sprint/` — active sprint dirs (one per planning cycle)
- `.phases/_archive/<campaign>/` — completed campaigns; never modified after archive
- Each campaign subdir has its own README + phase files

See `~/.claude/CLAUDE.md` for the phased-TDD methodology this directory implements.

Note: this directory is gitignored. The plans live locally only; only the code they produce ends up in git history.
