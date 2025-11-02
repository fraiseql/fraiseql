# Extracted from: docs/performance/APQ_ASSESSMENT.md
# Block number: 1
# When Apollo Client sends APQ request:
1. Client sends query hash (sha256)
2. Server checks if query string is cached
3. If cache miss: Client re-sends full query + hash
4. Server stores query string for future requests
5. If cache hit: Server uses cached query string
