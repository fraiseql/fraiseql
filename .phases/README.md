# Seed ETL Fix

## Objective
Fix the seed data ETL pipeline issues causing incorrect maintenance stats and weekend volumes.

## Root Causes
- Phase 01 (core): `fk_printoptim_field` is lost during non-daily maintenance stats rollup.
  `populate_maintenance_stats_non_daily()` groups without it; `merge_to_tenant_statistics()`
  writes NULL explicitly. All 6 non-daily staging tables are missing `fk_printoptim_field`.
- Phase 02: Weekend volumes not zeroed in `scripts/generate_frontend_seed_data.py`.
- Phases 03–05: Regenerate seed files, validate, finalize + ship.

## Phases

| Phase | File | Goal | Status |
|-------|------|------|--------|
| 1 | [phase-01-core-etl-fix.md](phase-01-core-etl-fix.md) | Fix fk_printoptim_field loss in non-daily maintenance stats rollup | [ ] Not Started |
| 2 | [phase-02-weekend-volumes-fix.md](phase-02-weekend-volumes-fix.md) | Zero weekend volumes in generate_frontend_seed_data.py | [ ] Not Started |
| 3 | [phase-03-regenerate-seed.md](phase-03-regenerate-seed.md) | Regenerate seed files after fixes | [ ] Not Started |
| 4 | [phase-04-validate-seed.md](phase-04-validate-seed.md) | Validate regenerated seed data correctness | [ ] Not Started |
| 5 | [phase-05-finalize-ship.md](phase-05-finalize-ship.md) | Finalize and ship the fixes | [ ] Not Started |

## Success Criteria
- All maintenance stats rollups preserve fk_printoptim_field
- Weekend volumes are correctly zeroed in seed data
- Seed files are regenerated and validated
- No regressions in existing functionality</content>
<parameter name="filePath">.phases/README.md