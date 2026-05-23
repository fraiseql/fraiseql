# FraiseQL .phases/

Active sprint: [2026-05-20-sprint/](2026-05-20-sprint/)

## Archive

Completed campaigns moved to [_archive/](_archive/).

## Deferred (open campaigns not in current sprint)

- [federation-docs-170/](federation-docs-170/) — docs follow-up to PR #276 WASM host bridge (issue #170)
- [freebsd-148/](freebsd-148/) — FreeBSD support (issue #148)

## Convention

- `.phases/<date>-sprint/` — active sprint dirs (one per planning cycle)
- `.phases/_archive/<campaign>/` — completed campaigns; never modified after archive
- Each campaign subdir has its own README + phase files

See `~/.claude/CLAUDE.md` for the phased-TDD methodology this directory implements.

Note: this directory is gitignored. The plans live locally only; only the code they produce ends up in git history.
