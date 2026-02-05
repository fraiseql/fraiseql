# GraphQL API Specification

## Overview

This document specifies the GraphQL API provided by the FraiseQL HTTP server. The API follows the official GraphQL specification with additional validation and security features.

## Request Protocol

### HTTP Method

All GraphQL operations use **POST** to `/graphql` endpoint.

```text
POST /graphql HTTP/1.1
Host: api.example.com
Content-Type: application/json
```text

### Request Body

```json
{
  "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
  "variables": {
    "id": "123"
  },
  "operationName": "GetUser"
}
```text

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

**Fields:**

- `data`: Query result (object) or null if errors occurred during parsing
- `errors`: Array of errors (omitted if no errors)

### Error Response

```json
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

### Partial Success Response

```json
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

**Note**: When errors occur for nullable fields, `data` is still returned with null for error fields.

## Query Language

### Basic Query

```graphql
query {
  users {
    id
    name
    email
  }
}
```text

### Query with Arguments

```graphql
query {
  user(id: "123") {
    id
    name
    email
  }
}
```text

### Query with Variables

```graphql
query GetUser($userId: ID!) {
  user(id: $userId) {
    id
    name
    email
  }
}
```text

**Variables**:

```json
{
  "userId": "123"
}
```text

### Query with Aliases

```graphql
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

### Query with Fragments

```graphql
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

### Nested Queries

```graphql
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
# Nullable string (can be null or string)
name: String

# Required string (cannot be null)
name: String!

# List of strings
tags: [String]

# Required list of required strings
tags: [String!]!
```text

## Mutations

Mutations modify data on the server. They follow the same syntax as queries but use the `mutation` keyword.

### Simple Mutation

```graphql
mutation {
  createUser(input: {name: "Jane", email: "jane@example.com"}) {
    id
    name
    email
  }
}
```text

### Mutation with Input Variables

```graphql
mutation CreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    id
    name
    email
  }
}
```text

**Variables**:

```json
{
  "input": {
    "name": "Jane",
    "email": "jane@example.com"
  }
}
```text

### Multiple Mutations

```graphql
mutation {
  user1: createUser(input: {name: "User1"}) {
    id
  }
  user2: createUser(input: {name: "User2"}) {
    id
  }
}
```text

## Error Handling

### Validation Errors

Occur before execution (query syntax, depth, complexity, variables).

```json
{
  "errors": [
    {
      "message": "Query exceeds maximum depth of 10: depth = 15",
      "code": "VALIDATION_ERROR"
    }
  ]
}
```text

HTTP Status: **400 Bad Request**

### Parse Errors

Occur when query syntax is invalid.

```json
{
  "errors": [
    {
      "message": "Malformed GraphQL query: unexpected token '}'",
      "code": "PARSE_ERROR"
    }
  ]
}
```text

HTTP Status: **400 Bad Request**

### Execution Errors

Occur during query execution (field resolution, database errors).

```json
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

HTTP Status: **200 OK** (even with errors in data)

### Database Errors

```json
{
  "errors": [
    {
      "message": "Database connection failed",
      "code": "DATABASE_ERROR"
    }
  ]
}
```text

HTTP Status: **500 Internal Server Error**

## Introspection

Query the schema to discover available types and fields.

### Full Schema Introspection

```graphql
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

### Specific Type Introspection

```graphql
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
query {
  user(id: "123") {
    name
  }
}
```text

✅ **Good** (variables):

```graphql
query GetUser($id: ID!) {
  user(id: $id) {
    name
  }
}
```text

**Why**: Security (prevents injection), reusability, caching.

### 2. Request Only Needed Fields

❌ **Bad** (overfetch):

```graphql
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

✅ **Good** (exact fields):

```graphql
query {
  user(id: "123") {
    id
    name
    email
  }
}
```text

**Why**: Reduces bandwidth, faster execution, security.

### 3. Use Fragments for Reuse

❌ **Bad** (repetition):

```graphql
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

✅ **Good** (fragments):

```graphql
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

### 4. Handle Nullable Fields

```graphql
query {
  user(id: "123") {
    id
    name
    email        # Could be null
    phone        # Could be null
  }
}
```text

Always check for null in client code:

```javascript
const user = data.user;
if (user.email) {
  // Use email
} else {
  // Handle missing email
}
```text

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

**Why**: Better logging, easier debugging, improved monitoring.

## Performance Tips

### 1. Batch Requests

❌ **Inefficient** (3 requests):

```text
POST /graphql { query: GetUser($id: "1") ... }
POST /graphql { query: GetUser($id: "2") ... }
POST /graphql { query: GetUser($id: "3") ... }
```text

✅ **Efficient** (1 request):

```graphql
query {
  user1: user(id: "1") { ... }
  user2: user(id: "2") { ... }
  user3: user(id: "3") { ... }
}
```text

### 2. Use Pagination

```graphql
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

### 3. Cache Queries

Store frequently used queries in client:

```javascript
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

### 4. Monitor Query Complexity

Use `/introspection` to understand schema structure and optimize queries accordingly.

## Examples

See [examples/](../../../examples/) for complete working examples with various query patterns and edge cases.
