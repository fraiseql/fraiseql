# FraiseQL - Targeted Deep-Dive Review Questions

These questions are designed to elicit comprehensive security, performance, and architectural reviews specific to FraiseQL's implementation.

## 🔒 SECURITY QUESTIONS

### Authentication & Authorization
1. **JWT Implementation**: Examine `fraiseql_rs/src/auth/` - Are JWT tokens properly validated? Is there proper key rotation? Can expired tokens be replayed?
2. **OAuth Flow**: Are OAuth scopes properly enforced? Can an attacker escalate their OAuth permissions?
3. **Session Management**: If sessions are used, are they properly invalidated on logout? What's the timeout policy?
4. **Password Handling**: How are passwords hashed? (bcrypt, argon2, scrypt?) What are the iteration counts?
5. **API Key Protection**: If API keys are used, are they properly rotated and revoked? Can they leak via logs?

### RBAC & Multi-Tenancy
1. **Field-Level Permissions**: Review `fraiseql_rs/src/rbac/` - How are field-level permissions enforced? Can a user request a denied field directly?
2. **Row-Level Security**: Are database queries properly filtered by tenant ID? Can one tenant's query accidentally return another tenant's data?
3. **RBAC Bypass**: Can permissions be bypassed by:
   - Directly querying the database?
   - Using federation/references?
   - Modifying GraphQL variables?
   - Requesting schema metadata?
4. **Role Inheritance**: If roles have inheritance, can cycles be created? Can this cause issues?
5. **Nested Field Access**: If a user can't access field A, but field A contains field B which they can access, what happens?

### GraphQL & Query Safety
1. **Query Depth Limits**: Is there a maximum GraphQL query depth? What is it? Can attackers craft deep queries for DoS?
2. **Query Complexity**: Are all GraphQL operations analyzed for complexity? Can expensive operations be limited?
3. **Batch Queries**: Can an attacker batch N expensive queries in one request?
4. **Mutations vs Queries**: Are mutations properly protected? Can someone craft a mutation that performs unintended actions?
5. **Introspection**: Is GraphQL introspection disabled in production? Can it leak schema details?
6. **SQL Injection**: Review mutation building - is SQL properly parameterized? Any raw string concatenation?

### Rate Limiting & DoS
1. **Rate Limit Implementation**: Review `fraiseql_rs/src/security/rate_limit.rs` - Is rate limiting per-user, per-IP, or both?
2. **Bypass Vectors**: Can rate limits be bypassed by:
   - Distributed requests?
   - Header manipulation?
   - Connection pooling tricks?
3. **Resource Limits**: Are there limits on:
   - Query timeout?
   - Result set size?
   - File upload size?
   - Connection pool size?
4. **WebSocket DoS**: In `fraiseql_rs/src/subscriptions/` - what happens if 10,000 clients subscribe to the same field?
5. **Gradual Backoff**: Is there exponential backoff for repeated failures?

### Data Protection
1. **Encryption**: Are sensitive fields encrypted at rest? In transit (TLS enforced)?
2. **Secrets Management**: How are secrets (API keys, passwords, tokens) handled? Any in logs/errors?
3. **Audit Logging**: Is there audit logging of who accessed what data and when?
4. **Data Masking**: Are sensitive fields masked in logs and errors?
5. **PII Handling**: How is personally identifiable information protected?

### Input Validation
1. **GraphQL Variables**: Are all GraphQL variables validated before use?
2. **Type Coercion**: Can type coercion be exploited? (e.g., "123" → 123)
3. **Null Handling**: What happens with unexpected nulls in required fields?
4. **CSRF Protection**: Is CSRF protection implemented? How does it work with GraphQL?

---

## ⚡ PERFORMANCE QUESTIONS

### Database Queries
1. **N+1 Queries**: In `fraiseql_rs/src/query/` - review query building:
   - If you fetch 100 users with posts, how many database queries are executed?
   - Is there automatic query batching or DataLoader?
   - Can users request deeply nested queries that explode into 1000s of DB queries?

2. **Query Planning**: Are queries analyzed before execution? Can slow queries be rejected?

3. **Indexes**: What database indexes are assumed? Are they documented? Can performance degrade without them?

4. **Connection Pooling**: Review `fraiseql_rs/src/db/` - how many connections are pooled? What's the timeout?

### Caching
1. **Cache Strategy**: In `fraiseql_rs/src/cache/` - what's cached? TTL values?
2. **Cache Invalidation**: When data changes, how is the cache invalidated? Any race conditions?
3. **APQ Caching**: How does automatic persisted query caching work? Can it be exploited?
4. **Redis Dependency**: If Redis fails, what happens? Graceful degradation?

### Memory
1. **Large Result Sets**: If a query returns 1GB of data, what happens?
2. **Connection Memory**: How much memory per connection? At 10k connections, that's...?
3. **Mutation Buffering**: When executing mutations, is all data buffered in memory or streamed?
4. **Subscription Memory**: For subscriptions, how long are messages retained? Memory growth over time?

### Subscriptions/WebSocket
1. **Connection Limits**: Review `fraiseql_rs/src/subscriptions/` - is there a max connection limit? Per-user?
2. **Broadcast Efficiency**: When broadcasting an event, is it efficiently delivered to all subscribers?
3. **Memory Leaks**: Are closed connections properly cleaned up?
4. **Backpressure**: If a subscriber can't keep up, what happens? Is there a queue size limit?
5. **Event Loss**: If the event bus crashes, are events lost or replayed?

