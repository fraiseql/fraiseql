# fraiseql-wire Implementation Phases

This directory contains the phased implementation plan for fraiseql-wire, a minimal async Rust query engine that streams JSON data from Postgres 17.

## Phase Overview

| Phase | Type | Description | Status |
|-------|------|-------------|--------|
| [Phase 0](phase-0-project-setup.md) | GREENFIELD | Project setup & foundation | 📋 Not Started |
| [Phase 1](phase-1-protocol-foundation.md) | RED | Protocol foundation (encoding/decoding) | 📋 Not Started |
| [Phase 2](phase-2-connection-layer.md) | GREEN | Connection layer (TCP/Unix sockets) | 📋 Not Started |
| [Phase 3](phase-3-json-streaming.md) | REFACTOR | JSON streaming with backpressure | 📋 Not Started |
| [Phase 4](phase-4-client-api.md) | QA | Client API & query builder | 📋 Not Started |
| [Phase 5](phase-5-rust-predicates.md) | GREENFIELD | Rust-side predicate filtering | 📋 Not Started |
| [Phase 6](phase-6-polish-documentation.md) | QA | Polish, documentation, tests | 📋 Not Started |

## TDD Cycle Mapping

This project follows a modified TDD cycle:

* **Phase 0 (GREENFIELD)**: Bootstrap project structure
* **Phase 1 (RED)**: Define protocol interfaces with tests (fail first)
* **Phase 2 (GREEN)**: Implement connection layer (make tests pass)
* **Phase 3 (REFACTOR)**: Add streaming abstraction (improve design)
* **Phase 4 (QA)**: Validate with high-level API and examples
* **Phase 5 (GREENFIELD)**: Add new feature (Rust predicates)
* **Phase 6 (QA)**: Final validation and documentation

## How to Use These Plans

Each phase plan includes:

* **Objective**: What this phase accomplishes
* **Context**: Why this phase is needed and how it fits
* **Prerequisites**: What must be completed first
* **Files to Create/Modify**: Specific file changes
* **Implementation Steps**: Detailed code with examples
* **Verification Commands**: How to test the phase
* **Expected Output**: What success looks like
* **Acceptance Criteria**: Checklist for completion
* **DO NOT**: What explicitly does NOT belong in this phase

## Development Workflow

### For Claude (or another AI agent)

When the user requests implementation:

1. Read the current phase plan
2. Verify prerequisites are met
3. Follow implementation steps precisely
4. Create/modify files as specified
5. Run verification commands
6. Check acceptance criteria
7. Commit changes if successful
8. Move to next phase

### For Human Developers

1. Read phase plan
2. Understand context and design decisions
3. Implement following the provided examples
4. Run tests frequently
5. Check acceptance criteria before moving on

## Execution with opencode

These phases can be executed using `opencode`:

```bash
# Run a specific phase
opencode run .claude/phases/phase-0-project-setup.md

# Run with local model for simple tasks
opencode run .claude/phases/phase-0-project-setup.md --model local

# Chain phases
for phase in .claude/phases/phase-*.md; do
  opencode run "$phase" || break
done
```

## Phase Dependencies

```
Phase 0 (Setup)
    ↓
Phase 1 (Protocol)
    ↓
Phase 2 (Connection)
    ↓
Phase 3 (Streaming)
    ↓
Phase 4 (Client API)
    ↓
Phase 5 (Predicates)
    ↓
Phase 6 (Polish)
```

Each phase depends on the previous one being complete.

## Key Design Principles

These principles are enforced across all phases:

1. **Streaming first**: Never buffer full result sets
2. **One-way data flow**: Server → client only
3. **Bounded memory**: Scales with chunk size, not result size
4. **Fail fast**: Schema violations terminate streams
5. **Explicit state**: Connection state machine is clear
6. **Pure protocol**: Encoding/decoding has no side effects
7. **Single query**: One active query per connection

## Testing Strategy

* **Unit tests**: Each phase adds tests for new components
* **Integration tests**: Phase 6 adds comprehensive end-to-end tests
* **Manual testing**: Each phase includes manual test instructions
* **Performance validation**: Phase 6 validates memory/latency characteristics

## Documentation Strategy

* **Inline docs**: Added as code is written
* **Module docs**: Each module has comprehensive documentation
* **Examples**: Phase 4+ adds runnable examples
* **README**: Updated in Phase 6
* **API docs**: Generated via rustdoc

## Scope Boundaries

### ✅ In Scope

* Single JSON/JSONB column queries
* Views named `v_{entity}` or tables `tv_{entity}`
* TCP and Unix socket connections
* Simple Query protocol
* SQL predicates (WHERE, ORDER BY)
* Rust predicates (client-side filtering)
* Async streaming with backpressure
* Query cancellation via drop

### ❌ Out of Scope

* Multi-column result sets
* Arbitrary SQL queries
* Fact tables (`tf_{entity}`)
* Arrow data plane (`va_{entity}`)
* Write operations (INSERT/UPDATE/DELETE)
* Transactions (BEGIN/COMMIT/ROLLBACK)
* Prepared statements (Extended Query protocol)
* Connection pooling (separate concern)
* TLS/SSL (for MVP)
* SCRAM authentication (for MVP)

## Troubleshooting

If a phase fails:

1. Check prerequisites are completed
2. Review acceptance criteria from previous phase
3. Run verification commands for previous phase
4. Check error messages carefully
5. Review DO NOT section (might be out of scope)

## Contributing

When adding new phases or modifying existing ones:

1. Follow the phase template structure
2. Include concrete code examples
3. Specify verification commands
4. Define clear acceptance criteria
5. Document what does NOT belong in the phase

## Questions?

For questions about the implementation plan:

1. Check the PRD.md for architectural decisions
2. Review CLAUDE.md for project-specific guidance
3. Check phase comments for context
4. Open an issue if unclear

---

**Status Legend**:

* 📋 Not Started
* 🔄 In Progress
* ✅ Complete
* ⏸️ Blocked
* ⚠️ Needs Review
