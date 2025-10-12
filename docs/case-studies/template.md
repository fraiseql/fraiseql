# Production Case Study Template

> **Purpose**: Document real-world FraiseQL deployments to showcase performance, cost savings, and production-readiness for potential adopters.

## Company Information

- **Company**: [Company Name or Anonymous]
- **Industry**: [e.g., SaaS, E-commerce, FinTech, Healthcare]
- **Use Case**: [Brief description of what they built with FraiseQL]
- **Production Since**: [Month Year]
- **Team Size**: [Number of developers]
- **Contact**: [Optional: email or website for verification]

## System Architecture

### Infrastructure
- **Hosting**: [AWS/GCP/Azure/DigitalOcean/Heroku/Self-hosted]
- **Database**: [PostgreSQL version, managed/self-hosted]
- **Application**: [FastAPI/Strawberry/Custom]
- **Deployment**: [Docker/Kubernetes/Serverless/Traditional]
- **Regions**: [Number of regions/datacenters]

### FraiseQL Configuration
- **Version**: [e.g., 0.11.0]
- **Modules Used**:
  - [ ] Core GraphQL
  - [ ] PostgreSQL-native caching
  - [ ] PostgreSQL-native error tracking
  - [ ] Multi-tenancy
  - [ ] TurboRouter (query caching)
  - [ ] APQ (Automatic Persisted Queries)

### Architecture Diagram

```
[Include a simple ASCII or mermaid diagram showing the architecture]

Example:
┌─────────────┐
│   Clients   │
└──────┬──────┘
       │
┌──────▼──────────┐
│    FastAPI      │
│   + FraiseQL    │
└──────┬──────────┘
       │
┌──────▼──────────┐
│   PostgreSQL    │
│  (Everything!)  │
└─────────────────┘
```

## Performance Metrics

### Request Volume
- **Daily Requests**: [number] requests/day
- **Peak Traffic**: [number] req/sec
- **Average Traffic**: [number] req/sec
- **Query Types**: [% queries vs % mutations]

### Response Times

| Metric | Value | Notes |
|--------|-------|-------|
| **P50** | [X ms] | Median response time |
| **P95** | [X ms] | 95th percentile |
| **P99** | [X ms] | 99th percentile |
| **P99.9** | [X ms] | 99.9th percentile |

### Cache Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Hit Rate** | [X%] | PostgreSQL UNLOGGED cache |
| **Miss Rate** | [X%] | |
| **Avg Cache Latency** | [X ms] | |
| **Cache Size** | [X GB] | Current cache table size |

### Database Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Avg Query Time** | [X ms] | Across all queries |
| **Pool Utilization** | [X%] | Database connection pool |
| **Slow Queries** | [X] | Queries > 1 second (per day) |
| **Database Size** | [X GB] | Total including cache |

## Cost Analysis

### Before FraiseQL

| Service | Monthly Cost | Purpose |
|---------|-------------|---------|
| [Traditional Stack Component] | $[X] | [Description] |
| [Traditional Stack Component] | $[X] | [Description] |
| [Traditional Stack Component] | $[X] | [Description] |
| **Total** | **$[X]/month** | |

### After FraiseQL

| Service | Monthly Cost | Purpose |
|---------|-------------|---------|
| PostgreSQL | $[X] | Everything (API, cache, errors, logs) |
| Application Hosting | $[X] | [Platform] |
| [Optional Components] | $[X] | [If any] |
| **Total** | **$[X]/month** | |

### Cost Savings

- **Monthly Savings**: $[X]/month ([X]% reduction)
- **Annual Savings**: $[X]/year
- **Eliminated Services**:
  - [Service 1]: Replaced with PostgreSQL-native feature
  - [Service 2]: Replaced with PostgreSQL-native feature

## Technical Wins

### Development Velocity

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| **API Development Time** | [X days] | [X days] | [X%] faster |
| **Lines of Code** | [X LOC] | [X LOC] | [X%] less |
| **API Changes** | [X hrs] | [X hrs] | [X%] faster |
| **Onboarding Time** | [X days] | [X days] | [X%] faster |

### Operational Benefits

