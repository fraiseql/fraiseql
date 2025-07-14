# Issue: GraphiQL fails to load due to MIME type blocking in v0.1.0a12

## Summary
GraphiQL playground fails to load when accessing the `/graphql` endpoint. The browser blocks the GraphiQL CSS and JS resources from unpkg.com due to incorrect MIME type (`text/plain` instead of `text/css` and `application/javascript`).

## Environment
- fraiseql version: 0.1.0a12
- Python version: 3.13
- Browser: Firefox
- OS: Linux (Arch)

## Steps to Reproduce
1. Install fraiseql==0.1.0a12
2. Run a FastAPI app with fraiseql GraphQL endpoint
3. Navigate to http://localhost:8000/graphql
4. Page shows "Loading..." indefinitely

## Expected Behavior
GraphiQL playground should load successfully, allowing users to interact with the GraphQL API.

## Actual Behavior
The page remains stuck on "Loading..." with the following console errors:

```
La ressource à l'adresse « https://unpkg.com/graphiql@5.0.0/graphiql.min.css » a été bloquée en raison d'un type MIME (« text/plain ») incorrect (X-Content-Type-Options: nosniff).
La ressource à l'adresse « https://unpkg.com/graphiql@5.0.0/graphiql.min.js » a été bloquée en raison d'un type MIME (« text/plain ») incorrect (X-Content-Type-Options: nosniff).
SES_UNCAUGHT_EXCEPTION: ReferenceError: GraphiQL is not defined
```

## Root Cause
The issue appears to be that unpkg.com is serving the GraphiQL assets with `Content-Type: text/plain` instead of the correct MIME types. Modern browsers with strict MIME type checking (X-Content-Type-Options: nosniff) block these resources.

## Current Code
In `fraiseql/fastapi/routers.py`, the GraphiQL resources are loaded as:
```html
<link rel="stylesheet" href="https://unpkg.com/graphiql/graphiql.min.css" />
<script src="https://unpkg.com/graphiql/graphiql.min.js" type="application/javascript"></script>
```

## Suggested Solutions

### Option 1: Use jsDelivr CDN instead of unpkg
jsDelivr properly serves files with correct MIME types:
```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphiql@3.0.10/graphiql.min.css" />
<script src="https://cdn.jsdelivr.net/npm/graphiql@3.0.10/graphiql.min.js"></script>
```

### Option 2: Bundle GraphiQL assets locally
Include GraphiQL assets directly in the fraiseql package to avoid CDN issues entirely.

### Option 3: Use Apollo Sandbox
Since Apollo Sandbox is mentioned in the v0.1.0a12 release notes as the solution, consider redirecting to Apollo Sandbox or embedding it instead of GraphiQL.

## Workaround
Users can access Apollo Sandbox directly at https://studio.apollographql.com/sandbox/explorer and connect to their local GraphQL endpoint.

## Additional Notes
- The GraphQL endpoint itself works correctly (verified with curl POST requests)
- This issue prevents developers from using the built-in GraphQL playground
- The issue was supposed to be fixed in v0.1.0a12 according to the release notes

## Related Information
- The deprecation warning about AsyncConnectionPool is unrelated but also appears in the logs
