# FraiseQL .phases/

## ⭐ Active program (as of 2026-07-27)

[2026-07-27-open-issue-remediation/](2026-07-27-open-issue-remediation/) — **every one of the
203 open GitHub issues**, assigned to exactly one of 32 phases across 8 waves, targeting
v2.14.2 (security patch) → v2.19.0+. 158 of the 203 came from the three audit passes.
Start at its [README](2026-07-27-open-issue-remediation/README.md); the full mapping is in
[ISSUE-INDEX.md](2026-07-27-open-issue-remediation/ISSUE-INDEX.md) and the ordering
rationale in [TRIAGE.md](2026-07-27-open-issue-remediation/TRIAGE.md). Five user-owned gates
(G1–G5) are listed in the README.

The trains below predate it; their remaining work is folded into the program's Wave 7.

---

**Active train (as of 2026-07-01):**
[v2.11.0-saga-full-roundtrip/](v2.11.0-saga-full-roundtrip/) — closes the round-trip
half of the #429 saga umbrella (compensation → recovery → coordinator →
remote-dispatch → finalize). Phases 01–04 are **merged** (dev `176de7ece`); only
**Phase 05 (Finalize)** remains — it keeps `unstable-saga` gated and adds a
`# Stability` docs section (promotion is deferred to the hardening train below).

**Queued behind it** (all written, `Not Started`):
- [v2.14.0-saga-hardening/](v2.14.0-saga-hardening/) — the remaining saga work that
  v2.11.0 deferred, laid out as 7 phases ending in the `unstable-saga` → stable
  promotion: remote-mutation-name persistence → remote HTTP compensation → `@requires`
  pre-fetch → concurrency-safe recovery (`SKIP LOCKED`) → transport hardening (mTLS) →
  strategies + observability → finalize/promote. (Numbered v2.14.0 as the natural
  successor to the two trains below; re-prioritise earlier if a consumer needs
  distributed sagas in production.)
- [v2.12.0-cdc-and-observer-transports/](v2.12.0-cdc-and-observer-transports/) — Kafka +
  Kinesis sinks (#382) + observer transports + server auto-mount (#428)
- [v2.13.0-ai-native/](v2.13.0-ai-native/) — actor model / vector similarity DSL /
  session-state / MCP resources / Python helpers (renumbered off its dead v2.10.0 label —
  the v2.10.0 slot shipped as the AX cluster #484–#488)

**Shipped since the release-train era:** v2.9.0 (auth foundation + make-it-real cluster +
CDC first slice), v2.10.0 (AX feedback cluster #484–#488). The old
[2026-05-31-release-train/](2026-05-31-release-train/) master orchestration is complete;
its remaining follow-ups are the three plans above.

## Archive

Completed campaigns moved to [_archive/](_archive/).

## Shipped (previously-deferred, now merged to dev)

- [federation-docs-170/](federation-docs-170/) — issue #170 merged via PR #403
- [freebsd-148/](freebsd-148/) — issue #148 merged via PR #402

(Superseded sprint: [2026-05-20-sprint/](2026-05-20-sprint/).)

## Audit handoffs

- [2026-07-25-audit-pass3/](2026-07-25-audit-pass3/) — framework-wide bug-hunting audit.
  Pass 1 → issues #715–#738, pass 2 → #739–#788. **Pass 3 not started**: the next-agent
  prompt, the reusable review-workflow script, and the dedup issue-map are checked in there
  (force-added past the gitignore) so another terminal can carry it on.

## Convention

- `.phases/<date>-sprint/` — active sprint dirs (one per planning cycle)
- `.phases/_archive/<campaign>/` — completed campaigns; never modified after archive
- Each campaign subdir has its own README + phase files

See `~/.claude/CLAUDE.md` for the phased-TDD methodology this directory implements.

Note: this directory is gitignored. The plans live locally only; only the code they produce ends up in git history.
