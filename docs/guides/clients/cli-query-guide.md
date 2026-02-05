# CLI Query Tool for FraiseQL

**Status:** ✅ Production Ready
**Audience:** DevOps, backend developers, automation engineers
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

Complete guide for querying FraiseQL servers from the command line using the `fraiseql-query` CLI tool.

---

## Installation

### Prerequisites

- FraiseQL server running
- curl or HTTP client access to server

### Install CLI Tool

```bash
# Using npm
npm install -g @fraiseql/cli

# Using Homebrew (macOS/Linux)
brew install fraiseql

# Or download binary directly
# https://github.com/fraiseql/fraiseql-cli/releases
```

### Verify Installation

```bash
fraiseql-query --version
# fraiseql-query 2.0.0
```

---

## Basic Query Execution

### Simple Query

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }"
```

Output:
```json
{
  "data": {
    "users": [
      { "id": "1", "name": "Alice", "email": "alice@example.com" },
      { "id": "2", "name": "Bob", "email": "bob@example.com" }
    ]
  }
}
```

### Query with Variables

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "query GetUser(\$id: ID!) { user(id: \$id) { id name email } }" \
  --variables '{"id": "1"}'
```

### Pretty-Print Output

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format pretty
```

---

## Query Files

### Create Query File

```graphql
# queries/get_users.graphql
query GetUsers {
  users {
    id
    name
    email
    createdAt
  }
}
```

### Execute from File

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_users.graphql
```

### Variables in File

```graphql
# queries/get_user_by_id.graphql
query GetUserById($id: ID!) {
  user(id: $id) {
    id
    name
    email
    posts {
      id
      title
    }
  }
}
```

```bash
# Create variables file
cat > variables.json <<EOF
{
  "id": "1"
}
EOF

# Execute with variables
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_user_by_id.graphql \
  --variables-file variables.json
```

---

## Mutations

### Execute Mutation

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "mutation CreatePost(\$title: String!, \$content: String!) {
    createPost(title: \$title, content: \$content) {
      id
      title
      createdAt
    }
  }" \
  --variables '{"title": "My First Post", "content": "Hello World"}'
```

### Mutation File

```graphql
# mutations/create_post.graphql
mutation CreatePost($title: String!, $content: String!) {
  createPost(title: $title, content: $content) {
    id
    title
    content
    createdAt
  }
}
```

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --file mutations/create_post.graphql \
  --variables '{"title": "Test", "content": "Content"}'
```

---

## Output Formatting

### JSON Output (Default)

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format json
```

### CSV Output

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }" \
  --format csv
```

Output:
```
id,name,email
1,Alice,alice@example.com
2,Bob,bob@example.com
```

### Table Output

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }" \
  --format table
```

Output:
```
┌────┬───────┬───────────────────┐
│ id │ name  │ email             │
├────┼───────┼───────────────────┤
│ 1  │ Alice │ alice@example.com │
│ 2  │ Bob   │ bob@example.com   │
└────┴───────┴───────────────────┘
```

### YAML Output

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format yaml
```

---

## Scripting & Automation

### Bash Script Example

```bash
#!/bin/bash
# fetch_user_data.sh

ENDPOINT="http://localhost:5000/graphql"
USER_ID=$1

if [ -z "$USER_ID" ]; then
  echo "Usage: $0 <user_id>"
  exit 1
fi

fraiseql-query \
  --endpoint "$ENDPOINT" \
  --file queries/get_user.graphql \
  --variables "{\"id\": \"$USER_ID\"}" \
  --format pretty
```

Run:
```bash
chmod +x fetch_user_data.sh
./fetch_user_data.sh 1
```

### Batch Operations

```bash
#!/bin/bash
# batch_create_posts.sh

ENDPOINT="http://localhost:5000/graphql"
CSV_FILE=$1

if [ -z "$CSV_FILE" ]; then
  echo "Usage: $0 <csv_file>"
  exit 1
fi

# Skip header, process each line
tail -n +2 "$CSV_FILE" | while IFS=',' read -r title content; do
  fraiseql-query \
    --endpoint "$ENDPOINT" \
    --file mutations/create_post.graphql \
    --variables "{\"title\": \"$title\", \"content\": \"$content\"}" \
    --format json | jq '.data.createPost.id'
done
```

### Parallel Execution

```bash
#!/bin/bash
# parallel_queries.sh

ENDPOINT="http://localhost:5000/graphql"

# Run queries in parallel
for i in {1..100}; do
  fraiseql-query \
    --endpoint "$ENDPOINT" \
    --query "query { user(id: \"$i\") { id name } }" \
    --format json > "user_$i.json" &
done

# Wait for all background jobs
wait

# Combine results
jq -s '.' user_*.json > all_users.json
```

---

## Authentication

### Bearer Token

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --auth "Bearer token_here"
```

### Custom Headers

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --header "Authorization: Bearer token_here" \
  --header "X-Custom-Header: value"
```

### Environment Variable

