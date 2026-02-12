# Internal Documentation

This directory contains internal development documentation, planning documents, and AI assistant context that are not part of the public-facing documentation.

## Structure

### claude/
Context and planning documents used by AI assistants (Claude) during development:
- Architecture principles and design documents
- Code review and quality analysis reports
- Clippy fix roadmaps and violation catalogs
- Migration plans (Rust migration strategies)
- Compiler design documents
- Documentation review materials

**Note:** Files in this directory are development artifacts and may be outdated. Always refer to the main codebase for current implementation details.

### dev/
Development planning and release coordination:
- `architecture/` - Component PRDs, vision documents, audience analysis
- `planning/` - Refactoring opportunities, documentation quality audits
- `releases/` - Release plans, push instructions, version prompts
- `rust/` - Rust-specific implementation docs and benchmarks

## Usage

These documents are intended for:
- Core maintainers working on architectural decisions
- Release managers coordinating version updates
- AI assistants providing context-aware help

**For general usage documentation, see:**
- `README.md` (repository root)
- `docs/` (public documentation)
- `fraiseql-python/docs/` (Python SDK documentation)
