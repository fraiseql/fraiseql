---
title: Documentation Maintenance Plan
description: Plan for ongoing maintenance and updates to FraiseQL documentation
---

# Documentation Maintenance Plan

This document outlines the process for maintaining and updating FraiseQL documentation after v2.0.0-alpha.1 release.

## Update Schedule

### Weekly Checks

- ✅ Automated link validation (GitHub Actions)
- ✅ Code example validation (syntax checking)
- ⚠️ Check for broken external links (manual)

### Monthly Tasks

- Review and update examples for new releases
- Process user feedback from GitHub Issues
- Update best practices based on real-world usage
- Refresh troubleshooting guide with new issues
- Check for outdated information

### Quarterly Review

- Full documentation audit
- Update architecture patterns if needed
- Review all code examples for currency
- Verify all external links still valid
- Performance guide updates based on benchmarks

### Annual Review

- Comprehensive documentation rewrite for major releases
- Update version-specific documentation
- Archive previous versions
- Review and update all 16 language SDK docs

## Maintenance Responsibilities

### Documentation Owners

**Core Guides:**

- Getting Started & Foundation
- Architecture documentation
- Reference documentation

**Language-Specific:**

- Each of 16 SDK references (one maintainer per SDK)
- Language-specific best practices

**Operational:**

- Deployment guides
- Monitoring & observability
- Security & compliance
- Troubleshooting guides

### Process

1. **Identify needed changes**
   - GitHub Issues with `docs` label
   - User feedback and questions
   - Product releases and features
   - Bug fixes affecting documentation

2. **Create documentation PR**
   - Branch: `docs/description` or `docs/ISSUE-123`
   - Title: `docs: Clear description of changes`
   - Include reasoning in PR body

3. **Review and Merge**
   - At least one reviewer (preferably domain expert)
   - Automated checks must pass:
     - Link validation
     - Markdown linting
     - Code example syntax
   - Merge to main branch

4. **Deploy**
   - ReadTheDocs auto-deploys on main push
   - Verify at https://fraiseql.readthedocs.io
   - Wait 2-3 minutes for cache clear

## Contributing to Docs

### Getting Started with Docs

```bash
# Install dependencies
pip install -r docs/requirements.txt

# Build documentation locally
mkdocs serve

# Visit http://localhost:8000
```

### Format Guidelines

- Use GitHub Flavored Markdown (GFM)
- Code fences with language tags (python, typescript, sql, etc.)
- Admonitions for notes, warnings, tips
- Tables for structured information
- Internal links using relative paths: `../guides/production-deployment.md`

### Code Examples

All code examples should:

- Be syntactically valid for the language
- Include sufficient context (imports, setup)
- Show both correct and incorrect patterns (with ❌/✅ marks)
- Include explanatory comments
- Be tested and verified

### Adding New Pages

1. Create file in appropriate directory
2. Add front matter (title, description, keywords, tags)
3. Add entry to `mkdocs.yml` nav section
4. Submit PR for review

### Updating Existing Pages

- Use `Last Updated` in front matter: `Last Updated: YYYY-MM-DD`
- Note significant changes in commit message
- Link to related documentation
- Update table of contents if structure changed

## Documentation CI/CD

### Automated Checks

`.github/workflows/documentation.yml` runs on every PR:

- ✅ YAML validation (mkdocs.yml, .readthedocs.yml)
- ✅ Link validation (internal and external)
- ✅ Markdown linting
- ✅ Documentation structure check
- ✅ Code example syntax validation

### Validation Tools

Located in `tools/`:

```bash
# Validate all links
python3 tools/validate-docs-links.py docs/

# Test documentation configuration
python3 tools/test-docs-site.py

# Check for development markers
python3 tools/find-development-markers.py docs/

# Validate code examples
python3 tools/validate-code-examples.py docs/
```

### Deployment

ReadTheDocs automatically builds and deploys:

