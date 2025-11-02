# Extracted from: docs/archive/fraiseql_enterprise_gap_analysis.md
# Block number: 2
# Proposed sharding configuration
@dataclass
class ShardConfig:
    shard_key: str
    shard_count: int
    routing_strategy: RoutingStrategy
    replica_configs: list[ReplicaConfig]
