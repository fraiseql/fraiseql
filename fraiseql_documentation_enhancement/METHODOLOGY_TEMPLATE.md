# Implementation Methodology Template

## Purpose

This template provides standardized implementation methodology for all FraiseQL documentation enhancement patterns. Copy and adapt this section into each prompt.md file.

## Standard Template

```markdown
## Implementation Methodology

### Development Workflow

**Critical: Commit Early and Often**

Break this [PATTERN_NAME] documentation into manageable commits:

1. **[PHASE_NAME] Commit** ([TIME_ESTIMATE] minutes)
   ```bash
   # [DESCRIPTION_OF_PHASE]
   git add [FILES]
   git commit -m "docs: [COMMIT_MESSAGE_PREFIX]

   - [BULLET_POINT_DESCRIPTION]
   - [BULLET_POINT_DESCRIPTION]
   - References #[issue-number]"
   ```

2. **[NEXT_PHASE] Commit** ([TIME_ESTIMATE] minutes)
   ```bash
   # [DESCRIPTION_OF_PHASE]
   git add [FILES]
   git commit -m "docs: [COMMIT_MESSAGE_PREFIX]

   - [BULLET_POINT_DESCRIPTION]
   - [BULLET_POINT_DESCRIPTION]"
   ```

[REPEAT FOR 4-6 PHASES TOTAL]

### Progress Validation

After each commit:
- [ ] Build documentation locally (`mkdocs serve`)
- [ ] Validate SQL syntax in examples (if applicable)
- [ ] Test GraphQL schema definitions (if applicable)
- [ ] Check cross-references and links
- [ ] Ensure examples follow PrintOptim patterns

### Risk Management

**For [SPECIFIC_RISK]:**
```bash
# [RISK_MITIGATION_STRATEGY]
```

**Quality checks:**
```bash
# Test documentation build frequently
mkdocs serve
# Check for broken links
# Validate code syntax in examples
# Review diff before each commit: git diff --cached
```

**Recovery strategy:**
```bash
# If commit has issues, amend instead of new commit
git add -A
git commit --amend --no-edit

# Or create fixup commit for small changes
git commit -m "fixup: [BRIEF_DESCRIPTION]"
```
```

## Customization Guidelines

### Phase Planning

**Typical phase structure:**
1. **Structure/Planning** (5-15 min) - Document outline, headings, TODOs
2. **Core Content** (20-40 min) - Main technical implementation
3. **Examples** (15-30 min) - Working code samples and patterns
4. **Integration** (10-25 min) - Cross-references, links, updates
5. **Polish** (5-15 min) - Final review, index updates, cleanup

### Time Estimates

**Guidelines:**
- Structure phases: 5-15 minutes
- Content phases: 15-40 minutes based on complexity
- Example phases: 15-30 minutes
- Integration phases: 10-25 minutes
- Polish phases: 5-15 minutes

**Total estimates:**
- Small patterns: 60-90 minutes (3-4 commits)
- Medium patterns: 90-150 minutes (4-5 commits)
- Large patterns: 120-200 minutes (5-6 commits)

### Commit Message Format

**Prefix patterns:**
- `docs: add [pattern-name] [component]` - New content
- `docs: update [existing-doc] for [pattern]` - Modifications
- `docs: complete [pattern-name] [section]` - Finalizations

**Body format:**
```
- [Specific change description]
- [Another specific change]
- [Reference to issue if applicable]
```

### Risk Categories

**Common risks and mitigations:**

1. **Large file modifications**
   - Create feature branch
   - Backup original file
   - Use intermediate commits for checkpoints

2. **Complex SQL examples**
   - Test in separate file first
   - Validate syntax before adding to docs
   - Include error handling examples

3. **Integration changes**
   - Preserve existing content
   - Add before/after comparisons
   - Update cross-references carefully

4. **Code example failures**
   - Test all examples independently
   - Use consistent variable naming
   - Include complete context

### Quality Validation

**Between each commit:**
- Documentation builds without errors
- All code examples have correct syntax
- Links and references work properly
- Style matches FraiseQL conventions
- Examples follow PrintOptim patterns
- File structure remains logical

**Final validation:**
- Complete documentation review
- All cross-references tested
- Examples tested independently
- Troubleshooting section complete
- Integration points verified

## Usage Instructions

1. **Copy template** into prompt.md file
2. **Replace placeholders** with pattern-specific content:
   - `[PATTERN_NAME]` - Name of the documentation pattern
   - `[PHASE_NAME]` - Descriptive phase name
   - `[TIME_ESTIMATE]` - Realistic time estimate
   - `[DESCRIPTION_OF_PHASE]` - What this phase accomplishes
   - `[FILES]` - Specific files being modified
   - `[COMMIT_MESSAGE_PREFIX]` - Meaningful commit prefix
   - `[BULLET_POINT_DESCRIPTION]` - Specific changes in this commit
   - `[SPECIFIC_RISK]` - Pattern-specific risk factors

3. **Customize phases** for pattern complexity
4. **Adjust time estimates** based on content volume
5. **Add specific risks** relevant to the pattern
6. **Include pattern-specific validation** steps

## Examples

### Simple Pattern (3-4 commits)
```markdown
1. **Structure Commit** (10 minutes)
2. **Content Commit** (25 minutes)
3. **Examples Commit** (20 minutes)
4. **Integration Commit** (15 minutes)
```

### Complex Pattern (5-6 commits)
```markdown
1. **Planning Commit** (10 minutes)
2. **Architecture Commit** (30 minutes)
3. **Implementation Commit** (35 minutes)
4. **Examples Commit** (25 minutes)
5. **Integration Commit** (20 minutes)
6. **Polish Commit** (15 minutes)
```

### File Modification Pattern
```markdown
1. **Planning Commit** (5 minutes) - Add TODO markers
2. **Foundation Commit** (20 minutes) - Core new content
3. **Integration Commit** (25 minutes) - Merge with existing
4. **Examples Commit** (20 minutes) - Update all examples
5. **Finalization Commit** (10 minutes) - Cross-references
```

## Quality Assurance

This template ensures:
- ✅ Consistent commit strategies across all patterns
- ✅ Realistic time estimates for planning
- ✅ Risk mitigation for complex documentation
- ✅ Quality validation at each step
- ✅ Recovery strategies for issues
- ✅ Professional git history for reviews