1. On every push to `main` branch
2. On every PR (preview)
3. On manual trigger via RTD dashboard

### Health Checks

```bash
# Check build status
curl https://readthedocs.org/api/v3/projects/fraiseql/builds/

# View live site
https://fraiseql.readthedocs.io

# Check search index
https://fraiseql.readthedocs.io/search-index.json
```

## Managing SDK Documentation

### Adding a New Language

1. Create `docs/integrations/sdk/{language}-reference.md`
2. Use existing SDK reference as template
3. Add to `mkdocs.yml` under "SDK References"
4. Include:
   - Installation instructions
   - Quick start example
   - All 30 features documented
   - Type mapping table
   - Common patterns
   - Error handling

### Updating SDK Documentation

When FraiseQL API changes:

1. Update code examples in SDK reference
2. Update examples in `docs/examples/`
3. Update tutorials that use affected SDK
4. Run validation: `python3 tools/validate-code-examples.py`
5. Update CHANGELOG.md
6. Test in CI/CD

## Reporting and Fixing Issues

### Documentation Bugs

Report documentation issues:

```
Title: [DOCS] Description of issue
Labels: docs, bug
Body:
- Where: Specific page URL
- Issue: What's wrong (unclear, incorrect, outdated)
- Suggestion: How to fix it (if known)
```

### Urgent Fixes

For critical documentation errors (incorrect security info, broken tutorial):

1. Create issue with `urgent` label
2. Create fix PR immediately
3. Reference issue in commit message
4. Merge without waiting for full review if critical

## Metrics & Analytics

### Tracking Documentation Usage

- ReadTheDocs analytics dashboard
- Popular pages
- Common search terms
- Referral sources
- Time on page

### Quality Metrics

- Broken links: 0 (automated check)
- Code example success rate: 100% (syntax validation)
- Page load time: < 2s (Material theme optimization)
- Search relevance: High (indexed on all pages)

## Troubleshooting Documentation

### Common Maintenance Tasks

**Update SDK for new version:**

```bash
cd docs/integrations/sdk/
# Update {language}-reference.md
python3 ../../tools/validate-docs-links.py
git commit -m "docs(sdk): Update for v2.1.0"
```

**Add new feature documentation:**

1. Add to appropriate section
2. Link from related pages
3. Add to table of contents
4. Submit PR with examples

**Fix broken external link:**

1. Search for all occurrences: `grep -r "broken-url" docs/`
2. Replace with working link or archive link
3. Update CHANGELOG.md
4. Commit fix

**Remove outdated information:**

- Mark as deprecated in front matter
- Add note with deprecation date
- Suggest alternative in related files
- Schedule for removal in next major release

## Documentation Roadmap

**Short-term (1-3 months):**

- Gather user feedback from alpha testing
- Update troubleshooting based on issues
- Add missing examples
- Clarify complex sections

**Medium-term (3-6 months):**

- Add advanced tutorials
- Expand enterprise guides
- Add performance benchmarks
- Create video tutorials (optional)

**Long-term (6-12 months):**

- Add community-contributed patterns
- Expand to 20+ languages if SDKs created
- Add interactive examples
- Create certification guides

## Version Management

### Documentation Versions

- **Latest:** https://fraiseql.readthedocs.io (current: v2.0.0-alpha.1)
- **Stable:** https://fraiseql.readthedocs.io/stable/ (available after GA)
- **Dev:** https://fraiseql.readthedocs.io/dev/ (development branch)

### Versioning Updates

When releasing new version:

1. Create release branch `docs/vX.Y.Z`
2. Update version in all files
3. Create new docs version in ReadTheDocs
4. Redirect `/latest` to new version
5. Archive previous version

## Contact & Support

- **Documentation Issues:** [GitHub Issues](https://github.com/fraiseql/fraiseql/issues?q=label:docs)
- **Documentation Discussions:** [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- **Maintainer:** @fraiseql-team

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
**Maintained By:** FraiseQL Documentation Team
