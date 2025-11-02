# Extracted from: docs/benchmarks/methodology.md
# Block number: 1
# baseline.py - Python JSON serialization
import json
import time

# Simulate ORM fetching data
users = db.query(User).limit(1000).all()

start = time.perf_counter()
for user in users:
    result = json.dumps(
        {
            "id": user.id,
            "name": user.name,
            # ... 100 fields
        }
    )
end = time.perf_counter()

print(f"Python: {(end - start) * 1000:.2f}ms")
