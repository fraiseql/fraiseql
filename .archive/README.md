# FraiseQL Archive

This directory contains archived, deprecated, and experimental code from FraiseQL's development history.

## Purpose

The `.archive/` directory maintains historical context while keeping the main repository clean and focused on current development. Code here is not actively maintained or tested.

## Directory Structure

### `/phases/` - Development Phase Documentation
Historical development phases from earlier versions. These documents track planning and implementation decisions but are not part of the current codebase.

**Status**: Reference only - do not execute
**Maintained**: No
**Use Case**: Historical context, decision rationale

### `/deprecated/` - Deprecated Features
Features that were removed or replaced in later versions. Code here may be useful as reference for understanding historical patterns.

**Status**: Not functional - reference only
**Maintained**: No
**Use Case**: Understanding removed features, implementation patterns

### `/experimental/` - Experimental Code
Parallel implementations, prototypes, and experimental features that were not merged into main codebase.

**Status**: Proof-of-concept only
**Maintained**: No
**Use Case**: Ideas for future development, alternative approaches

## Before Using Archived Code

⚠️ **Warning**: Code in this directory is:
- **Not tested** - May not run without modifications
- **Not maintained** - Dependencies may be outdated
- **Not up-to-date** - Does not reflect current architecture
- **Not supported** - Issues/PRs related to archived code will be closed

## Migration Guidelines

If you need to revive code from this archive:

1. **Understand the context**: Read related documentation to understand why code was archived
2. **Check compatibility**: Verify against current codebase structure and dependencies
3. **Write tests**: Add comprehensive tests before merging
4. **Update dependencies**: Ensure all imports and external dependencies are current
5. **Get review**: Archive resurrection requires additional review

## Archive Policy

### When Code Gets Archived

- Features marked as "deprecated" for 2+ minor versions
- Experimental features that didn't reach stability
- Development phases that are complete
- Alternative implementations (e.g., unused HTTP servers)

### How to Remove Archived Code

After 6+ months in archive with no usage:
1. Create an issue documenting the removal
2. Remove from archive in a separate commit
3. Update CHANGELOG with removal notice

## Questions?

See the main project README for current development practices.

---

**Last Updated**: January 8, 2026
**Archive Version**: v2.0 Preparation
