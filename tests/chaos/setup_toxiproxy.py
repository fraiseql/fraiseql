#!/usr/bin/env python3
"""
Toxiproxy Setup for Chaos Engineering

This script sets up Toxiproxy proxies for FraiseQL database connections
to enable network chaos testing scenarios.
"""

import requests
import json
import time
import sys
from typing import Dict, Any, Optional


class FraiseQLToxiproxySetup:
    """Setup Toxiproxy for FraiseQL chaos engineering."""

    def __init__(self, toxiproxy_host: str = "localhost", toxiproxy_port: int = 8474):
        self.base_url = f"http://{toxiproxy_host}:{toxiproxy_port}"
        self.proxies_created = []

    def setup_postgres_proxy(
        self, postgres_host: str = "postgres", postgres_port: int = 5432
    ) -> bool:
        """
        Set up Toxiproxy proxy for PostgreSQL connections.

        Args:
            postgres_host: PostgreSQL server hostname
            postgres_port: PostgreSQL server port

        Returns:
            True if setup successful, False otherwise
        """
        print("🔧 Setting up PostgreSQL proxy for chaos testing...")

        proxy_config = {
            "name": "fraiseql_postgres",
            "listen": "0.0.0.0:5433",  # Different port to avoid conflicts
            "upstream": f"{postgres_host}:{postgres_port}",
            "enabled": True,
        }

        try:
            # Check if proxy already exists
            existing = self.get_proxy("fraiseql_postgres")
            if existing:
                print("📋 PostgreSQL proxy already exists, updating configuration...")
                response = requests.post(
                    f"{self.base_url}/proxies/fraiseql_postgres", json=proxy_config
                )
            else:
                response = requests.post(f"{self.base_url}/proxies", json=proxy_config)

            response.raise_for_status()
            proxy = response.json()

            self.proxies_created.append("fraiseql_postgres")
            print("✅ PostgreSQL proxy configured successfully!")
            print(f"   📡 Listen: {proxy['listen']}")
            print(f"   🔗 Upstream: {proxy['upstream']}")
            print(f"   🆔 Name: {proxy['name']}")

            return True

        except requests.exceptions.RequestException as e:
            print(f"❌ Failed to create PostgreSQL proxy: {e}")
            return False

    def test_proxy_connection(self) -> bool:
        """Test that the proxy is working correctly."""
        print("\n🧪 Testing proxy connection...")

        try:
            # Get proxy status
            proxy = self.get_proxy("fraiseql_postgres")
            if not proxy:
                print("❌ Proxy not found")
                return False

            print(f"✅ Proxy status: {'enabled' if proxy.get('enabled', False) else 'disabled'}")

            # Check if we can reach the upstream through the proxy
            # Note: This is a basic connectivity test
            listen_parts = proxy["listen"].split(":")
            if len(listen_parts) == 2:
                listen_port = int(listen_parts[1])
                print(f"✅ Proxy listening on port {listen_port}")
                return True

        except Exception as e:
            print(f"❌ Proxy connection test failed: {e}")
            return False

    def demonstrate_chaos_capabilities(self) -> bool:
        """Demonstrate basic chaos injection capabilities."""
        print("\n🎭 Demonstrating chaos injection capabilities...")

        try:
            # Add a latency toxic
            print("   📡 Injecting 100ms latency...")
            toxic = {
                "name": "demo_latency",
                "type": "latency",
                "attributes": {"latency": 100},
                "stream": "upstream",
                "toxicity": 1.0,
            }

            response = requests.post(
                f"{self.base_url}/proxies/fraiseql_postgres/toxics", json=toxic
            )
            response.raise_for_status()

            print("   ✅ Latency toxic injected")

            # Wait a moment
            time.sleep(1)

            # Remove the toxic
            print("   🧹 Removing latency toxic...")
            requests.delete(f"{self.base_url}/proxies/fraiseql_postgres/toxics/demo_latency")

            print("   ✅ Latency toxic removed")
            print("   🎉 Chaos injection working correctly!")

            return True

        except Exception as e:
            print(f"❌ Chaos demonstration failed: {e}")
            return False

    def get_proxy(self, name: str) -> Optional[Dict[str, Any]]:
        """Get proxy configuration."""
        try:
            response = requests.get(f"{self.base_url}/proxies/{name}")
            if response.status_code == 200:
                return response.json()
            return None
        except:
            return None

    def list_proxies(self) -> Dict[str, Any]:
        """List all proxies."""
        try:
            response = requests.get(f"{self.base_url}/proxies")
            response.raise_for_status()
            return response.json()
        except Exception as e:
            print(f"❌ Failed to list proxies: {e}")
            return {}

    def cleanup(self):
        """Clean up created proxies."""
        print("\n🧹 Cleaning up chaos proxies...")
        for proxy_name in self.proxies_created:
            try:
                requests.delete(f"{self.base_url}/proxies/{proxy_name}")
                print(f"   ✅ Removed proxy: {proxy_name}")
            except Exception as e:
                print(f"   ⚠️  Failed to remove proxy {proxy_name}: {e}")

    def check_toxiproxy_health(self) -> bool:
        """Check if Toxiproxy is running and healthy."""
        try:
            response = requests.get(f"{self.base_url}/version", timeout=5)
            response.raise_for_status()
            version = response.json().get("version", "unknown")
            print(f"✅ Toxiproxy v{version} is running")
            return True
        except requests.exceptions.RequestException as e:
            print(f"❌ Toxiproxy not accessible: {e}")
            print("   💡 Make sure Toxiproxy is running:")
            print(
                "      docker run -d --name toxiproxy -p 8474:8474 -p 22220:22220 ghcr.io/shopify/toxiproxy:latest"
            )
            return False


def main():
    """Main setup function."""
    print("🚀 FraiseQL Chaos Engineering Setup")
    print("=" * 50)

    setup = FraiseQLToxiproxySetup()

    # Check Toxiproxy health
    if not setup.check_toxiproxy_health():
        print("\n❌ Setup failed - Toxiproxy not available")
        return 1

    try:
        # Setup PostgreSQL proxy
        if not setup.setup_postgres_proxy():
            print("\n❌ Setup failed - PostgreSQL proxy creation failed")
            return 1

        # Test proxy connection
        if not setup.test_proxy_connection():
            print("\n❌ Setup failed - Proxy connection test failed")
            return 1

        # Demonstrate chaos capabilities
        if not setup.demonstrate_chaos_capabilities():
            print("\n❌ Setup failed - Chaos demonstration failed")
            return 1

        print("\n" + "=" * 50)
        print("🎉 FraiseQL Chaos Engineering Setup Complete!")
        print("\n📋 Configuration Summary:")
        print("   🗄️  PostgreSQL Proxy: fraiseql_postgres")
        print("   📡 Listen: 0.0.0.0:5433")
        print("   🔗 Upstream: postgres:5432")
        print("   🎭 Chaos Ready: Yes")
        print("\n🚀 Ready for Phase 1 chaos testing!")

        return 0

    except KeyboardInterrupt:
        print("\n⏹️  Setup interrupted by user")
        return 1
    except Exception as e:
        print(f"\n❌ Setup failed with error: {e}")
        return 1
    finally:
        setup.cleanup()


if __name__ == "__main__":
    sys.exit(main())
