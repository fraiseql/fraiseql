---
← [Backend Developer](backend-developer.md) | [Learning Paths](index.md) | [Next: Migrating](migrating.md) →
---

# Learning Path: Frontend Developer

> **For:** Frontend developers consuming GraphQL APIs
> **Time to complete:** 1.5-2 hours
> **Goal:** Master consuming FraiseQL APIs from frontend applications

As a frontend developer, you'll love FraiseQL's predictable GraphQL API, strong typing, and excellent developer experience. This path focuses on querying, mutations, error handling, and real-time features from the client perspective.

## Prerequisites

You should have:

- Experience with GraphQL clients (Apollo, urql, or graphql-request)
- Understanding of async JavaScript/TypeScript
- Basic knowledge of API consumption
- Familiarity with modern frontend frameworks

## Learning Journey

### 🎯 Phase 1: GraphQL Fundamentals (20 minutes)

Understand FraiseQL's GraphQL implementation:

1. **[GraphQL Playground](../getting-started/graphql-playground.md)** *(10 min)*

   - Interactive query testing
   - Schema exploration
   - Documentation browsing

2. **[Type System](../core-concepts/type-system.md)** *(10 min)*

   - Available scalar types
   - Custom types
   - Nullability and lists

### 📡 Phase 2: Querying Data (30 minutes)

Master data fetching patterns:

