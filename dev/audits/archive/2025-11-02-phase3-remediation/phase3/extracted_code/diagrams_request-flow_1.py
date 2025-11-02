# Extracted from: docs/diagrams/request-flow.md
# Block number: 1
# GraphQL Query
query GetUser($id: UUID!) {
  user(id: $id) {
    id
    name
    email
  }
}
