# FraiseQL Production Case Studies

Real-world production deployments showcasing FraiseQL's performance, cost savings, and scalability.

## Overview

This directory contains case studies from teams running FraiseQL in production. Each case study provides:

- **Architecture details**: Infrastructure, database configuration, deployment strategy
- **Performance metrics**: Request volume, latency (P50/P95/P99), cache hit rates
- **Cost analysis**: Before/after comparisons, monthly savings
- **Technical wins**: Development velocity improvements, operational benefits
- **Challenges & solutions**: Real problems faced and how they were solved
- **Lessons learned**: Recommendations for other teams

## Available Case Studies

**No production case studies available yet.**

We're actively seeking teams running FraiseQL in production to share their experiences. See [Submit Your Case Study](#submit-your-case-study) below.

---

## Submit Your Case Study

Running FraiseQL in production? We'd love to feature your deployment!

### Benefits of Sharing Your Story

1. **Help the Community**: Your experience helps others evaluate FraiseQL
2. **Validation**: Demonstrates real-world production use cases
3. **Networking**: Connect with other FraiseQL users
4. **Recognition**: Public acknowledgment of your team's work
5. **Feedback Loop**: Direct line to maintainers for feature requests

### How to Submit

1. **Use the Template**: Start with [`template.md`](./template.md)
2. **Gather Metrics**: Collect performance, cost, and operational data
3. **Write Honestly**: Include both wins and challenges
4. **Anonymize if Needed**: You can keep company details private
5. **Contact Us**: Email lionel.hamayon@evolution-digitale.fr

### What We're Looking For

✅ **Great Case Studies Include**:
- Specific metrics (not just "fast" but "P95 latency of 65ms")
- Cost comparisons ($X/month before → $Y/month after)
- Real challenges faced and solutions found
- Actual SQL queries or code patterns used
- Timeline showing metrics evolution

✅ **Any Scale Welcome**:
- MVP/Startup: 100K req/day
- Growth: 1M-10M req/day
- Scale: 10M+ req/day

✅ **Any Use Case**:
- SaaS platforms
- E-commerce
- FinTech
- Healthcare
- Enterprise B2B
- Internal tools

## Case Study Template

Download: [`template.md`](./template.md)

The template includes sections for:
- Company & infrastructure information
- Architecture diagram
- Performance metrics (traffic, latency, cache hit rate)
- Cost analysis (before/after)
- Technical wins & development velocity
- Challenges faced & solutions implemented
- PostgreSQL-native features usage
- Lessons learned & recommendations

**Estimated Time**: 2-4 hours to complete

## Questions?

- **General**: lionel.hamayon@evolution-digitale.fr
- **Technical**: Open a [GitHub Discussion](../discussions)
- **Security**: See [SECURITY.md](../../SECURITY.md)

## Case Study Guidelines

### Data Requirements

**Minimum Metrics**:
- Request volume (req/day or req/sec)
- Latency (at least P95)
- Cache hit rate (if using caching)
- Monthly cost (before & after if migrating)

**Recommended Metrics**:
- P50, P95, P99, P99.9 latency
- Database query performance
- Error rates
- Pool utilization
- Development velocity improvements

### Privacy Options

You can choose your level of anonymity:

1. **Fully Public**: Company name, logo, testimonial, contact
2. **Semi-Anonymous**: Industry, metrics, no company name
3. **Fully Anonymous**: "Anonymous SaaS Company", no identifying details

All options are valuable! Even anonymous case studies help potential adopters.

### Review Process

1. **Submit**: Send completed template to lionel.hamayon@evolution-digitale.fr
2. **Review**: We'll review for completeness and technical accuracy (1-2 days)
3. **Revisions**: Work with you to clarify any details if needed
4. **Publication**: Add to this directory via PR (with your approval)
5. **Updates**: You can request updates anytime as your deployment evolves

## Example Metrics That Help Others

### Performance Metrics
```
✅ Good: "P95 latency is 65ms with 12.5M req/day"
❌ Vague: "Fast performance at scale"

✅ Good: "Cache hit rate improved from 52% to 73% after TTL tuning"
❌ Vague: "Caching works well"
```

### Cost Analysis
```
✅ Good: "Reduced from $2,760/mo to $1,475/mo (46.5% savings)"
❌ Vague: "Saved money compared to old stack"

✅ Good: "Eliminated: Redis ($340/mo), Sentry ($890/mo)"
❌ Vague: "Removed some third-party services"
```

### Technical Details
```
✅ Good: "Using db.r6g.xlarge with 200 connection pool per pod"
❌ Vague: "PostgreSQL on AWS"

✅ Good: "Row-Level Security with SET LOCAL app.current_tenant_id"
❌ Vague: "Multi-tenancy with PostgreSQL"
```

## Verification

To maintain credibility, we may:
- Ask for verification of key metrics (screenshots, logs)
- Request reference contact for potential customers
- Follow up after 6 months for updated metrics

All verification is confidential and used only to ensure accuracy.

## Updates & Corrections

Found an error or have updated metrics? Email us or open a PR with:
- Case study file path
- Section to update
- New/corrected information
- Update date

We'll add an "Updated: [Date]" note to the case study.

---

**Ready to share your FraiseQL production story?** Contact lionel.hamayon@evolution-digitale.fr to get started!
