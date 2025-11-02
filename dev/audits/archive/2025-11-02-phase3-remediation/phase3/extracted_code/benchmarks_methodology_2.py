# Extracted from: docs/benchmarks/methodology.md
# Block number: 2
# This generates 101 queries!
users = session.query(User).limit(100).all()
for user in users:
    posts = user.posts  # Separate query for each user!