3. **[Query Examples](#query-examples)** *(15 min)*

   - Basic queries
   - Filtering and pagination
   - Nested relationships

4. **[Advanced Queries](#advanced-queries)** *(15 min)*

   - Fragments and variables
   - Aliases and directives
   - Batch queries

### ✏️ Phase 3: Mutations & Actions (30 minutes)

Learn to modify data:

5. **[Mutation Patterns](#mutation-patterns)** *(15 min)*

   - Creating records
   - Updating data
   - Deleting records

6. **[Error Handling](../errors/handling-patterns.md)** *(15 min)*

   - Error types
   - Client-side handling
   - Retry strategies

### 🔐 Phase 4: Authentication & Security (20 minutes)

Implement secure API access:

7. **[Authentication](../advanced/authentication.md)** *(10 min)*

   - Token management
   - Headers and cookies
   - Refresh patterns

8. **[Security Best Practices](../advanced/security.md)** *(10 min)*

   - CSRF protection
   - Rate limiting
   - Field-level permissions

## Query Examples

### Basic Queries

#### Simple List Query
```graphql
query GetPosts {
  posts(limit: 10) {
    id
    title
    content
    createdAt
    author {
      id
      name
      avatar
    }
  }
}
```

#### Single Item Query
```graphql
query GetPost($id: ID!) {
  post(id: $id) {
    id
    title
    content
    createdAt
    updatedAt
    author {
      id
      name
      email
    }
    comments {
      id
      content
      author {
        name
      }
      createdAt
    }
  }
}
```

#### Filtered Query
```graphql
query GetPublishedPosts($authorId: ID) {
  posts(
    where: {
      status: "published"
      authorId: $authorId
    }
    orderBy: { createdAt: DESC }
    limit: 20
  ) {
    id
    title
    excerpt
    publishedAt
  }
}
```

### Advanced Queries

#### Using Fragments
```graphql
fragment UserInfo on User {
  id
  name
  email
  avatar
}

fragment PostDetails on Post {
  id
  title
  content
  createdAt
  author {
    ...UserInfo
  }
}

query GetFeed {
  recentPosts: posts(limit: 5) {
    ...PostDetails
  }
  popularPosts: posts(
    orderBy: { viewCount: DESC }
    limit: 5
  ) {
    ...PostDetails
    viewCount
  }
}
```

#### Pagination with Cursors
```graphql
query GetPaginatedPosts(
  $cursor: String
  $limit: Int = 10
) {
  posts(
    after: $cursor
    first: $limit
  ) {
    edges {
      node {
        id
        title
        excerpt
      }
      cursor
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
    totalCount
  }
}
```

## Mutation Patterns

### Creating Records
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    success
    post {
      id
      title
      slug
      status
    }
    errors {
      field
      message
    }
  }
}
```

Variables:
```json
{
  "input": {
    "title": "My New Post",
    "content": "Post content here...",
    "tags": ["graphql", "fraiseql"],
    "status": "draft"
  }
}
```

### Updating Records
```graphql
mutation UpdatePost(
  $id: ID!
  $input: UpdatePostInput!
) {
  updatePost(id: $id, input: $input) {
    success
    post {
      id
      title
      content
      updatedAt
    }
    errors {
      field
      message
    }
  }
}
```

### Batch Operations
```graphql
mutation BatchUpdatePosts(
  $updates: [PostUpdate!]!
) {
  batchUpdatePosts(updates: $updates) {
    successful
    failed
    results {
      id
      success
      error
    }
  }
}
```

## Client Integration Examples

### Apollo Client (React)

```typescript
import {
  ApolloClient,
  InMemoryCache,
  createHttpLink
} from '@apollo/client';
import { setContext } from '@apollo/client/link/context';

// Configure Apollo Client
const httpLink = createHttpLink({
  uri: 'http://localhost:8000/graphql',
});

const authLink = setContext((_, { headers }) => {
  const token = localStorage.getItem('token');
  return {
    headers: {
      ...headers,
      authorization: token ? `Bearer ${token}` : "",
    }
  };
});

const client = new ApolloClient({
  link: authLink.concat(httpLink),
  cache: new InMemoryCache(),
  defaultOptions: {
    watchQuery: {
      fetchPolicy: 'cache-and-network',
    },
  },
});

// React Component
import { useQuery, useMutation, gql } from '@apollo/client';

const GET_POSTS = gql`
  query GetPosts($limit: Int) {
    posts(limit: $limit) {
      id
      title
      excerpt
      author {
        name
      }
    }
  }
`;

function PostList() {
  const { loading, error, data, refetch } = useQuery(
    GET_POSTS,
    { variables: { limit: 10 } }
  );

  if (loading) return <Loading />;
  if (error) return <Error error={error} />;

  return (
    <div>
      {data.posts.map(post => (
        <PostCard key={post.id} post={post} />
      ))}
    </div>
  );
}
```

### urql (Vue.js)

```typescript
import {
  createClient,
  fetchExchange,
  cacheExchange
} from '@urql/core';

// Configure urql client
const client = createClient({
  url: 'http://localhost:8000/graphql',
  exchanges: [cacheExchange, fetchExchange],
  fetchOptions: () => {
    const token = localStorage.getItem('token');
    return {
      headers: {
        authorization: token ? `Bearer ${token}` : ''
      },
    };
  },
});

// Vue Component
<template>
  <div v-if="fetching">Loading...</div>
  <div v-else-if="error">Error: {{ error.message }}</div>
  <div v-else>
    <post-card
      v-for="post in data.posts"
      :key="post.id"
      :post="post"
    />
  </div>
</template>

<script setup>
import { useQuery } from '@urql/vue';

const { fetching, error, data } = useQuery({
  query: `
    query GetPosts($limit: Int) {
      posts(limit: $limit) {
        id
        title
        excerpt
        author {
          name
        }
      }
    }
  `,
  variables: { limit: 10 },
});
</script>
```

### graphql-request (Next.js)

```typescript
import { GraphQLClient, gql } from 'graphql-request';

// Configure client
const client = new GraphQLClient(
  'http://localhost:8000/graphql',
  {
    headers: {
      authorization: `Bearer ${process.env.API_TOKEN}`,
    },
  }
);

// Server Component (Next.js 13+)
export default async function PostsPage() {
  const query = gql`
    query GetPosts {
      posts(limit: 10) {
        id
        title
        excerpt
        author {
          name
        }
      }
    }
  `;

  const data = await client.request(query);

  return (
    <div>
      {data.posts.map(post => (
        <PostCard key={post.id} post={post} />
      ))}
    </div>
  );
}

// With SWR for client-side
import useSWR from 'swr';

const fetcher = (query) => client.request(query);

export function usePost(id: string) {
  const { data, error, mutate } = useSWR(
    gql`
      query GetPost($id: ID!) {
        post(id: $id) {
          id
          title
          content
        }
      }
    `,
    (query) => fetcher(query, { id })
  );

  return {
    post: data?.post,
    isLoading: !error && !data,
    isError: error,
    mutate,
  };
}
```

## Error Handling Patterns

### Structured Error Response
```typescript
interface GraphQLError {
  message: string;
  extensions?: {
    code: string;
    field?: string;
    details?: Record<string, any>;
  };
}

// Handle errors gracefully
function handleGraphQLError(error: GraphQLError) {
  switch (error.extensions?.code) {
    case 'UNAUTHENTICATED':
      // Redirect to login
      router.push('/login');
      break;

    case 'FORBIDDEN':
      // Show permission error
      toast.error('You don\'t have permission');
      break;

    case 'VALIDATION_ERROR':
      // Show field errors
      const fieldErrors = error.extensions.details;
      Object.entries(fieldErrors).forEach(
        ([field, message]) => {
          form.setError(field, message);
        }
      );
      break;

    case 'RATE_LIMITED':
      // Show rate limit message
      const retryAfter = error.extensions.details.retryAfter;
      toast.warning(
        `Too many requests. Retry in ${retryAfter}s`
      );
      break;

    default:
      // Generic error handling
      toast.error(error.message);
  }
}
```

### Optimistic Updates
```typescript
const [updatePost] = useMutation(UPDATE_POST, {
  optimisticResponse: {
    updatePost: {
      __typename: 'UpdatePostPayload',
      success: true,
      post: {
        __typename: 'Post',
        id: postId,
        title: newTitle,
        updatedAt: new Date().toISOString(),
      },
    },
  },
  update: (cache, { data }) => {
    if (data?.updatePost?.success) {
      // Update cache with new data
      cache.modify({
        id: cache.identify({
          __typename: 'Post',
          id: postId
        }),
        fields: {
          title: () => newTitle,
          updatedAt: () => data.updatePost.post.updatedAt,
        },
      });
    }
  },
});
```

## Real-time Features

### Subscriptions (WebSocket)
```typescript
import {
  ApolloClient,
  split,
  HttpLink
} from '@apollo/client';
import {
  GraphQLWsLink
} from '@apollo/client/link/subscriptions';
import { createClient } from 'graphql-ws';

// WebSocket link for subscriptions
const wsLink = new GraphQLWsLink(
  createClient({
    url: 'ws://localhost:8000/graphql',
    connectionParams: {
      authentication: localStorage.getItem('token'),
    },
  })
);

// HTTP link for queries and mutations
const httpLink = new HttpLink({
  uri: 'http://localhost:8000/graphql',
});

// Split based on operation type
const splitLink = split(
  ({ query }) => {
    const definition = getMainDefinition(query);
    return (
      definition.kind === 'OperationDefinition' &&
      definition.operation === 'subscription'
    );
  },
  wsLink,
  httpLink
);

// Use in component
const COMMENT_SUBSCRIPTION = gql`
  subscription OnCommentAdded($postId: ID!) {
    commentAdded(postId: $postId) {
      id
      content
      author {
        name
      }
      createdAt
    }
  }
`;

function PostComments({ postId }) {
  const { data, loading } = useSubscription(
    COMMENT_SUBSCRIPTION,
    { variables: { postId } }
  );

  // Handle real-time updates
  useEffect(() => {
    if (data?.commentAdded) {
      // Update UI with new comment
      addComment(data.commentAdded);
    }
  }, [data]);
}
```

## TypeScript Integration

### Generate Types from Schema
```bash
# Install GraphQL Code Generator
npm install -D @graphql-codegen/cli \
  @graphql-codegen/typescript \
  @graphql-codegen/typescript-operations

# codegen.yml
overwrite: true
schema: "http://localhost:8000/graphql"
documents: "src/**/*.graphql"
generates:
  src/generated/graphql.ts:
    plugins:

      - typescript
      - typescript-operations
    config:
      withHooks: true
      withComponent: false
      withHOC: false
```

### Use Generated Types
```typescript
import {
  GetPostsQuery,
  GetPostsQueryVariables
} from './generated/graphql';

const { data, loading, error } = useQuery<
  GetPostsQuery,
  GetPostsQueryVariables
>(GET_POSTS, {
  variables: { limit: 10 },
});

// Full type safety
data?.posts.map(post => {
  console.log(post.title); // TypeScript knows the shape
});
```

## Performance Optimization

### Query Batching
```typescript
import { BatchHttpLink } from '@apollo/client/link/batch-http';

const batchLink = new BatchHttpLink({
  uri: 'http://localhost:8000/graphql',
  batchMax: 5, // Max queries per batch
  batchInterval: 20, // Ms to wait before batching
});
```

### Persisted Queries
```typescript
import { createPersistedQueryLink } from '@apollo/client/link/persisted-queries';
import { sha256 } from 'crypto-hash';

const persistedQueriesLink = createPersistedQueryLink({
  sha256,
  useGETForHashedQueries: true, // Use GET for CDN caching
});
```

### Cache Management
```typescript
const cache = new InMemoryCache({
  typePolicies: {
    Query: {
      fields: {
        posts: {
          // Pagination handling
          keyArgs: ['where', 'orderBy'],
          merge(existing = [], incoming) {
            return [...existing, ...incoming];
          },
        },
      },
    },
    Post: {
      fields: {
        // Computed field
        isNew: {
          read(_, { readField }) {
            const createdAt = readField('createdAt');
            const hourAgo = Date.now() - 3600000;
            return new Date(createdAt) > hourAgo;
          },
        },
      },
    },
  },
});
```

## Testing Strategies

### Mock GraphQL Responses
```typescript
import { MockedProvider } from '@apollo/client/testing';

const mocks = [
  {
    request: {
      query: GET_POSTS,
      variables: { limit: 10 },
    },
    result: {
      data: {
        posts: [
          { id: '1', title: 'Test Post', excerpt: 'Test' },
        ],
      },
    },
  },
];

describe('PostList', () => {
  it('renders posts', async () => {
    render(
      <MockedProvider mocks={mocks}>
        <PostList />
      </MockedProvider>
    );

    await waitFor(() => {
      expect(screen.getByText('Test Post')).toBeInTheDocument();
    });
  });
});
```

## Best Practices

### 1. Query Only What You Need
```graphql
# ❌ Over-fetching
query GetPost($id: ID!) {
  post(id: $id) {
    id
    title
    content
    author {
      id
      name
      email
      bio
      avatar
      posts {
        id
        title
        # ... more nested data
      }
    }
  }
}

# ✅ Fetch only required fields
query GetPost($id: ID!) {
  post(id: $id) {
    id
    title
    content
    author {
      name
      avatar
    }
  }
}
```

### 2. Use Fragments for Reusability
```graphql
fragment PostPreview on Post {
  id
  title
  excerpt
  author {
    name
  }
  createdAt
}

# Reuse in multiple queries
query GetHomeFeed {
  recentPosts: posts(limit: 5) {
    ...PostPreview
  }
  popularPosts: posts(orderBy: { likes: DESC }) {
    ...PostPreview
    likeCount
  }
}
```

### 3. Handle Loading States
```typescript
function PostList() {
  const { data, loading, error, fetchMore } = useQuery(GET_POSTS);

  if (loading && !data) return <Skeleton />;
  if (error) return <ErrorBoundary error={error} />;

  return (
    <>
      {data?.posts.map(post => (
        <PostCard key={post.id} post={post} />
      ))}
      {loading && <LoadingMore />}
    </>
  );
}
```

## Next Steps

### Continue Learning

- **[Backend Developer Path](backend-developer.md)** - Understand the API internals
- **[Migration Path](migrating.md)** - Migrate from REST or other GraphQL servers

### Advanced Topics

- **[Caching Strategies](../advanced/lazy-caching.md)** - Server-side caching
- **[TurboRouter](../advanced/turbo-router.md)** - Performance optimization
- **[Rate Limiting](../advanced/security.md#rate-limiting)** - API protection

### Tools & Resources

- [GraphQL Playground](../getting-started/graphql-playground.md) - Interactive testing
- [Apollo DevTools](https://www.apollographql.com/docs/react/development-testing/developer-tools/) - Browser extension
- [GraphQL Code Generator](https://graphql-code-generator.com/) - Type generation

## Tips for Frontend Success

💡 **Use fragments** - DRY principle for queries
💡 **Cache wisely** - Understand cache normalization
💡 **Handle errors** - Users need feedback
💡 **Optimize bundles** - Tree-shake unused queries
💡 **Test thoroughly** - Mock API responses

Congratulations! You now have the skills to build robust frontend applications with FraiseQL GraphQL APIs.
