"""TestFoundry extension for automated test generation in FraiseQL.

TestFoundry provides automated test generation for PostgreSQL databases,
creating comprehensive pgTAP tests for GraphQL mutations and database operations.
"""

from .analyzer import FoundryAnalyzer
from .config import FoundryConfig
from .generator import FoundryGenerator
from .setup import FoundrySetup

__version__ = "0.1.0"
__all__ = ["FoundryAnalyzer", "FoundryConfig", "FoundryGenerator", "FoundrySetup"]