1. **Unified Stack**: [Description of operational simplifications]
2. **Reduced Complexity**: [e.g., "No Redis, no Sentry, no separate caching layer"]
3. **Easier Debugging**: [e.g., "All data in PostgreSQL for easy correlation"]
4. **Simplified Deployments**: [e.g., "Single database connection string"]
5. **Better Monitoring**: [e.g., "Direct SQL queries for all metrics"]

## Challenges & Solutions

### Challenge 1: [Title]
**Problem**: [Description of challenge faced]

**Solution**: [How it was resolved with FraiseQL]

**Outcome**: [Results after solution]

### Challenge 2: [Title]
**Problem**: [Description]

**Solution**: [Resolution]

**Outcome**: [Results]

## Key Learnings

### What Worked Well

1. **[Learning 1]**: [Description]
2. **[Learning 2]**: [Description]
3. **[Learning 3]**: [Description]

### What Required Adjustment

1. **[Learning 1]**: [Description of what needed changing]
2. **[Learning 2]**: [Description]

### Recommendations for Others

1. **[Recommendation 1]**: [Advice for new adopters]
2. **[Recommendation 2]**: [Best practice discovered]
3. **[Recommendation 3]**: [Tip for success]

## PostgreSQL-Native Features Usage

### Error Tracking (Sentry Alternative)

- **Errors Tracked**: [X/day]
- **Error Grouping**: [How fingerprinting works in practice]
- **Cost Savings**: $[X]/month (vs Sentry)
- **Experience**: [Pros/cons compared to Sentry]

**Example Query**:
```sql
-- [Include an actual query they use for error monitoring]
SELECT
    error_fingerprint,
    COUNT(*) as occurrences,
    MAX(last_seen) as last_occurrence
FROM tb_error_log
WHERE environment = 'production'
  AND status = 'unresolved'
GROUP BY error_fingerprint
ORDER BY occurrences DESC
LIMIT 10;
```

### Caching (Redis Alternative)

- **Cache Hit Rate**: [X%]
- **Cache Size**: [X GB]
- **Cost Savings**: $[X]/month (vs Redis)
- **Experience**: [Performance comparison vs Redis]

**Example Pattern**:
```python
# [Include actual caching pattern they use]
await cache.set(f"user:{user_id}", user_data, ttl=3600)
```

### Multi-Tenancy (if applicable)

- **Tenants**: [X] active tenants
- **Isolation Strategy**: [RLS/Schema/DB-level]
- **Performance Impact**: [Minimal/Acceptable/etc]

## Testimonial

> "[Quote from team member or CTO about their experience with FraiseQL]"
>
> — [Name, Title, Company]

## Metrics Timeline

### Month 1: Initial Deployment
- [Key metrics]
- [Challenges]

### Month 3: Production Stable
- [Growth metrics]
- [Optimizations made]

### Month 6+: At Scale
- [Current performance]
- [Lessons learned]

## Contact & Verification

- **Case Study Date**: [Month Year]
- **FraiseQL Version**: [X.X.X]
- **Contact for Verification**: [Optional: email for potential customers to verify]
- **Public Reference**: [Yes/No - can FraiseQL publicly reference this deployment?]

---

## Template Instructions

When filling out this template:

1. **Be Specific**: Real numbers are more valuable than ranges
2. **Include Context**: Explain why metrics matter for your use case
3. **Show Comparisons**: Before/after comparisons are most compelling
4. **Add Real Code**: Actual SQL queries and patterns help others learn
5. **Be Honest**: Include challenges, not just wins
6. **Anonymize if Needed**: You can anonymize company name but keep metrics real
7. **Update Over Time**: Add "Update: [Date]" sections as system evolves

## What Makes a Good Case Study

✅ **Good**:
- "We handle 50M requests/day with P95 latency of 45ms"
- "Reduced our infrastructure costs from $4,200/mo to $800/mo"
- "Challenge: Initial cache hit rate was 60%, solved by adjusting TTLs to 73%"

❌ **Avoid**:
- "We handle many requests"
- "Saved some money"
- "Everything works perfectly" (not believable)

## Questions?

Contact: lionel.hamayon@evolution-digitale.fr
