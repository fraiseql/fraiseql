"""
Chaos Engineering Fixtures

This module provides fixtures for chaos engineering tests, particularly
for network chaos injection using Toxiproxy.
"""

import time
import requests
from typing import Dict, Any, Optional


class ToxiproxyManager:
    """Manager for Toxiproxy network chaos injection."""

    def __init__(self, host: str = "localhost", port: int = 8474):
        self.base_url = f"http://{host}:{port}"
        self.proxies: Dict[str, Dict[str, Any]] = {}

    def create_proxy(self, name: str, listen_addr: str, upstream_addr: str) -> Dict[str, Any]:
        """
        Create a new Toxiproxy proxy.

        Args:
            name: Proxy name
            listen_addr: Address to listen on (e.g., "0.0.0.0:5432")
            upstream_addr: Upstream address to proxy to (e.g., "postgres:5432")

        Returns:
            Proxy configuration
        """
        payload = {"name": name, "listen": listen_addr, "upstream": upstream_addr}

        response = requests.post(f"{self.base_url}/proxies", json=payload)
        response.raise_for_status()

        proxy = response.json()
        self.proxies[name] = proxy
        return proxy

    def delete_proxy(self, name: str):
        """Delete a Toxiproxy proxy."""
        response = requests.delete(f"{self.base_url}/proxies/{name}")
        if response.status_code == 200:
            self.proxies.pop(name, None)

    def list_proxies(self) -> Dict[str, Any]:
        """List all Toxiproxy proxies."""
        response = requests.get(f"{self.base_url}/proxies")
        response.raise_for_status()
        return response.json()

    def get_proxy(self, name: str) -> Optional[Dict[str, Any]]:
        """Get a specific proxy configuration."""
        response = requests.get(f"{self.base_url}/proxies/{name}")
        if response.status_code == 200:
            return response.json()
        return None

    def enable_proxy(self, name: str):
        """Enable a proxy (remove all toxics)."""
        response = requests.post(f"{self.base_url}/proxies/{name}/toxics", json=[])
        response.raise_for_status()

    def disable_proxy(self, name: str):
        """Disable a proxy completely."""
        # Set upstream to non-existent address
        payload = {"upstream": "127.0.0.1:0"}
        response = requests.post(f"{self.base_url}/proxies/{name}", json=payload)
        response.raise_for_status()

    def add_latency_toxic(
        self, proxy_name: str, latency_ms: int, jitter_ms: int = 0
    ) -> Dict[str, Any]:
        """
        Add latency toxic to a proxy.

        Args:
            proxy_name: Name of the proxy
            latency_ms: Base latency in milliseconds
            jitter_ms: Jitter variation in milliseconds

        Returns:
            Toxic configuration
        """
        toxic = {
            "name": "latency",
            "type": "latency",
            "attributes": {"latency": latency_ms, "jitter": jitter_ms},
            "stream": "upstream",
            "toxicity": 1.0,
        }

        response = requests.post(f"{self.base_url}/proxies/{proxy_name}/toxics", json=toxic)
        response.raise_for_status()
        return response.json()

    def add_packet_loss_toxic(self, proxy_name: str, loss_percent: float) -> Dict[str, Any]:
        """
        Add packet loss toxic to a proxy.

        Args:
            proxy_name: Name of the proxy
            loss_percent: Percentage of packets to drop (0.0-1.0)

        Returns:
            Toxic configuration
        """
        toxic = {
            "name": "packet_loss",
            "type": "timeout",
            "attributes": {
                "timeout": 1  # Very short timeout to simulate packet loss
            },
            "stream": "upstream",
            "toxicity": loss_percent,
        }

        response = requests.post(f"{self.base_url}/proxies/{proxy_name}/toxics", json=toxic)
        response.raise_for_status()
        return response.json()

    def add_bandwidth_limit_toxic(self, proxy_name: str, rate_kbps: int) -> Dict[str, Any]:
        """
        Add bandwidth limit toxic to a proxy.

        Args:
            proxy_name: Name of the proxy
            rate_kbps: Bandwidth limit in kbps

        Returns:
            Toxic configuration
        """
        toxic = {
            "name": "bandwidth_limit",
            "type": "bandwidth",
            "attributes": {"rate": rate_kbps},
            "stream": "upstream",
            "toxicity": 1.0,
        }

        response = requests.post(f"{self.base_url}/proxies/{proxy_name}/toxics", json=toxic)
        response.raise_for_status()
        return response.json()

    def remove_all_toxics(self, proxy_name: str):
        """Remove all toxics from a proxy."""
        response = requests.delete(f"{self.base_url}/proxies/{proxy_name}/toxics")
        response.raise_for_status()

    def reset_proxy(self, proxy_name: str):
        """Reset a proxy to normal operation."""
        self.remove_all_toxics(proxy_name)
        self.enable_proxy(proxy_name)

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - clean up all proxies."""
        for proxy_name in list(self.proxies.keys()):
            try:
                self.reset_proxy(proxy_name)
                self.delete_proxy(proxy_name)
            except:
                pass  # Ignore cleanup errors


# Convenience fixtures for common chaos scenarios


def chaos_database_latency(toxiproxy: ToxiproxyManager):
    """Fixture for database latency chaos."""
    proxy_name = "postgres_chaos"
    toxiproxy.create_proxy(proxy_name, "0.0.0.0:5432", "postgres:5432")
    return lambda latency_ms: toxiproxy.add_latency_toxic(proxy_name, latency_ms)


def chaos_packet_loss(toxiproxy: ToxiproxyManager):
    """Fixture for packet loss chaos."""
    proxy_name = "postgres_chaos"
    toxiproxy.create_proxy(proxy_name, "0.0.0.0:5432", "postgres:5432")
    return lambda loss_percent: toxiproxy.add_packet_loss_toxic(proxy_name, loss_percent)


def chaos_slow_database(toxiproxy: ToxiproxyManager):
    """Fixture for slow database operations."""
    proxy_name = "postgres_chaos"
    toxiproxy.create_proxy(proxy_name, "0.0.0.0:5432", "postgres:5432")
    return lambda delay_ms: toxiproxy.add_latency_toxic(
        proxy_name, delay_ms, jitter_ms=delay_ms // 4
    )
