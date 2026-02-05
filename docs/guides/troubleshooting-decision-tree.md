<!-- Skip to main content -->
---
title: Troubleshooting Decision Tree
description: Use this decision tree to quickly identify which troubleshooting guide applies to your problem.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# Troubleshooting Decision Tree

**Status:** ✅ Production Ready
**Audience:** Developers, DevOps, Support Engineers
**Reading Time:** 5 minutes
**Last Updated:** 2026-02-05

Use this decision tree to quickly identify which troubleshooting guide applies to your problem.

---

## 🎯 Start Here: What's Your Problem?

### Step 1: Identify the Symptom Category

**Select the one that best describes your situation:**

```text
<!-- Code example in TEXT -->
Does your problem involve...

1. Starting the server or deployment?
   → Go to: DEPLOYMENT ISSUES

2. GraphQL queries returning errors?
   → Go to: QUERY EXECUTION ISSUES

3. Mutations not working or failing?
   → Go to: MUTATION ISSUES

4. Real-time updates or subscriptions?
   → Go to: SUBSCRIPTION ISSUES

5. Authentication or authorization problems?
   → Go to: AUTHENTICATION & AUTHORIZATION

6. Slow queries or performance issues?
   → Go to: PERFORMANCE ISSUES

7. Database connection problems?
   → Go to: DATABASE CONNECTIVITY

8. Configuration issues?
   → Go to: CONFIGURATION ISSUES

9. Specific error codes?
   → Go to: ERROR CODE LOOKUP

10. Multi-service or federation problems?
    → Go to: FEDERATION ISSUES
```text
<!-- Code example in TEXT -->

---

## 🚀 DEPLOYMENT ISSUES

**Container fails to start:**

- Check Docker image build: `docker build . --no-cache`
- Verify Rust compilation: `cargo build --release`
- Review startup logs: `docker logs <container_id>`
- → **Full guide:** [Deployment Guide](../deployment/guide.md)

**Application crashes during startup:**

- Check schema compilation: `FraiseQL compile schema.json`
- Verify TOML syntax: `FraiseQL validate config.toml`
- Check environment variables: `env | grep FRAISEQL`
- → **Full guide:** [Production Deployment](./production-deployment.md)

**Server starts but no requests work:**

- Verify port is listening: `netstat -an | grep 5000`
- Check firewall rules: `sudo iptables -L`
- Test with curl: `curl -i http://localhost:5000/health`
- → **Full guide:** [Deployment Guide](../deployment/guide.md)

**Service won't connect to database:**

- → Go to: **DATABASE CONNECTIVITY** (below)

---

## 🔍 QUERY EXECUTION ISSUES

**Query returns GraphQL error:**

**Error type:**

```text
<!-- Code example in TEXT -->
Is the error about...

a) "Field X doesn't exist"?
   - Check schema is compiled: `schema.compiled.json` exists
   - Verify field name in schema definition
   - Regenerate schema: `FraiseQL compile`
   → [Troubleshooting Guide: Schema Errors](../TROUBLESHOOTING.md#schema-errors)

b) "Unauthorized" or "Permission denied"?
   → Go to: AUTHENTICATION & AUTHORIZATION

c) Database error (SQL error in message)?
   → Go to: DATABASE CONNECTIVITY

d) "Query timeout"?
   → Go to: PERFORMANCE ISSUES

e) Something else?
   → Go to: ERROR CODE LOOKUP
```text
<!-- Code example in TEXT -->

**Query returns null when expecting data:**

