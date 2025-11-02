# Extracted from: docs/diagrams/apq-cache-flow.md
# Block number: 11
# Set appropriate cache headers for APQ responses
response.headers["Cache-Control"] = "public, max-age=300"  # 5 minutes
response.headers["ETag"] = f'W/"{query_hash}"'
