"""Fraisier - Deployment orchestrator for the FraiseQL ecosystem.

Any language, any database, any deployment target.

A fraisier (French for strawberry plant) manages fraises (services).
Just as a strawberry plant produces strawberries, Fraisier orchestrates
the deployment of your services (fraises).

Key Concepts:
    - fraise: A deployable service (the strawberry fruit)
    - fraisier: The deployment orchestrator (the plant)
    - fraises.yaml: Configuration file listing all fraises

Usage:
    fraisier list                           # List all fraises
    fraisier deploy <fraise> <environment>  # Deploy a fraise
    fraisier status <fraise> <environment>  # Check fraise status
"""

__version__ = "0.1.0"
__all__ = ["__version__"]
