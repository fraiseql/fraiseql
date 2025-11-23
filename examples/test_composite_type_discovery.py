"""Manual test to verify composite type discovery works with real SpecQL database.

Usage:
    python examples/test_composite_type_discovery.py
"""

import asyncio
import os

import psycopg_pool

from fraiseql.introspection import AutoDiscovery


async def main():
    # Connect to database with SpecQL schema
    database_url = os.getenv("DATABASE_URL", "postgresql://user:password@localhost:5432/printoptim")

    print(f"🔌 Connecting to: {database_url}")

    connection_pool = psycopg_pool.AsyncConnectionPool(conninfo=database_url)

    # Initialize AutoDiscovery
    auto_discovery = AutoDiscovery(connection_pool)

    print("🔍 Discovering schema...")

    # Discover all (READ from database)
    result = await auto_discovery.discover_all(
        schemas=["app"]  # SpecQL puts things in 'app' schema
    )

    # Print results
    print(f"\n✅ Discovered {len(result['types'])} types")
    print(f"✅ Discovered {len(result['queries'])} queries")
    print(f"✅ Discovered {len(result['mutations'])} mutations")

    # Print mutation details
    if result["mutations"]:
        print("\n📝 Mutations:")
        for mutation in result["mutations"]:
            print(f"   - {mutation}")
    else:
        print("\n⚠️  No mutations discovered - check if functions exist in 'app' schema")

    await connection_pool.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except Exception as e:
        print(f"\n❌ Error: {e}")
        print("\nTroubleshooting:")
        print("1. Check DATABASE_URL is correct")
        print("2. Verify SpecQL schema exists: \\dT app.type_* in psql")
        print("3. Check functions exist: \\df app.* in psql")
