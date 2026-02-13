<!-- Skip to main content -->
---

title: GraphQL API Specification
description: This document specifies the GraphQL API provided by the FraiseQL HTTP server. The API follows the official GraphQL specification with additional validation and
keywords: ["directives", "types", "scalars", "schema", "graphql", "api"]
tags: ["documentation", "reference"]
---

# GraphQL API Specification

## Overview

This document specifies the GraphQL API provided by the FraiseQL HTTP server. The API follows the official GraphQL specification with additional validation and security features.

## Request Protocol

### HTTP Method

All GraphQL operations use **POST** to `/graphql` endpoint.

```text
<!-- Code example in TEXT -->
POST /graphql HTTP/1.1
Host: api.example.com
Content-Type: application/json
```text
<!-- Code example in TEXT -->

### Request Body

```json
<!-- Code example in JSON -->
{
  "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
  "variables": {
    "id": "123"
  },
  "operationName": "GetUser"
}
```text
<!-- Code example in TEXT -->

**Fields:**

- `query` (string, required): GraphQL query document
- `variables` (object, optional): Variable values as JSON object
- `operationName` (string, optional): Operation name to execute

### Request Validation

Before execution, the server validates:

1. **Syntax**: Query is valid GraphQL
2. **Depth**: Query nesting doesn't exceed limit (default: 10)
3. **Complexity**: Query score doesn't exceed limit (default: 100)
4. **Variables**: Variables are a JSON object if provided

Validation errors return HTTP 400 with error details.

## Response Protocol