- Verify data exists in database: `SELECT * FROM table_name LIMIT 1;`
- Check WHERE clause filters: `SELECT * FROM table_name WHERE ... LIMIT 1;`
- Verify authorization isn't hiding data (row-level filters)
- Check pagination offset: Is `skip` too high?
- → [Troubleshooting Guide: No Results](../TROUBLESHOOTING.md#no-results)

**Query response is incomplete or truncated:**

- Check pagination limit: Default is 100, max is 1000
- Increase limit in query: `users(first: 500) { ... }`
- Check response size: Very large responses may be truncated
- → [Troubleshooting Guide: Incomplete Results](../TROUBLESHOOTING.md#incomplete-results)

**Query takes too long:**

- → Go to: **PERFORMANCE ISSUES**

---

## ✏️ MUTATION ISSUES

**Mutation fails or returns error:**

**Error type:**

```text
<!-- Code example in TEXT -->
Is the error about...

a) "Constraint violation" (duplicate key, foreign key)?
   - Check unique constraints: SHOW UNIQUE CONSTRAINTS
   - Verify foreign key exists: SELECT * FROM referenced_table WHERE id = ...
   → [Troubleshooting Guide: Constraint Violations](../TROUBLESHOOTING.md#constraint-violations)

b) "Invalid input" or "Validation error"?
   - Review input validation error message
   - Check field types match schema
   → [Troubleshooting Guide: Input Validation](../TROUBLESHOOTING.md#input-validation)

c) "Permission denied"?
   → Go to: AUTHENTICATION & AUTHORIZATION

d) Database error?
   → Go to: DATABASE CONNECTIVITY

e) Something else?
   → Go to: ERROR CODE LOOKUP
```text
<!-- Code example in TEXT -->

**Mutation succeeds but data looks wrong:**

- Verify mutation result in GraphQL response
- Query database directly: `SELECT * FROM table_name WHERE id = ...`
- Check for triggers or stored procedures modifying data
- → [Troubleshooting Guide: Data Integrity](../TROUBLESHOOTING.md#data-integrity)

**Mutation is very slow:**

- → Go to: **PERFORMANCE ISSUES**

---

## 🔄 SUBSCRIPTION ISSUES

**Subscription not connecting:**

- Verify WebSocket endpoint: `wss://server:5000/graphql`
- Check WebSocket proxy configuration
- Verify authentication token in subscription
- → [Troubleshooting Guide: WebSocket Connection](../TROUBLESHOOTING.md#websocket)

**Subscription connects but no events:**

- Verify CDC enabled: Check `tb_entity_change_log` has data
- Check event filtering: `where` clause might hide events
- Verify polling interval: Default 100ms, configurable
- → [Subscriptions Architecture](../architecture/realtime/subscriptions.md#debugging)

**Subscription receives stale data:**

- Check event timestamp vs current time
- Verify database replication lag (if multi-database)
- Check CDC polling interval: Increase if too low
- → [Troubleshooting Guide: Event Delivery](../TROUBLESHOOTING.md#event-delivery)

---

## 🔐 AUTHENTICATION & AUTHORIZATION

**Can't log in:**

- Verify OAuth provider is configured
- Check client ID and secret in vault: `echo $OAUTH_CLIENT_ID`
- Verify redirect URI matches OAuth provider settings
- Check OAuth provider health: Can you log in directly to provider?
- → [Authentication Provider Guide](../integrations/authentication/provider-selection-guide.md)

**Token rejected or expired:**

- Check token expiry: JWT tokens expire after 1 hour
- Verify token refresh working: Is refresh token valid?
- Check token signature: Token might be from different issuer
- → [Authentication Security Checklist](../integrations/authentication/SECURITY-CHECKLIST.md)

**Query or mutation denied with "Unauthorized":**

- Verify user is authenticated: Check Authorization header
- Check user has required role: Verify in RBAC configuration
- Check field-level permissions: Some fields might be restricted
- → [RBAC & Field Authorization](../enterpri../../guides/authorization-quick-start.md)

**Row-level data hidden or unauthorized:**

- Verify row-level security filter in schema: `where: Where... = FraiseQL.where(...)`
- Check tenant/org filtering is working
- Verify context values passed: `x-tenant-id` header set?
- → [RBAC Guide](../enterpri../../guides/authorization-quick-start.md)

---

## ⚡ PERFORMANCE ISSUES

**Single query is slow (>1 second):**

1. Is it the first query? (Cold start, schema compilation)
2. Is database responding slowly? Test database directly: `time psql -c "SELECT COUNT(*) FROM table"`
3. Is query complex (many nested fields)?
   - Simplify query, remove nested selections
   - Add filtering to reduce rows scanned
   - → [Performance Tuning Runbook](../operations/performance-tuning-runbook.md)

**Specific query always slow:**

- Analyze query: `EXPLAIN ANALYZE ...` on database
- Check indexes exist on filtered columns
- Check database statistics: `ANALYZE table_name;`
- Consider table-backed views (tv_*) for frequently accessed data
- → [View Selection Guide](./view-selection-performance-testing.md)

**All queries getting slower over time:**

- Check database connection pool: `SHOW max_connections;`
- Check for connection leaks: Count open connections
- Verify indexes haven't fragmented: `REINDEX;`
- Check disk space: `df -h`
- → [Database Connectivity](#database-connectivity)

**High latency for federation queries:**

- Check inter-service latency: `ping service-name`
- Verify database indexes on @key fields
- Check federation strategy: HTTP vs DirectDB vs Local
- → [Federation Troubleshooting](../integrations/federation/guide.md#troubleshooting)

**Memory usage increasing:**

- Check for memory leaks: Monitor `top -p <pid>`
- Verify connection pooling: Connections should be reused
- Check query result caching: Cache size might be too large
- → [Performance Tuning Runbook](../operations/performance-tuning-runbook.md)

---

## 🗄️ DATABASE CONNECTIVITY

**Can't connect to database:**

- Verify database server is running: `ping db-host`
- Check database port: `telnet db-host 5432`
- Verify credentials: Username, password, database name
- Check connection string: `postgresql://user:pass@host:5432/db`
- → [Database Connection Guide](../deployment/guide.md#database)

**Connection times out:**

- Increase timeout: `connect_timeout=30`
- Check firewall rules: `telnet db-host 5432`
- Check network latency: `ping db-host`
- Verify database isn't overloaded
- → [Connection Pooling Guide](../deployment/guide.md#pooling)

**"Too many connections" error:**

- Check connection pool size: Default 10, max 100
- Check for connection leaks: `SELECT COUNT(*) FROM pg_stat_activity;`
- Increase database `max_connections` if needed
- Enable connection pooling: PgBouncer or built-in pool
- → [Connection Pooling Guide](../deployment/guide.md#pooling)

**SSL/TLS connection errors:**

- Verify SSL mode: `sslmode=require` in connection string
- Check certificate chain: `openssl s_client -connect db-host:5432`
- Verify certificate not expired: `openssl x509 -enddate`
- → [TLS Configuration](../deployment/guide.md#tls)

**Authentication errors:**

- Check database user password (special characters might need escaping)
- Verify database user has SELECT/INSERT/UPDATE permissions
- Check `pg_hba.conf` (PostgreSQL) for connection restrictions
- → [Database Hardening](./production-security-checklist.md#database-hardening)

---

## ⚙️ CONFIGURATION ISSUES

**Configuration not taking effect:**

- Check TOML syntax: `FraiseQL validate config.toml`
- Verify environment variables override: Variables take precedence
- Check file permissions: Can FraiseQL read config file?
- Restart server after config change
- → [Troubleshooting Guide](../TROUBLESHOOTING.md)

**Environment variables not recognized:**

- Check variable name: `FRAISEQL_*` prefix required
- Verify case sensitivity: `FRAISEQL_RATE_LIMIT_ENABLED` (not camelCase)
- Check for typos: List all set variables: `env | grep FRAISEQL`
- → [Troubleshooting Guide](../TROUBLESHOOTING.md)

**TOML parsing error:**

- Use TOML validator: <https://www.toml-lint.com/>
- Check for invalid characters or quotes
- Verify array syntax: `[[section]]` vs `[section]`
- → [Configuration Examples](../deployment/guide.md#configuration)

---

## 🔢 ERROR CODE LOOKUP

**Have an error code?** (Format: E_XXXXX_NNN)

```text
<!-- Code example in TEXT -->
Error Category:
- E_PARSE_* → GraphQL parsing errors
- E_BINDING_* → Schema binding/type errors
- E_VALIDATION_* → Request validation errors
- E_AUTH_* → Authentication/authorization errors
- E_DB_* → Database errors
- E_FEDERATION_* → Federation-specific errors
- E_INTERNAL_* → Internal server errors

To find your error:
1. Copy error code: "E_BINDING_UNKNOWN_FIELD_202"
2. Search GitHub issues: "E_BINDING_UNKNOWN_FIELD_202"
3. Refer to [Main Troubleshooting Guide](../TROUBLESHOOTING.md)
```text
<!-- Code example in TEXT -->

**Don't see your error?**

- → Go to: **[Main Troubleshooting Guide](../TROUBLESHOOTING.md)**

---

## 🔗 FEDERATION ISSUES

**Entity not found in federation:**

- Verify @key directive matches across subgraphs
- Check entity exists in database: `SELECT * FROM table WHERE id = ...`
- Verify federation strategy: HTTP vs DirectDB vs Local
- → [Federation Troubleshooting](../integrations/federation/guide.md#troubleshooting)

**Federation query very slow:**

- Check inter-service latency: `ping other-service`
- Verify database indexes on @key fields
- Consider switching to DirectDB strategy
- → [Federation Performance](../integrations/federation/guide.md#performance-optimization)

**SAGA transaction failed:**

- Check SAGA logs: Look for compensation steps
- Verify all services are running
- Check inter-service network connectivity
- → [SAGA Pattern](../integrations/federation/sagas.md)

---

## 📞 Still Having Issues?

**If you can't find your problem:**

1. **Check if you have an error code:**
   - Search: [GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues)
   - Refer to: [Troubleshooting Guide](../TROUBLESHOOTING.md)

2. **Review comprehensive guides:**
   - **[Main Troubleshooting Guide](../TROUBLESHOOTING.md)** — All FAQs and common issues
   - **[Production Deployment](./production-deployment.md)** — Deployment procedures
   - **[Performance Tuning](../operations/performance-tuning-runbook.md)** — Performance optimization

3. **Get help:**
   - **Open a GitHub Issue:** [GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues)
   - **Include:** Error code, steps to reproduce, environment details (database, language, OS)
   - **Tag:** `troubleshooting` label for visibility

---

## See Also

**Complete Troubleshooting Guides:**

- **[Main Troubleshooting Guide](../TROUBLESHOOTING.md)** — Comprehensive FAQ
- **[Authentication Troubleshooting](../integrations/authentication/TROUBLESHOOTING.md)** — Auth-specific issues
- **[Federation Troubleshooting](../integrations/federation/guide.md#troubleshooting)** — Multi-service issues
- **[Observer Troubleshooting](../guides/observers.md#troubleshooting)** — Event system issues

**Related Guides:**

- **[Production Deployment](./production-deployment.md)** — Deployment and operations
- **[Performance Tuning](../operations/performance-tuning-runbook.md)** — Optimization
- **[Monitoring & Observability](./monitoring.md)** — Observability setup
- **[Common Gotchas](./common-gotchas.md)** — Pitfalls and solutions

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
