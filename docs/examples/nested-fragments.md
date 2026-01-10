# Nested Fragments Examples

This guide demonstrates how to use GraphQL fragments in nested selections within FraiseQL queries.

## Basic Nested Fragment

```graphql
fragment UserFields on User {
  id
  name
}

query GetPostsWithAuthors {
  posts {
    id
    title
    author {
      ...UserFields
      email
    }
  }
}
```

**Expected Result:**
```json
{
  "data": {
    "posts": [
      {
        "id": "1",
        "title": "First Post",
        "author": {
          "id": "101",
          "name": "Alice",
          "email": "alice@example.com"
        }
      }
    ]
  }
}
```

## Multiple Nested Fragments

```graphql
fragment BasicUser on User {
  id
  name
}

fragment ContactInfo on User {
  email
  phone
}

query GetPostsWithFullAuthor {
  posts {
    id
    title
    author {
      ...BasicUser
      ...ContactInfo
    }
  }
}
```

## Deeply Nested Fragments

```graphql
fragment ProfileFields on Profile {
  bio
  website
}

fragment UserFields on User {
  id
  name
  profile {
    ...ProfileFields
  }
}

query GetPostsWithProfiles {
  posts {
    id
    title
    author {
      ...UserFields
      email
    }
  }
}
```

**Expected Result:**
```json
{
  "data": {
    "posts": [
      {
        "id": "1",
        "title": "Tech Blog",
        "author": {
          "id": "101",
          "name": "Alice",
          "profile": {
            "bio": "Software Engineer",
            "website": "alice.dev"
          },
          "email": "alice@example.com"
        }
      }
    ]
  }
}
```

## Fragments with Field Aliases

```graphql
fragment UserFields on User {
  id
  name
}

query GetPostsWithAliasedAuthor {
  posts {
    id
    title
    writer: author {
      ...UserFields
      contactEmail: email
    }
  }
}
```

## Fragments with Directives

```graphql
fragment UserFields on User {
  id
  name
}

query GetPostsWithConditionalAuthor($includeAuthor: Boolean = true) {
  posts {
    id
    title
    author @include(if: $includeAuthor) {
      ...UserFields
    }
  }
}
```

## Inline Fragments in Nested Context

```graphql
query GetPostsWithTypedAuthor {
  posts {
    id
    title
    author {
      id
      name
      ... on User {
        email
        role
      }
    }
  }
}
```

## Best Practices

1. **Reusable Fragments**: Define fragments for commonly used field sets
2. **Type Safety**: Ensure fragment types match the GraphQL schema
3. **Performance**: Fragments don't add query overhead in FraiseQL
4. **Avoid Cycles**: Don't create circular fragment references (see [fragment cycles guide](fragment-cycles.md))

## Error Handling

Fragments in nested selections follow the same validation rules as root-level fragments. See the [fragment cycles guide](fragment-cycles.md) for information about cycle detection and error handling.