```bash
export FRAISEQL_TOKEN="my_secret_token"

fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --auth "Bearer $FRAISEQL_TOKEN"
```

---

## Environment Configuration

### Config File

```toml
# ~/.fraiseql/config.toml
[default]
endpoint = "http://localhost:5000/graphql"
format = "pretty"
timeout = 30

[production]
endpoint = "https://api.example.com/graphql"
auth = "Bearer ${FRAISEQL_PROD_TOKEN}"

[staging]
endpoint = "https://staging-api.example.com/graphql"
auth = "Bearer ${FRAISEQL_STAGING_TOKEN}"
```

### Use Config Profile

```bash
fraiseql-query \
  --config production \
  --file queries/get_users.graphql
```

### Override Config

```bash
fraiseql-query \
  --config production \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_users.graphql
```

---

## Performance & Monitoring

### Show Query Execution Time

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --show-timing
```

Output:
```json
{
  "data": { "users": [...] },
  "timing": {
    "totalTime": 125,
    "networkTime": 100,
    "parseTime": 15,
    "compileTime": 10
  }
}
```

### Enable Debug Output

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --debug
```

### Verbose Logging

```bash
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --verbose
```

---

## Error Handling

### Capture Errors in Script

```bash
#!/bin/bash
# safe_query.sh

ENDPOINT="http://localhost:5000/graphql"

result=$(fraiseql-query \
  --endpoint "$ENDPOINT" \
  --query "{ users { id name } }" \
  --format json 2>&1)

if [ $? -ne 0 ]; then
  echo "Query failed:"
  echo "$result"
  exit 1
fi

# Check for GraphQL errors in response
if echo "$result" | jq -e '.errors' > /dev/null; then
  echo "GraphQL error:"
  echo "$result" | jq '.errors'
  exit 1
fi

# Process successful response
echo "$result" | jq '.data.users'
```

### Retry Logic

```bash
#!/bin/bash
# retry_query.sh

ENDPOINT="http://localhost:5000/graphql"
MAX_RETRIES=3
RETRY_DELAY=2

for i in $(seq 1 $MAX_RETRIES); do
  result=$(fraiseql-query \
    --endpoint "$ENDPOINT" \
    --query "{ users { id name } }" \
    --format json 2>&1)

  if [ $? -eq 0 ] && ! echo "$result" | jq -e '.errors' > /dev/null; then
    echo "$result"
    exit 0
  fi

  if [ $i -lt $MAX_RETRIES ]; then
    echo "Attempt $i failed, retrying in ${RETRY_DELAY}s..."
    sleep $RETRY_DELAY
  fi
done

echo "Query failed after $MAX_RETRIES attempts"
exit 1
```

---

## Integration Examples

### Cron Job for Data Export

```bash
#!/bin/bash
# export_users_daily.sh
# Run daily: 0 2 * * * /path/to/export_users_daily.sh

ENDPOINT="http://localhost:5000/graphql"
OUTPUT_DIR="/data/exports"
DATE=$(date +%Y-%m-%d)

fraiseql-query \
  --endpoint "$ENDPOINT" \
  --file queries/export_users.graphql \
  --format csv > "$OUTPUT_DIR/users_$DATE.csv"

# Compress
gzip "$OUTPUT_DIR/users_$DATE.csv"

# Upload to S3
aws s3 cp "$OUTPUT_DIR/users_$DATE.csv.gz" s3://my-bucket/exports/
```

### GitHub Actions Workflow

```yaml
# .github/workflows/sync_data.yml
name: Sync Data

on:
  schedule:
    - cron: '0 */6 * * *'  # Every 6 hours

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install fraiseql-query
        run: npm install -g @fraiseql/cli

      - name: Export users
        run: |
          fraiseql-query \
            --endpoint ${{ secrets.FRAISEQL_ENDPOINT }} \
            --file queries/export_users.graphql \
            --auth "Bearer ${{ secrets.FRAISEQL_TOKEN }}" \
            --format csv > users.csv

      - name: Upload to S3
        run: |
          aws s3 cp users.csv s3://my-bucket/data/
        env:
          AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
          AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

---

## Advanced Usage

### Streaming Large Datasets

```bash
# Stream results directly to file without loading in memory
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/all_users.graphql \
  --format csv \
  --stream > large_export.csv
```

### GraphQL Introspection

```bash
# Get schema introspection
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --introspect > schema.json
```

### Validate Query

```bash
# Validate query syntax without executing
fraiseql-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --validate
```

---

## See Also

**Related Guides:**
- **[Node.js Runtime Client](./nodejs-runtime-guide.md)** - Server-to-server queries
- **[Real-Time Patterns](../PATTERNS.md)** - Subscription support
- **[Production Deployment](../production-deployment.md)** - Running FraiseQL

**Tools & Utilities:**
- **[GraphQL CLI](https://github.com/Urigo/graphql-cli)** - Alternative GraphQL CLI tool
- **[curl](https://curl.se/)** - Direct HTTP queries
- **[jq](https://stedolan.github.io/jq/)** - JSON processing

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
