# Diagramming Enhancement Roadmap

**Status:** ENHANCEMENT PLAN
**Created:** January 29, 2026
**Priority:** Medium (improves visual clarity, not blocking)

---

## Current Implementation

### Approach: ASCII Art Diagrams

- **Rationale:**
  - Renders correctly on GitHub markdown without build dependencies
  - Version-controllable in plain text
  - Works immediately with existing documentation pipeline
  - No infrastructure setup required

- **Benefits:**
  - Zero build tooling required
  - Git-friendly (diffs are readable)
  - Works on any platform/browser
  - Simple to edit and maintain

### Current Diagrams

| Topic | Diagram Type | Count | Current Format |
|-------|--------------|-------|-----------------|
| 1.1 | Comparison matrices | 3 | ASCII tables |
| 1.2 | Mental model flowchart | 1 | ASCII diagram |
| 1.3 | View system matrix | 2 | ASCII (table + diagram) |
| 1.3 | Architecture layers | 1 | ASCII diagram |
| 1.4 | Design principles | 3 | ASCII diagrams |
| **Total** | **Various** | **~10** | **ASCII** |

---

## Future Implementation

### Proposed Approach: D2 Diagramming Language

D2 is a modern diagramming language that produces professional diagrams from code:

- Open source (GitHub: terrastruct/d2)
- Renders SVG output (scalable, beautiful)
- Version-controllable source code
- Supports: flowcharts, sequence diagrams, Gantt charts, class diagrams, ERD

### Example: Architecture Layers

**Current ASCII:**

```
┌─────────────────────────────────────┐
│  Authoring (Python/TypeScript)      │
└────────────┬────────────────────────┘
             │ generates
┌────────────▼────────────────────────┐
│  schema.json (API contract)         │
└────────────┬────────────────────────┘
             │ compiles
┌────────────▼────────────────────────┐
│  schema.compiled.json (SQL templates)
└────────────┬────────────────────────┘
             │ loads
┌────────────▼────────────────────────┐
│  Runtime Server (query execution)   │
└─────────────────────────────────────┘
```

**Future D2 Version:**

```d2
Authoring: {
  label: "Authoring\n(Python/TypeScript)"
}
Schema JSON: {
  label: "schema.json\n(API contract)"
}
Compiled Schema: {
  label: "schema.compiled.json\n(SQL templates)"
}
Runtime: {
  label: "Runtime Server\n(Query execution)"
}

Authoring -> Schema JSON: generates
Schema JSON -> Compiled Schema: compiles
Compiled Schema -> Runtime: loads
```

### D2 Diagrams to Implement

#### Topic 1.1: What is FraiseQL?

- [ ] Comparison matrix: FraiseQL vs Apollo Server
- [ ] Comparison matrix: FraiseQL vs Hasura
- [ ] Comparison matrix: FraiseQL vs Custom REST

**Type:** Class diagram or comparison table
**Effort:** 1-2 hours per diagram

#### Topic 1.3: Database-Centric Architecture

- [ ] Four-tier view system (v_*, tv_*, va_*, ta_*)
- [ ] Architecture layers (Authoring → Compilation → Runtime)
- [ ] Data flow (request → SQL execution)
- [ ] Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)

**Type:** Entity relationship, flowchart
**Effort:** 2-3 hours per diagram

#### Topic 1.4: Design Principles

- [ ] Design principles relationship (how 5 principles work together)
- [ ] Compile-time optimization workflow
- [ ] Type safety enforcement layers

**Type:** Flowchart, sequence diagram
**Effort:** 1-2 hours per diagram

### Infrastructure Requirements

**Option A: Render at Build Time**

- Add D2 CLI tool to documentation build pipeline
- Generates SVG files during `docs build`
- Stores SVG in `/docs/diagrams/` directory
- Links SVG from markdown

**Option B: Render in Browser (Simple)**

- Use D2 online playground URL
- Link directly from diagrams
- No build infrastructure needed

**Option C: Commit SVG Files (Hybrid)**

- Generate SVG locally with D2 CLI
- Commit SVG to git
- Keep D2 source files alongside
- Allows offline use without build infrastructure

### Timeline Estimate

**Total Effort:** 20-25 hours

| Phase | Work | Hours | Timeline |
|-------|------|-------|----------|
| Phase 2 | Set up D2 infrastructure | 3-4 | Week 1 |
| Phase 2 | Convert Phase 1 diagrams | 12-15 | Weeks 2-3 |
| Phase 2+ | Create new Phase 2+ diagrams in D2 | Ongoing | As topics written |

### Recommended Approach

**Phase 1 (Now):** Continue with ASCII diagrams

- Fast iteration
- Zero dependencies
- Markdown-compatible
- Good enough for initial release

**Phase 2-3:** Implement D2 for architecture diagrams

- After Phase 1 content is stable
- Focus on complex diagrams (architecture, flows)
- Keep simple matrices as ASCII or Markdown tables
- Gradual migration (not all-or-nothing)

**Phase 7 (Finalization):** Visual polish pass

- Convert remaining ASCII diagrams
- Ensure consistent D2 styling across all diagrams
- Test rendering in all documentation formats
- Performance optimization

### D2 Styling Standard

When implementing D2, use consistent styling:

```d2
# Color scheme matching FraiseQL brand
direction: down
classes: {
  core: {
    fill: #4A90E2
    stroke: #2E5C8A
    text-color: white
  }
  process: {
    fill: #50C878
    stroke: #2E7D4E
    text-color: white
  }
  data: {
    fill: #FFB84D
    stroke: #B38A2C
    text-color: white
  }
}

# Consistent arrow styling

*.a -> *.b: {
  stroke: #666
  stroke-width: 2
}
```

---

## Decision Matrix

| Criteria | ASCII | D2 |
|----------|-------|-----|
| **Setup Required** | None | Build tool + D2 CLI |
| **GitHub Rendering** | Native | Via SVG commit |
| **Visual Quality** | Basic | Professional |
| **Maintainability** | High | High |
| **Version Control** | Great | Good |
| **PDF Export** | Good | Excellent |
| **Effort to Convert** | N/A | 20-25 hours |

### Verdict

- **Phase 1:** ASCII (unblock content writing)
- **Phase 2+:** D2 (visual polish and professionalism)

---

## Success Criteria for D2 Implementation

When D2 migration begins:

- [ ] D2 CLI integrated into documentation build pipeline
- [ ] All Phase 1 diagrams converted to D2
- [ ] Consistent styling applied across all diagrams
- [ ] SVG output renders correctly in all documentation formats
- [ ] Build time <5 seconds (diagram rendering)
- [ ] Diagrams render correctly on GitHub, HTML, PDF
- [ ] D2 source files stored alongside markdown
- [ ] Documentation updated with D2 authoring guidelines

---

## Related Documentation

- Phase 2 detailed planning (will include D2 implementation task)
- Phase 7 finalization criteria (visual polish checklist)
- Build pipeline documentation

---

## Summary

**Phase 1:** Keep ASCII diagrams (fast, simple, works now)
**Phase 2-3:** Implement D2 for architecture and flow diagrams (upgrade visual quality)
**Phase 7:** Complete D2 migration as part of finalization polish
