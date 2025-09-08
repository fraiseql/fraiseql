"""Example of how PrintOptim Backend should integrate with fixed FraiseQL TurboRouter.

This example demonstrates how to load queries registered with raw hashes
in a PostgreSQL database into FraiseQL's TurboRegistry.
"""

import hashlib
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry


class PrintOptimTurboIntegration:
    """Integration helper for PrintOptim backend."""

    def __init__(self, registry: TurboRegistry):
        """Initialize with a TurboRegistry instance."""
        self.registry = registry

    async def load_database_queries(self, db_connection):
        """Load turbo queries from database into registry.

        This method simulates loading queries from the turbo.tb_turbo_query table.
        """
        # Example query that would be run against PrintOptim's database
        db_queries = [
            {
                'operation_name': 'GetNetworkConfigurations',
                'query_hash': '859f5d3b94c4c1add28a74674c83d6b49cc4406c1292e21822d4ca3beb76d269',
                'graphql_query': """query GetNetworkConfigurations {
  networkConfigurations {
    id
    ipAddress
    isDhcp
    identifier
    subnetMask
    emailAddress
    nDirectAllocations
    dns1 {
      id
      ipAddress
      __typename
    }
    dns2 {
      id
      ipAddress
      __typename
    }
    gateway {
      id
      ipAddress
      __typename
    }
    router {
      id
      hostname
      ipAddress
      macAddress
      __typename
    }
    printServers {
      id
      hostname
      __typename
    }
    smtpServer {
      id
      hostname
      port
      __typename
    }
    __typename
  }
}""",
                'sql_template': 'SELECT turbo.fn_get_network_configurations()::json as result',
                'is_active': True
            }
        ]

        loaded_count = 0
        for db_query in db_queries:
            if not db_query['is_active']:
                continue

            # Create TurboQuery
            turbo_query = TurboQuery(
                graphql_query=db_query['graphql_query'],
                sql_template=db_query['sql_template'],
                param_mapping={},  # PrintOptim queries don't use variables
                operation_name=db_query['operation_name']
            )

            # Register using the raw hash from database
            # This is the key fix - use register_with_raw_hash for database-stored hashes
            self.registry.register_with_raw_hash(turbo_query, db_query['query_hash'])
            loaded_count += 1

            print(f"✅ Loaded {db_query['operation_name']} with hash {db_query['query_hash'][:16]}...")

        return loaded_count


def demonstrate_fix():
    """Demonstrate the fix for PrintOptim backend issue."""
    print("🔧 PrintOptim Backend TurboRouter Integration Fix")
    print("=" * 60)

    # Create registry
    registry = TurboRegistry()
    integration = PrintOptimTurboIntegration(registry)

    # Load database queries (simulated)
    print("1. Loading database-registered queries...")
    # In real code, this would be: await integration.load_database_queries(db)
    loaded_count = 1  # Simulated result

    # Simulate the loading manually for demo
    raw_query = """query GetNetworkConfigurations {
  networkConfigurations {
    id
    ipAddress
    isDhcp
    identifier
    subnetMask
    emailAddress
    nDirectAllocations
    dns1 {
      id
      ipAddress
      __typename
    }
    dns2 {
      id
      ipAddress
      __typename
    }
    gateway {
      id
      ipAddress
      __typename
    }
    router {
      id
      hostname
      ipAddress
      macAddress
      __typename
    }
    printServers {
      id
      hostname
      __typename
    }
    smtpServer {
      id
      hostname
      port
      __typename
    }
    __typename
  }
}"""

    # The raw hash that PrintOptim calculated and stored in their database
    raw_hash = "859f5d3b94c4c1add28a74674c83d6b49cc4406c1292e21822d4ca3beb76d269"

    turbo_query = TurboQuery(
        graphql_query=raw_query,
        sql_template="SELECT turbo.fn_get_network_configurations()::json as result",
        param_mapping={},
        operation_name="GetNetworkConfigurations"
    )

    registry.register_with_raw_hash(turbo_query, raw_hash)

    print(f"✅ Loaded 1 query with raw hash registration")

    print(f"\n2. Testing query lookup...")

    # Test hash calculations
    print(f"   Raw hash (PrintOptim):      {registry.hash_query_raw(raw_query)}")
    print(f"   Normalized hash (FraiseQL): {registry.hash_query(raw_query)}")

    # Test query lookup - this should now work!
    found_query = registry.get(raw_query)

    if found_query:
        print(f"✅ SUCCESS: Query found in registry!")
        print(f"   Operation: {found_query.operation_name}")
        print(f"   SQL Template: {found_query.sql_template}")

        # Test with different formatting
        minified = "query GetNetworkConfigurations{networkConfigurations{id}}"
        found_minified = registry.get(minified)
        if found_minified:
            print(f"✅ BONUS: Even works with different formatting!")
        else:
            print(f"ℹ️  Different query content = different hash (expected)")
    else:
        print("❌ FAILED: Query not found in registry")

    print(f"\n3. Summary")
    print(f"   Registry size: {len(registry)}")
    print(f"   Fix status: {'SUCCESS' if found_query else 'FAILED'}")

    return found_query is not None


if __name__ == "__main__":
    success = demonstrate_fix()

    print(f"\n{'🎉 INTEGRATION FIX VERIFIED' if success else '❌ INTEGRATION FIX FAILED'}")

    if success:
        print("\n📝 Integration Instructions for PrintOptim Backend:")
        print("   1. Upgrade to FraiseQL with the TurboRouter hash fix")
        print("   2. Use registry.register_with_raw_hash() when loading database queries")
        print("   3. Raw hashes from database will now match at query time")
        print("   4. TurboRouter should activate with 'mode': 'turbo' and <20ms response times")
