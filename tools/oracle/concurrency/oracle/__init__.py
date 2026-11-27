"""Concurrency oracle: what clients read while writes happen.

Writers bump per-account version counters through GraphQL mutations; readers read the
counters back through several read shapes. Sequential cells check one promise each, and
concurrent cells log every operation with its invoke and complete times for a
linearizability-lite checker (`checker.py`). Every cell that does not hold must match
`baseline.json`, in both directions.
"""