### Success Response

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "123",
      "name": "John Doe",
      "email": "john@example.com"
    }
  }
}
```text
<!-- Code example in TEXT -->

**Fields:**

- `data`: Query result (object) or null if errors occurred during parsing
- `errors`: Array of errors (omitted if no errors)

### Error Response

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "User not found",
      "code": "NOT_FOUND",
      "locations": [
        {
          "line": 1,
          "column": 18
        }
      ],
      "path": ["user"],
      "extensions": {
        "category": "EXECUTION",
        "status": 404,
        "request_id": "req-abc123"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Partial Success Response

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "123",
      "name": "John Doe",
      "email": null
    }
  },
  "errors": [
    {
      "message": "Access denied to email field",
      "code": "FORBIDDEN",
      "path": ["user", "email"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

**Note**: When errors occur for nullable fields, `data` is still returned with null for error fields.

## Query Language

### Basic Query

```graphql
<!-- Code example in GraphQL -->
query {
  users {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

### Query with Arguments

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

### Query with Variables

```graphql
<!-- Code example in GraphQL -->
query GetUser($userId: ID!) {
  user(id: $userId) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

**Variables**:

```json
<!-- Code example in JSON -->
{
  "userId": "123"
}
```text
<!-- Code example in TEXT -->

### Query with Aliases

```graphql
<!-- Code example in GraphQL -->
query {
  currentUser: user(id: "123") {
    id
    name
  }
  adminUser: user(id: "456") {
    id
    name
  }
}
```text
<!-- Code example in TEXT -->

### Query with Fragments

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    ...userFields
  }
}

fragment userFields on User {
  id
  name
  email
  profile {
    bio
    avatar
  }
}
```text
<!-- Code example in TEXT -->

### Nested Queries

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    posts {
      id
      title
      content
      author {
        id
        name
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

## Data Types

### Scalar Types

| Type | Description | Example |
|------|-------------|---------|
| `Int` | 32-bit integer | `42` |
| `Float` | Floating point number | `3.14` |
| `String` | Text string | `"Hello"` |
| `Boolean` | True/false value | `true` |
| `ID` | Unique identifier | `"user-123"` |

### Type Modifiers

| Notation | Meaning | Example |
|----------|---------|---------|
| `Type!` | Non-nullable | `String!` (required) |
| `[Type]` | List of type | `[String]` (array) |
| `[Type!]!` | Required list of required items | `[String!]!` |

### Examples

```graphql
<!-- Code example in GraphQL -->
# Nullable string (can be null or string)
name: String

# Required string (cannot be null)
name: String!

# List of strings
tags: [String]

# Required list of required strings
tags: [String!]!
```text
<!-- Code example in TEXT -->

## Mutations

Mutations modify data on the server. They follow the same syntax as queries but use the `mutation` keyword.

### Simple Mutation

```graphql
<!-- Code example in GraphQL -->
mutation {
  createUser(input: {name: "Jane", email: "jane@example.com"}) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

### Mutation with Input Variables

```graphql
<!-- Code example in GraphQL -->
mutation CreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

**Variables**:

```json
<!-- Code example in JSON -->
{
  "input": {
    "name": "Jane",
    "email": "jane@example.com"
  }
}
```text
<!-- Code example in TEXT -->

### Multiple Mutations

```graphql
<!-- Code example in GraphQL -->
mutation {
  user1: createUser(input: {name: "User1"}) {
    id
  }
  user2: createUser(input: {name: "User2"}) {
    id
  }
}
```text
<!-- Code example in TEXT -->

## Error Handling

### Validation Errors

Occur before execution (query syntax, depth, complexity, variables).

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Query exceeds maximum depth of 10: depth = 15",
      "code": "VALIDATION_ERROR"
    }
  ]
}
```text
<!-- Code example in TEXT -->

HTTP Status: **400 Bad Request**

### Parse Errors

Occur when query syntax is invalid.

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Malformed GraphQL query: unexpected token '}'",
      "code": "PARSE_ERROR"
    }
  ]
}
```text
<!-- Code example in TEXT -->

HTTP Status: **400 Bad Request**

### Execution Errors

Occur during query execution (field resolution, database errors).

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": null
  },
  "errors": [
    {
      "message": "User not found",
      "code": "NOT_FOUND",
      "path": ["user"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

HTTP Status: **200 OK** (even with errors in data)

### Database Errors

```json
<!-- Code example in JSON -->
{
  "errors": [
    {
      "message": "Database connection failed",
      "code": "DATABASE_ERROR"
    }
  ]
}
```text
<!-- Code example in TEXT -->

HTTP Status: **500 Internal Server Error**

## Introspection

Query the schema to discover available types and fields.

### Full Schema Introspection

```graphql
<!-- Code example in GraphQL -->
query {
  __schema {
    types {
      name
      kind
      fields {
        name
        type {
          name
          kind
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### Specific Type Introspection

```graphql
<!-- Code example in GraphQL -->
query {
  __type(name: "User") {
    name
    kind
    fields {
      name
      type {
        name
        kind
        ofType {
          name
          kind
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

### Available Fields

**Type**:

- `name`: Type name
- `kind`: OBJECT, INTERFACE, ENUM, SCALAR, UNION, INPUT_OBJECT
- `description`: Type documentation
- `fields`: Array of Field objects
- `enumValues`: Values for ENUM types
- `interfaces`: Interfaces this type implements
- `possibleTypes`: Possible types for INTERFACE/UNION

**Field**:

- `name`: Field name
- `type`: Field type
- `args`: Arguments this field accepts
- `isDeprecated`: Whether field is deprecated
- `deprecationReason`: Reason for deprecation (if deprecated)
- `description`: Field documentation

## Best Practices

### 1. Use Query Variables

❌ **Bad** (string interpolation):

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    name
  }
}
```text
<!-- Code example in TEXT -->

✅ **Good** (variables):

```graphql
<!-- Code example in GraphQL -->
query GetUser($id: ID!) {
  user(id: $id) {
    name
  }
}
```text
<!-- Code example in TEXT -->

**Why**: Security (prevents injection), reusability, caching.

### 2. Request Only Needed Fields

❌ **Bad** (overfetch):

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    email
    phone
    address
    ssn
    salaryHistory
  }
}
```text
<!-- Code example in TEXT -->

✅ **Good** (exact fields):

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

**Why**: Reduces bandwidth, faster execution, security.

### 3. Use Fragments for Reuse

❌ **Bad** (repetition):

```graphql
<!-- Code example in GraphQL -->
query {
  user1: user(id: "1") {
    id
    name
    email
  }
  user2: user(id: "2") {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

✅ **Good** (fragments):

```graphql
<!-- Code example in GraphQL -->
query {
  user1: user(id: "1") {
    ...userFields
  }
  user2: user(id: "2") {
    ...userFields
  }
}

fragment userFields on User {
  id
  name
  email
}
```text
<!-- Code example in TEXT -->

### 4. Handle Nullable Fields

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    email        # Could be null
    phone        # Could be null
  }
}
```text
<!-- Code example in TEXT -->

Always check for null in client code:

```javascript
<!-- Code example in JAVASCRIPT -->
const user = data.user;
if (user.email) {
  // Use email
} else {
  // Handle missing email
}
```text
<!-- Code example in TEXT -->

### 5. Understand Rate Limits

The server enforces query limits to prevent abuse:

- **Query Depth**: Maximum 10 levels of nesting
- **Query Complexity**: Maximum 100 complexity points

Optimize complex queries by:

- Reducing nesting depth
- Using pagination for lists
- Requesting fewer fields
- Using filtering to reduce result sets

### 6. Use Meaningful Operation Names

```graphql
<!-- Code example in GraphQL -->
# ✅ Good
query GetUserProfile($id: ID!) {
  user(id: $id) {
    ...
  }
}

# ❌ Bad
query($id: ID!) {
  user(id: $id) {
    ...
  }
}
```text
<!-- Code example in TEXT -->

**Why**: Better logging, easier debugging, improved monitoring.

## Performance Tips

### 1. Batch Requests

❌ **Inefficient** (3 requests):

```text
<!-- Code example in TEXT -->
POST /graphql { query: GetUser($id: "1") ... }
POST /graphql { query: GetUser($id: "2") ... }
POST /graphql { query: GetUser($id: "3") ... }
```text
<!-- Code example in TEXT -->

✅ **Efficient** (1 request):

```graphql
<!-- Code example in GraphQL -->
query {
  user1: user(id: "1") { ... }
  user2: user(id: "2") { ... }
  user3: user(id: "3") { ... }
}
```text
<!-- Code example in TEXT -->

### 2. Use Pagination

```graphql
<!-- Code example in GraphQL -->
query {
  users(first: 10, after: "cursor-123") {
    edges {
      node {
        id
        name
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```text
<!-- Code example in TEXT -->

### 3. Cache Queries

Store frequently used queries in client:

```javascript
<!-- Code example in JAVASCRIPT -->
const GET_USER_PROFILE = `
  query GetUserProfile($id: ID!) {
    user(id: $id) {
      id
      name
      email
    }
  }
`;

// Reuse across requests
execute({ query: GET_USER_PROFILE, variables: { id } });
```text
<!-- Code example in TEXT -->

### 4. Monitor Query Complexity

Use `/introspection` to understand schema structure and optimize queries accordingly.

## Examples

See [examples/](../../../examples/) for complete working examples with various query patterns and edge cases.
