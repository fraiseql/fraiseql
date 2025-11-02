# Extracted from: docs/performance/rust-pipeline-optimization.md
# Block number: 3
from fraiseql.utils import DataLoader

user_loader = DataLoader(load_fn=batch_load_users)

# Batches multiple user lookups into single query
users = await asyncio.gather(*[user_loader.load(id) for id in user_ids])
