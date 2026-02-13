"""Git provider registry.

Manages available Git providers and allows custom providers to be registered.
"""

from typing import Any

from .base import GitProvider

# Registry of available providers
_providers: dict[str, type[GitProvider]] = {}


def register_provider(provider_class: type[GitProvider]) -> None:
    """Register a Git provider.

    Args:
        provider_class: GitProvider subclass to register
    """
    _providers[provider_class.name] = provider_class


def get_provider(name: str, config: dict[str, Any]) -> GitProvider:
    """Get a Git provider instance by name.

    Args:
        name: Provider name (e.g., "github", "gitlab")
        config: Provider configuration

    Returns:
        Configured GitProvider instance

    Raises:
        ValueError: If provider is not registered
    """
    if name not in _providers:
        available = ", ".join(_providers.keys())
        raise ValueError(
            f"Unknown Git provider: '{name}'. "
            f"Available providers: {available}"
        )

    return _providers[name](config)


def list_providers() -> list[str]:
    """List all registered provider names."""
    return list(_providers.keys())


# Auto-register built-in providers
def _register_builtin_providers() -> None:
    """Register all built-in Git providers."""
    from .github import GitHubProvider
    from .gitlab import GitLabProvider
    from .gitea import GiteaProvider
    from .bitbucket import BitbucketProvider

    register_provider(GitHubProvider)
    register_provider(GitLabProvider)
    register_provider(GiteaProvider)
    register_provider(BitbucketProvider)


_register_builtin_providers()
