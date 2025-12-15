# Mutations API Reference

## CASCADE Field Selection

### Overview

The `cascade` field is available on Success types when `enable_cascade=True` is set on the mutation decorator.

CASCADE is only included in responses when explicitly requested in the GraphQL selection set.

### Schema Definition

```graphql
type Cascade {
  updated: [CascadeEntity!]!
  deleted: [CascadeEntity!]!
  invalidations: [CascadeInvalidation!]!
  metadata: CascadeMetadata!
}

type CascadeEntity {
  __typename: String!
  id: ID!
  operation: String!
  entity: JSON!
}

type CascadeInvalidation {
  queryName: String!
  strategy: String!
  scope: String!
}

type CascadeMetadata {
  timestamp: String!
  affectedCount: Int!
  depth: Int!
  transactionId: String
}
```

### Selection Examples

**Full CASCADE**:
```graphql
cascade {
  updated {
    __typename
    id
    operation
    entity
  }
  deleted {
    __typename
    id
  }
  invalidations {
    queryName
    strategy
    scope
  }
  metadata {
    timestamp
    affectedCount
    depth
    transactionId
  }
}
```

**Partial CASCADE** (metadata only):
```graphql
cascade {
  metadata {
    affectedCount
  }
}
```

**With Inline Fragments**:
```graphql
cascade {
  updated {
    __typename
    id
    operation
    entity {
      ... on Post {
        id
        title
      }
      ... on User {
        id
        name
      }
    }
  }
}
```

### Nullability

The `cascade` field is nullable:
- Returns `null` if no side effects occurred
- Not present in response if not requested in selection
- Returns object with requested fields if side effects occurred

### Performance Characteristics

| Selection | Payload Overhead | Use Case |
|-----------|-----------------|----------|
| Not requested | 0 bytes | Display-only mutations |
| metadata only | ~50-100 bytes | Count tracking |
| invalidations only | ~100-300 bytes | Cache clearing |
| updated only | ~500-2000 bytes | Entity sync |
| Full CASCADE | ~1000-5000 bytes | Complete sync |
