# CLI Query Tool for FraiseQL

**Status:** ✅ Production Ready
**Audience:** DevOps, backend developers, automation engineers
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

Complete guide for querying FraiseQL servers from the command line using the `FraiseQL-query` CLI tool.

---

## Installation

### Prerequisites

- FraiseQL server running
- curl or HTTP client access to server

### Install CLI Tool

```bash
# Using npm
npm install -g @FraiseQL/cli

# Using Homebrew (macOS/Linux)
brew install FraiseQL

# Or download binary directly
# https://github.com/FraiseQL/FraiseQL-cli/releases
```text

### Verify Installation

```bash
FraiseQL-query --version
# FraiseQL-query 2.0.0
```text

---

## Basic Query Execution

### Simple Query

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }"
```text

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
```text

### Query with Variables

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "query GetUser(\$id: ID!) { user(id: \$id) { id name email } }" \
  --variables '{"id": "1"}'
```text

### Pretty-Print Output

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format pretty
```text

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
```text

### Execute from File

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_users.graphql
```text

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
```text

```bash
# Create variables file
cat > variables.json <<EOF
{
  "id": "1"
}
EOF

# Execute with variables
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_user_by_id.graphql \
  --variables-file variables.json
```text

---

## Mutations

### Execute Mutation

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "mutation CreatePost(\$title: String!, \$content: String!) {
    createPost(title: \$title, content: \$content) {
      id
      title
      createdAt
    }
  }" \
  --variables '{"title": "My First Post", "content": "Hello World"}'
```text

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
```text

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --file mutations/create_post.graphql \
  --variables '{"title": "Test", "content": "Content"}'
```text

---

## Output Formatting

### JSON Output (Default)

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format json
```text

### CSV Output

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }" \
  --format csv
```text

Output:

```text
id,name,email
1,Alice,alice@example.com
2,Bob,bob@example.com
```text

### Table Output

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name email } }" \
  --format table
```text

Output:

```text
┌────┬───────┬───────────────────┐
│ id │ name  │ email             │
├────┼───────┼───────────────────┤
│ 1  │ Alice │ alice@example.com │
│ 2  │ Bob   │ bob@example.com   │
└────┴───────┴───────────────────┘
```text

### YAML Output

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --format yaml
```text

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

FraiseQL-query \
  --endpoint "$ENDPOINT" \
  --file queries/get_user.graphql \
  --variables "{\"id\": \"$USER_ID\"}" \
  --format pretty
```text

Run:

```bash
chmod +x fetch_user_data.sh
./fetch_user_data.sh 1
```text

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
  FraiseQL-query \
    --endpoint "$ENDPOINT" \
    --file mutations/create_post.graphql \
    --variables "{\"title\": \"$title\", \"content\": \"$content\"}" \
    --format json | jq '.data.createPost.id'
done
```text

### Parallel Execution

```bash
#!/bin/bash
# parallel_queries.sh

ENDPOINT="http://localhost:5000/graphql"

# Run queries in parallel
for i in {1..100}; do
  FraiseQL-query \
    --endpoint "$ENDPOINT" \
    --query "query { user(id: \"$i\") { id name } }" \
    --format json > "user_$i.json" &
done

# Wait for all background jobs
wait

# Combine results
jq -s '.' user_*.json > all_users.json
```text

---

## Authentication

### Bearer Token

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --auth "Bearer token_here"
```text

### Custom Headers

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --header "Authorization: Bearer token_here" \
  --header "X-Custom-Header: value"
```text

### Environment Variable

```bash
export FRAISEQL_TOKEN="my_secret_token"

FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ me { id name } }" \
  --auth "Bearer $FRAISEQL_TOKEN"
```text

---

## Environment Configuration

### Config File

```toml
# ~/.FraiseQL/config.toml
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
```text

### Use Config Profile

```bash
FraiseQL-query \
  --config production \
  --file queries/get_users.graphql
```text

### Override Config

```bash
FraiseQL-query \
  --config production \
  --endpoint http://localhost:5000/graphql \
  --file queries/get_users.graphql
```text

---

## Performance & Monitoring

### Show Query Execution Time

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --show-timing
```text

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
```text

### Enable Debug Output

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --debug
```text

### Verbose Logging

```bash
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --verbose
```text

---

## Error Handling

### Capture Errors in Script

```bash
#!/bin/bash
# safe_query.sh

ENDPOINT="http://localhost:5000/graphql"

result=$(FraiseQL-query \
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
```text

### Retry Logic

```bash
#!/bin/bash
# retry_query.sh

ENDPOINT="http://localhost:5000/graphql"
MAX_RETRIES=3
RETRY_DELAY=2

for i in $(seq 1 $MAX_RETRIES); do
  result=$(FraiseQL-query \
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
```text

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

FraiseQL-query \
  --endpoint "$ENDPOINT" \
  --file queries/export_users.graphql \
  --format csv > "$OUTPUT_DIR/users_$DATE.csv"

# Compress
gzip "$OUTPUT_DIR/users_$DATE.csv"

# Upload to S3
aws s3 cp "$OUTPUT_DIR/users_$DATE.csv.gz" s3://my-bucket/exports/
```text

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

      - name: Install FraiseQL-query
        run: npm install -g @FraiseQL/cli

      - name: Export users
        run: |
          FraiseQL-query \
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
```text

---

## Advanced Usage

### Streaming Large Datasets

```bash
# Stream results directly to file without loading in memory
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --file queries/all_users.graphql \
  --format csv \
  --stream > large_export.csv
```text

### GraphQL Introspection

```bash
# Get schema introspection
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --introspect > schema.json
```text

### Validate Query

```bash
# Validate query syntax without executing
FraiseQL-query \
  --endpoint http://localhost:5000/graphql \
  --query "{ users { id name } }" \
  --validate
```text

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
