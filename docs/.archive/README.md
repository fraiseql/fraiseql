# Documentation Archive

This directory contains historical and future-phase documentation that is not part of the current v2.0.0 release.

## Completed Phases

Documentation for completed development phases (preserved for historical reference):

### Phase 5: Performance Analysis

- `phases/PHASE_5_PERFORMANCE_ANALYSIS.md` — Performance analysis work

### Phase 6: Dashboards and Monitoring

- `phases/PHASE_6_DASHBOARDS_AND_MONITORING.md` — Dashboard configuration

### Phase 7: End-to-End Integration

- `phases/PHASE_7_END_TO_END_INTEGRATION.md` — Integration testing

## Future Phases

Planning documentation for future work (not part of current roadmap):

### Future Phases 8+

- `future-phases/PHASE_16_READINESS.md` — Old phase planning (references deprecated phases 15-16)
- `future-phases/MIGRATION_PHASE_15_TO_16.md` — Old migration guide
- `future-phases/PHASE_8_6_JOB_QUEUE.md` — Job queue feature planning
- `future-phases/PHASE_8_7_METRICS.md` — Metrics feature planning

## Deprecated Features

Documentation for features that were planned but not implemented in v2.0.0:

### Endpoint Runtime (Future Sub-project)

- `deprecated/ENDPOINT_RUNTIME_ARCHIVE.md` — Archived Endpoint Runtime planning
- `deprecated/endpoint-runtime-archive-20260201.tar.gz` — Full Endpoint Runtime documentation archive

### Consolidated/Deduplicated
Files consolidated into main documentation structure:

- `deprecated/PERFORMANCE_CONSOLIDATED.md` — Was `PERFORMANCE.md` (consolidated with `performance/README.md`)
- `deprecated/PERFORMANCE_MONITORING_CONSOLIDATED.md` — Was `PERFORMANCE_MONITORING.md` (consolidated)
- `deprecated/DEPLOYMENT_GUIDE.md` — Was quick-start (merged into main guide)
- `deprecated/OPERATIONS_QUICK_START.md` — Was quick reference (merged into main guide)

## Extracting Archived Content

To review archived documentation:

```bash
# View phase completion reports
ls -la phases/

# View future phase planning
ls -la future-phases/

# Extract endpoint-runtime archive
cd deprecated
tar -xzf endpoint-runtime-archive-20260201.tar.gz
```

---

**Note**: These files are preserved for historical context and future reference. They are not part of the active v2.0.0 documentation structure.

**Archived Date**: February 1, 2026