### Concurrency
1. **Lock Contention**: Are there any global locks that could bottleneck under high concurrency?
2. **Async Patterns**: Are async/await patterns used correctly? Any blocking calls in async contexts?
3. **Thread Safety**: Are all shared data structures properly synchronized?

---

## 🏗️ ARCHITECTURE QUESTIONS

### Design Decisions
1. **Python/Rust Split**: Why is some functionality in Python and some in Rust? What are the boundaries?
2. **FFI Safety**: Review `fraiseql_rs/src/` for PyO3 bindings - are there any unsafe blocks? Are they justified?
3. **Module Separation**: Are module dependencies clear? Any circular dependencies?

### Scalability
1. **Horizontal Scaling**: Can multiple instances be deployed? How is state shared?
2. **Database Scaling**: Is sharding supported? Multi-tenant database architecture?
3. **Bottlenecks**: What's the first thing that breaks under load? (database? Redis? memory? CPU?)
4. **Federation**: Can federated queries scale? What's the performance profile for joining entities from multiple services?

### Operational
1. **Health Checks**: Are there proper health checks? Readiness probes?
2. **Graceful Shutdown**: When shutting down, how are in-flight requests handled?
3. **Configuration**: How is the system configured? Environment variables? Config files?
4. **Monitoring**: What metrics are exported? Can operators detect issues before customers?
5. **Observability**: Is there distributed tracing? Logging? What's the log volume?

---

## 🧪 TESTING & RELIABILITY QUESTIONS

### Test Coverage
1. **Security Tests**: Are there tests for each security control? Auth/RBAC/rate limiting?
2. **Integration Tests**: Are multi-component flows tested end-to-end?
3. **Failure Modes**: Are tests written for failure scenarios? (database down, timeout, etc)
4. **Concurrency Tests**: Are there stress tests with many concurrent operations?
5. **Regression Tests**: For Issue #124 and other bug fixes, are there regression tests preventing recurrence?

### Error Handling
1. **Graceful Degradation**: When something fails (database, cache, external service), what happens?
2. **Retry Logic**: Are retries implemented? With exponential backoff?
3. **Timeouts**: Are all network operations properly timed out?
4. **Circuit Breakers**: Is there a circuit breaker pattern for external dependencies?
5. **Error Messages**: Do error messages leak information to attackers?

---

## 📊 SPECIFIC VULNERABILITY CHECKS

### Must Verify
1. **Multi-Tenancy Data Leak**: Write a test where User A queries another User B's data. Does it fail properly?
2. **RBAC Bypass - Field Level**:
   ```graphql
   # User doesn't have access to "salaryHistory" field
   query {
     user {
       salaryHistory {  # Should be denied
         amount
       }
     }
   }
   ```
   What happens? Is it denied at the resolver level or the serialization level?

3. **RBAC Bypass - Federation**:
   ```graphql
   # Reference another entity that shouldn't be accessible
   query {
     user {
       manager {  # User can see manager, but manager points to a restricted user
         email  # Should they see this?
       }
     }
   }
   ```

4. **N+1 Query Attack**:
   ```graphql
   query {
     users { # 100 users
       posts { # 50 posts each
         comments { # 100 comments each
           author { # Fetch author details
             id
           }
         }
       }
     }
   }
   ```
   How many DB queries? 5? 50? 500,000?

5. **Subscription Connection Bomb**:
   ```javascript
   for(let i = 0; i < 100000; i++) {
     ws.send(subscriptionQuery);
   }
   ```
   What happens? Memory exhaustion? Graceful rejection?

6. **Rate Limit Bypass**:
   - Via different IPs (from proxy)?
   - Via header manipulation?
   - Via different user accounts?

---

## 📋 CHECKLIST QUESTIONS

- [ ] Can multiple tenants be isolated in a single database instance without data leakage?
- [ ] Is RBAC enforced consistently (can't bypass via direct DB, federation, or API variations)?
- [ ] Are GraphQL queries limited by depth and complexity?
- [ ] Is there protection against N+1 query attacks?
- [ ] Are database queries parameterized (no SQL injection)?
- [ ] Is rate limiting enforced and not easily bypassed?
- [ ] Are WebSocket subscriptions limited and protected from connection bombs?
- [ ] Are secrets (API keys, passwords) never logged?
- [ ] Is there audit logging of sensitive operations?
- [ ] Can the system gracefully handle database/Redis failures?
- [ ] Are there health checks and readiness probes?
- [ ] Is the code suitable for production deployment to paying customers?
- [ ] Would you deploy this today with company data?
- [ ] What's the riskiest component?
- [ ] What would be impossible to change after users adopt it?

---

## HOW TO USE THIS DOCUMENT

**For Web Chat Review:**
Copy the above questions and paste into Claude with: "Use these targeted questions to review FraiseQL. Focus on security first, then performance, then architecture."

**For Specific Deep-Dives:**
Ask about specific sections:
- "Review security with focus on RBAC and multi-tenancy questions"
- "Assess performance focusing on the N+1 and subscription questions"
- "Is FraiseQL ready for production based on the checklist?"

**For Component Reviews:**
Ask separately:
- "Review the auth module based on the auth questions"
- "Review subscriptions for DoS vulnerabilities"
- "Review RBAC for bypass vectors"

---

**Version**: 1.0
**For FraiseQL**: v1.9.1
**Last Updated**: 2026-01-04
