# Extracted from: docs/production/deployment.md
# Block number: 3
# Adjust pool size based on replicas
# Rule: total_connections = replicas * pool_size
# PostgreSQL max_connections should be: total_connections + buffer

# 3 replicas * 20 connections = 60 total
# Set PostgreSQL max_connections = 100

config = FraiseQLConfig(database_pool_size=20, database_max_overflow=10)
