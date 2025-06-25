# TICKET_002: Team Discussion - Apollo Sandbox GET Request Handling

**Status:** Active
**Priority:** High
**Assigned to:** Team Discussion Required
**Created:** 2025-01-24
**Updated:** 2025-01-24
**Related to:** apollo-sandbox-not-serving.md

## Discussion Topic

Determine the best approach for handling GET requests to `/graphql` when Apollo Sandbox is enabled.

## Current Situation

1. **Current Behavior:**
   - GET `/graphql` requires a `query` parameter
   - Without query parameter: returns 422 error
   - Apollo Sandbox is served at `/playground`
   - This differs from common GraphQL server behavior

2. **Standard GraphQL Server Behavior:**
   - GET `/graphql` without query → serves playground/sandbox UI
   - GET `/graphql` with query → executes GraphQL query
   - Single endpoint for both UI and API

## Options to Discuss

### Option 1: Modify GET Handler (Standard Behavior)
```python
# Serve playground HTML when no query provided
if query is None and config.enable_playground:
    return HTMLResponse(playground_html)
```
**Pros:**
- Follows GraphQL server conventions
- Single endpoint for everything
- Better developer experience

**Cons:**
- Changes existing behavior
- May break existing integrations expecting 422 error

### Option 2: Keep Current Behavior
- Keep `/graphql` for queries only
- Keep `/playground` for UI

**Pros:**
- No breaking changes
- Clear separation of concerns
- Explicit endpoints

**Cons:**
- Non-standard behavior
- Developers expect playground at `/graphql`

### Option 3: Dual Endpoint Support
- `/graphql` serves playground when no query (new behavior)
- Keep `/playground` for backward compatibility

**Pros:**
- Best of both worlds
- Gradual migration path
- No breaking changes

**Cons:**
- Two ways to access playground
- Slightly more complex

### Option 4: Configuration Option
```python
class FraiseQLConfig:
    serve_playground_on_graphql_get: bool = True  # New option
```

**Pros:**
- User choice
- Backward compatible with default
- Flexible

**Cons:**
- Another configuration option
- More complexity

## Technical Considerations

1. **Security:** 
   - Only affects development mode
   - Production mode doesn't serve GET requests

2. **Breaking Changes:**
   - Who relies on current 422 behavior?
   - Migration path needed?

3. **Documentation:**
   - Update docs for new behavior
   - Migration guide if needed

## Questions for Team

1. Are there any existing integrations that rely on the 422 error behavior?
2. Should we follow GraphQL server conventions or maintain our current pattern?
3. If we change, should we provide a migration period with both behaviors?
4. Do we need a configuration option or just pick one approach?

## Recommendation Needed

Please discuss and decide on the best approach. Consider:
- Developer experience
- Breaking changes
- Industry standards
- FraiseQL philosophy

## Recommendation: Option 1 with Environment-Aware Behavior

After analysis, **Option 1 (Modify GET Handler)** is recommended as the best long-term solution, with environment-specific behavior:

### Implementation Details

**Development/Staging Environment:**
```python
if request.method == "GET" and not query:
    if config.enable_playground and config.environment != "production":
        return HTMLResponse(playground_html)
```
- GET `/graphql` without query → serves playground UI
- GET `/graphql` with query → executes GraphQL query
- Introspection enabled
- Follows industry standard GraphQL server behavior

**Production Environment:**
```python
if request.method == "GET" and not query:
    return JSONResponse({"error": "Not found"}, status_code=404)
    # Or redirect to API documentation
```
- GET `/graphql` without query → returns 404 or redirects to docs
- GET `/graphql` with query → returns 405 (or executes if GET queries allowed)
- No playground served
- Introspection disabled (existing behavior)

### Why This Solution?

1. **Security First**: Maintains FraiseQL's security philosophy - no playground or introspection in production
2. **Developer Experience**: Follows GraphQL conventions in development where developers expect it
3. **Single Endpoint**: `/graphql` is THE GraphQL endpoint, reducing confusion
4. **Fixes Current Issue**: Playground automatically uses correct endpoint for queries
5. **Clean Architecture**: No need for separate `/playground` endpoint
6. **Environment Appropriate**: Development gets full features, production stays secure

### Migration Strategy

1. **Phase 1**: Implement new behavior with deprecation warning for `/playground`
2. **Phase 2**: Remove `/playground` endpoint in next major version
3. **Documentation**: Clear migration guide for users

This approach provides the best developer experience while maintaining production security, which aligns with FraiseQL's design principles.

## Action Items

- [ ] Schedule team discussion
- [ ] Review how other GraphQL servers handle this
- [ ] Check for existing user dependencies
- [ ] Make decision
- [ ] Implement chosen solution
- [ ] Update documentation