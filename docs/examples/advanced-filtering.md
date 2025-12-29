# Advanced Filtering Examples

This guide provides practical, real-world examples of using FraiseQL's advanced PostgreSQL filter operators. Each example includes the complete GraphQL query, generated SQL, and explanations.

## Table of Contents

- [E-commerce Product Catalog](#e-commerce-product-catalog)
- [Content Management System](#content-management-system)
- [User Management & Permissions](#user-management--permissions)
- [Log Analysis & Monitoring](#log-analysis--monitoring)
- [Multi-tenant SaaS Application](#multi-tenant-saas-application)

---

## E-commerce Product Catalog

### Example 1: Smart Product Search with Filters

**Scenario**: Customer searches for "gaming laptop" with price range, in-stock only, and specific features.

**Database Schema**:
```sql
CREATE TABLE tb_product (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL,
    sku TEXT UNIQUE NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    attributes JSONB NOT NULL DEFAULT '{}',
    search_vector TSVECTOR NOT NULL
);

-- Indexes for performance
CREATE INDEX idx_product_tags ON tb_product USING gin(tags);
CREATE INDEX idx_product_attrs ON tb_product USING gin(attributes);
CREATE INDEX idx_product_search ON tb_product USING gin(search_vector);
CREATE INDEX idx_product_price ON tb_product (price);

-- View for GraphQL
CREATE VIEW v_product AS
SELECT
    id,
    name,
    price,
    sku,
    tags,
    attributes,
    search_vector,
    jsonb_build_object(
        'id', id,
        'name', name,
        'description', description,
        'price', price,
        'sku', sku,
        'tags', tags,
        'attributes', attributes
    ) as data
FROM tb_product;
```

**GraphQL Query**:
```graphql
query SearchGamingLaptops {
  products(
    where: {
      AND: [
        # Full-text search with relevance threshold
        {
          searchVector: {
            websearch_query: "gaming laptop",
            rank_gt: 0.1
          }
        },
        # Must have gaming-related tags
        {
          tags: {
            overlaps: ["gaming", "laptop", "high-performance"]
          }
        },
        # Price range
        {
          price: {
            gte: 800,
            lte: 2000
          }
        },
        # Must be in stock
        {
          attributes: {
            contains: { inStock: true }
          }
        },
        # Must have GPU info
        {
          attributes: {
            has_key: "gpu"
          }
        }
      ]
    },
    limit: 20
  ) {
    id
    name
    price
    tags
    attributes
  }
}
```

**Generated SQL** (simplified):
```sql
SELECT data
FROM v_product
WHERE (
    search_vector @@ websearch_to_tsquery('english', 'gaming laptop')
    AND ts_rank(search_vector, websearch_to_tsquery('english', 'gaming laptop')) > 0.1
)
AND tags && ARRAY['gaming', 'laptop', 'high-performance']::text[]
AND price >= 800
AND price <= 2000
AND attributes @> '{"inStock": true}'::jsonb
AND attributes ? 'gpu'
LIMIT 20;
```

**Result**: High-relevance gaming laptops within budget, in-stock, with GPU specifications.

---

### Example 2: Find Products by Similar Tags (Recommendation)

**Scenario**: Given a product with tags `["electronics", "smartphone", "5G"]`, find similar products.

**GraphQL Query**:
```graphql
query SimilarProducts($productTags: [String!]!) {
  products(
    where: {
      AND: [
        # Must share at least 2 tags
        {
          tags: {
            overlaps: $productTags
          }
        },
        # But exclude exact match (the current product)
        {
          tags: {
            neq: $productTags
          }
        },
        # Must have minimum number of tags (quality signal)
        {
          tags: {
            len_gte: 2
          }
        }
      ]
    },
    limit: 10
  ) {
    id
    name
    tags
  }
}
```

**Variables**:
```json
{
  "productTags": ["electronics", "smartphone", "5G"]
}
```

**Use Case**: Product recommendation engine, "Similar Products" section.

---

### Example 3: Validate Product SKU Format

**Scenario**: Find products with invalid SKU codes (should be `PROD-XXXX` where X is digit).

**GraphQL Query**:
```graphql
query InvalidProducts {
  products(
    where: {
      sku: {
        not_matches: "^PROD-[0-9]{4}$"
      }
    }
  ) {
    id
    sku
    name
  }
}
```

**Use Case**: Data quality audit, find products needing SKU correction.

---

## Content Management System

### Example 1: Blog Post Search with Multi-Field Matching

**Scenario**: Search blog posts by content and metadata.

**Database Schema**:
```sql
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    author_id UUID NOT NULL,
    status TEXT NOT NULL,
    published_at TIMESTAMPTZ,
    tags TEXT[] DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    search_vector TSVECTOR NOT NULL
);

CREATE INDEX idx_post_search ON tb_post USING gin(search_vector);
CREATE INDEX idx_post_tags ON tb_post USING gin(tags);
CREATE INDEX idx_post_metadata ON tb_post USING gin(metadata);

-- Auto-update search vector
CREATE TRIGGER tb_post_search_update
BEFORE INSERT OR UPDATE ON tb_post
FOR EACH ROW EXECUTE FUNCTION
  tsvector_update_trigger(search_vector, 'pg_catalog.english', title, content);
```

**GraphQL Query**:
```graphql
query SearchPublishedPosts($query: String!, $category: String!) {
  posts(
    where: {
      AND: [
        # Must be published
        { status: { eq: "published" } },
        # Published in last 90 days
        {
          publishedAt: {
            gte: "2024-07-01T00:00:00Z"
          }
        },
        # Full-text search in title/content
        {
          searchVector: {
            websearch_query: $query,
            rank_gt: 0.15
          }
        },
        # Must have category tag
        {
          tags: {
            contains: $category
          }
        },
        # Must have featured image
        {
          metadata: {
            path_exists: "$.featuredImage"
          }
        }
      ]
    },
    limit: 20
  ) {
    id
    title
    publishedAt
    tags
    metadata
  }
}
```

**Variables**:
```json
{
  "query": "graphql api tutorial",
  "category": "tutorial"
}
```

---

### Example 2: Find Draft Posts Missing Required Fields

**Scenario**: Quality check - find draft posts that can't be published due to missing data.

**GraphQL Query**:
```graphql
query IncompleteDrafts {
  posts(
    where: {
      AND: [
        { status: { eq: "draft" } },
        {
          OR: [
            # Missing featured image
            {
              metadata: {
                NOT: {
                  path_exists: "$.featuredImage"
                }
              }
            },
            # No tags
            {
              tags: {
                len_eq: 0
              }
            },
            # Missing SEO metadata
            {
              metadata: {
                NOT: {
                  has_all_keys: ["seoTitle", "seoDescription"]
                }
              }
            }
          ]
        }
      ]
    }
  ) {
    id
    title
    tags
    metadata
  }
}
```

**Use Case**: Editorial dashboard showing posts needing attention before publication.

---

### Example 3: Author Content Analytics

**Scenario**: Find an author's most popular posts by topic.

**GraphQL Query**:
```graphql
query AuthorPopularPosts($authorId: UUID!, $topics: [String!]!) {
  posts(
    where: {
      AND: [
        { authorId: { eq: $authorId } },
        { status: { eq: "published" } },
        {
          tags: {
            overlaps: $topics
          }
        },
        # High engagement (stored in metadata)
        {
          metadata: {
            path_match: "$.stats.views > 1000"
          }
        }
      ]
    }
  ) {
    id
    title
    publishedAt
    tags
    metadata
  }
}
```

**Variables**:
```json
{
  "authorId": "550e8400-e29b-41d4-a716-446655440000",
  "topics": ["python", "javascript", "tutorial"]
}
```

---

## User Management & Permissions

### Example 1: Find Users with Specific Permissions

**Scenario**: Security audit - find all users who can manage billing.

**Database Schema**:
```sql
CREATE TABLE tb_user (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    username TEXT NOT NULL,
    roles TEXT[] NOT NULL DEFAULT '{}',
    permissions JSONB NOT NULL DEFAULT '{}',
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX idx_user_roles ON tb_user USING gin(roles);
CREATE INDEX idx_user_permissions ON tb_user USING gin(permissions);
```

**GraphQL Query**:
```graphql
query UsersWithBillingAccess {
  users(
    where: {
      OR: [
        # Has admin role
        {
          roles: {
            contains: "admin"
          }
        },
        # Has explicit billing permission
        {
          permissions: {
            has_key: "manage_billing"
          }
        },
        # Member of billing team
        {
          metadata: {
            contains: { team: "billing" }
          }
        }
      ]
    }
  ) {
    id
    email
    username
    roles
    permissions
  }
}
```

**Use Case**: Compliance audit, permission review, security analysis.

---

### Example 2: Find Inactive Admin Accounts

**Scenario**: Security cleanup - find admin accounts that haven't logged in recently.

**GraphQL Query**:
```graphql
query InactiveAdmins($thresholdDate: String!) {
  users(
    where: {
      AND: [
        # Has admin or moderator role
        {
          roles: {
            overlaps: ["admin", "moderator"]
          }
        },
        {
          OR: [
            # Never logged in
            {
              metadata: {
                NOT: {
                  path_exists: "$.lastLogin"
                }
              }
            },
            # Last login before threshold
            {
              metadata: {
                path_match: "$.lastLogin < \"$thresholdDate\""
              }
            }
          ]
        }
      ]
    }
  ) {
    id
    email
    roles
    metadata
  }
}
```

**Variables**:
```json
{
  "thresholdDate": "2024-07-01T00:00:00Z"
}
```

---

### Example 3: Role-Based Access Control Query

**Scenario**: Check if user has required permissions for an action.

**GraphQL Query**:
```graphql
query CanUserPerformAction(
  $userId: UUID!,
  $requiredRoles: [String!]!,
  $requiredPermissions: [String!]!
) {
  users(
    where: {
      AND: [
        { id: { eq: $userId } },
        {
          OR: [
            # Has any required role
            {
              roles: {
                overlaps: $requiredRoles
              }
            },
            # Has all required permissions
            {
              permissions: {
                has_all_keys: $requiredPermissions
              }
            }
          ]
        }
      ]
    }
  ) {
    id
    roles
    permissions
  }
}
```

---

## Log Analysis & Monitoring

### Example 1: Search Application Logs

**Scenario**: Find error logs matching pattern with context.

**Database Schema**:
```sql
CREATE TABLE tb_log_entry (
    id UUID PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    tags TEXT[] DEFAULT '{}',
    context JSONB DEFAULT '{}',
    search_vector TSVECTOR NOT NULL
);

CREATE INDEX idx_log_timestamp ON tb_log_entry (timestamp DESC);
CREATE INDEX idx_log_tags ON tb_log_entry USING gin(tags);
CREATE INDEX idx_log_search ON tb_log_entry USING gin(search_vector);
```

**GraphQL Query**:
```graphql
query SearchErrorLogs($startTime: String!, $endTime: String!) {
  logEntries(
    where: {
      AND: [
        # Error or critical level
        {
          level: {
            in: ["ERROR", "CRITICAL"]
          }
        },
        # Time range
        {
          timestamp: {
            gte: $startTime,
            lte: $endTime
          }
        },
        # Search for database-related errors
        {
          searchVector: {
            websearch_query: "database connection OR timeout"
          }
        },
        # From production environment
        {
          tags: {
            contains: "production"
          }
        },
        # Has request ID (correlate errors)
        {
          context: {
            has_key: "requestId"
          }
        }
      ]
    },
    limit: 100
  ) {
    id
    timestamp
    level
    message
    tags
    context
  }
}
```

---

### Example 2: Monitor API Rate Limiting

**Scenario**: Find IPs/users hitting rate limits.

**GraphQL Query**:
```graphql
query RateLimitViolations($since: String!) {
  logEntries(
    where: {
      AND: [
        { timestamp: { gte: $since } },
        {
          tags: {
            contains: "rate_limit"
          }
        },
        # HTTP 429 status
        {
          context: {
            contains: { statusCode: 429 }
          }
        },
        # Group by IP (filter shows repeated violations)
        {
          message: {
            matches: "Rate limit exceeded"
          }
        }
      ]
    }
  ) {
    id
    timestamp
    message
    context
  }
}
```

---

## Multi-tenant SaaS Application

### Example 1: Tenant Usage Analytics

**Scenario**: Find tenants using specific features.

**Database Schema**:
```sql
CREATE TABLE tb_tenant (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    plan TEXT NOT NULL,
    features TEXT[] DEFAULT '{}',
    settings JSONB DEFAULT '{}',
    usage_stats JSONB DEFAULT '{}'
);

CREATE INDEX idx_tenant_features ON tb_tenant USING gin(features);
CREATE INDEX idx_tenant_settings ON tb_tenant USING gin(settings);
```

**GraphQL Query**:
```graphql
query PremiumTenantsWithHighUsage {
  tenants(
    where: {
      AND: [
        # Premium or enterprise plan
        {
          plan: {
            in: ["premium", "enterprise"]
          }
        },
        # Has API access feature
        {
          features: {
            contains: "api_access"
          }
        },
        # High API usage (>10,000 requests/month)
        {
          usageStats: {
            path_match: "$.api.requestsThisMonth > 10000"
          }
        },
        # Has custom domain configured
        {
          settings: {
            has_key: "customDomain"
          }
        }
      ]
    }
  ) {
    id
    name
    plan
    features
    usageStats
  }
}
```

**Use Case**: Identify power users for upsell, support prioritization, capacity planning.

---

### Example 2: Feature Flag Rollout

**Scenario**: Find tenants eligible for new feature rollout.

**GraphQL Query**:
```graphql
query FeatureRolloutEligible($featureName: String!) {
  tenants(
    where: {
      AND: [
        # Not already enrolled in feature
        {
          features: {
            NOT: {
              contains: $featureName
            }
          }
        },
        # Has opted into beta features
        {
          settings: {
            contains: { betaFeatures: true }
          }
        },
        # Active within last 30 days
        {
          usageStats: {
            path_exists: "$.lastActive"
          }
        },
        # On compatible plan
        {
          plan: {
            in: ["premium", "enterprise"]
          }
        },
        # Meets minimum usage threshold
        {
          usageStats: {
            path_match: "$.activeUsers > 5"
          }
        }
      ]
    }
  ) {
    id
    name
    plan
    settings
  }
}
```

---

### Example 3: Tenant Health Monitoring

**Scenario**: Find tenants with potential issues.

**GraphQL Query**:
```graphql
query TenantsNeedingAttention {
  tenants(
    where: {
      OR: [
        # High error rate
        {
          usageStats: {
            path_match: "$.errors.rate > 0.05"
          }
        },
        # Low engagement (no activity in 14 days)
        {
          usageStats: {
            path_match: "$.daysSinceLastActive > 14"
          }
        },
        # Payment issues
        {
          settings: {
            contains: { paymentStatus: "failed" }
          }
        },
        # Missing required configuration
        {
          settings: {
            NOT: {
              has_all_keys: ["webhookUrl", "apiKey"]
            }
          }
        }
      ]
    }
  ) {
    id
    name
    plan
    usageStats
    settings
  }
}
```

**Use Case**: Proactive customer success, churn prevention, support triage.

---

## Performance Tips

### 1. Always Use Indexes

For every example above, appropriate indexes are created. **Without indexes, these queries will be slow**.

**Critical indexes**:
```sql
-- Array filters
CREATE INDEX idx_tags ON table USING gin(tags);

-- JSONB filters
CREATE INDEX idx_jsonb ON table USING gin(jsonb_column);

-- Full-text search (ESSENTIAL!)
CREATE INDEX idx_fts ON table USING gin(search_vector);

-- Range queries
CREATE INDEX idx_timestamp ON table (timestamp DESC);
CREATE INDEX idx_price ON table (price);
```

### 2. Combine Filters Wisely

Put the most selective filters first in your `AND` conditions:

```graphql
# ✅ Good: Selective filters first
where: {
  AND: [
    { id: { eq: $specificId } },           # Very selective
    { status: { eq: "active" } },          # Selective
    { tags: { overlaps: ["featured"] } }  # Less selective
  ]
}

# ❌ Less optimal: Broad filters first
where: {
  AND: [
    { tags: { overlaps: ["common"] } },   # Matches many rows
    { status: { eq: "active" } },          # Filters after scanning
    { id: { eq: $specificId } }           # Should be first!
  ]
}
```

### 3. Use LIMIT

Always limit result sets, especially with full-text search:

```graphql
query {
  posts(
    where: { searchVector: { websearch_query: "tutorial" } },
    limit: 20,
    offset: 0
  ) { id title }
}
```

### 4. Monitor Query Performance

Use `EXPLAIN ANALYZE` to verify index usage:

```sql
EXPLAIN ANALYZE
SELECT data FROM v_product
WHERE tags && ARRAY['electronics']::text[]
AND price >= 100;

-- Look for:
-- ✅ "Bitmap Index Scan on idx_product_tags"
-- ❌ "Seq Scan on tb_product" (means no index used!)
```

---

## Next Steps

- **[Filter Operators Reference](../advanced/filter-operators.md)** - Complete operator documentation
- **[Where Input Types](../advanced/where-input-types.md)** - Basic filtering guide
- **[PostgreSQL Extensions](../core/postgresql-extensions.md)** - Required PostgreSQL setup

---

**Need help?** Check the [troubleshooting section](../advanced/filter-operators.md#troubleshooting) in the filter operators reference.
