"""Package Scanner Infrastructure.

Concrete implementation of PackageMetadataRepository that scans Python
packages from the installed environment, pyproject.toml, and uv.lock.
"""

import importlib.metadata
import logging
from pathlib import Path
from typing import Optional

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # Fallback for Python < 3.11

from fraiseql.sbom.domain.models import ComponentIdentifier, License
from fraiseql.sbom.domain.repositories import PackageMetadataRepository

logger = logging.getLogger(__name__)


class PythonPackageScanner(PackageMetadataRepository):
    """Scan Python packages from installed environment.

    Infrastructure adapter that implements PackageMetadataRepository
    by reading from Python's package metadata system and lock files.

    Attributes:
        project_root: Path to project root directory
        lock_file_path: Path to uv.lock file
        pyproject_path: Path to pyproject.toml file
    """

    def __init__(
        self,
        project_root: Optional[Path] = None,
        lock_file_path: Optional[Path] = None,
        pyproject_path: Optional[Path] = None,
    ) -> None:
        """Initialize package scanner.

        Args:
            project_root: Project root directory (defaults to current directory)
            lock_file_path: Path to uv.lock (optional)
            pyproject_path: Path to pyproject.toml (optional)
        """
        self.project_root = project_root or Path.cwd()
        self.lock_file_path = lock_file_path or self.project_root / "uv.lock"
        self.pyproject_path = pyproject_path or self.project_root / "pyproject.toml"

        self._lock_data: Optional[dict] = None
        self._pyproject_data: Optional[dict] = None

    def get_installed_packages(self) -> list[ComponentIdentifier]:
        """Get list of all installed packages.

        Reads from importlib.metadata (installed packages) and enriches
        with information from uv.lock for hashes.

        Returns:
            List of component identifiers
        """
        identifiers: list[ComponentIdentifier] = []

        # Get all installed distributions
        try:
            distributions = importlib.metadata.distributions()

            for dist in distributions:
                try:
                    name = dist.metadata["Name"]
                    version = dist.metadata["Version"]

                    # Create Package URL (PURL)
                    purl = f"pkg:pypi/{name.lower()}@{version}"

                    identifier = ComponentIdentifier(
                        name=name, version=version, purl=purl, cpe=None
                    )

                    identifiers.append(identifier)

                    logger.debug(
                        f"Found package: {name}@{version}",
                        extra={"package": name, "version": version},
                    )

                except Exception as e:
                    logger.warning(
                        f"Failed to process distribution: {e}",
                        extra={"error": str(e)},
                    )

        except Exception as e:
            logger.error(f"Failed to scan installed packages: {e}", extra={"error": str(e)})

        logger.info(
            f"Scanned {len(identifiers)} installed packages",
            extra={"count": len(identifiers)},
        )

        return identifiers

    def get_package_license(self, name: str, version: str) -> Optional[License]:
        """Get license information for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            License if found, None otherwise
        """
        try:
            dist = importlib.metadata.distribution(name)

            # Try License field first
            license_text = dist.metadata.get("License")

            # If not found, try License-Expression (SPDX)
            if not license_text:
                license_text = dist.metadata.get("License-Expression")

            # Try Classifier fields
            if not license_text:
                classifiers = dist.metadata.get_all("Classifier") or []
                for classifier in classifiers:
                    if classifier.startswith("License ::"):
                        # Extract license from classifier
                        # e.g., "License :: OSI Approved :: MIT License"
                        parts = classifier.split(" :: ")
                        if len(parts) >= 3:
                            license_text = parts[-1]
                            break

            if license_text:
                # Normalize license ID (SPDX-like)
                license_id = self._normalize_license_id(license_text)
                return License(
                    id=license_id,
                    name=license_text,
                    url=None,  # Could be enhanced with SPDX license URLs
                )

        except Exception as e:
            logger.debug(
                f"Failed to get license for {name}: {e}",
                extra={"package": name, "error": str(e)},
            )

        return None

    def get_package_description(self, name: str, version: str) -> Optional[str]:
        """Get description for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            Description if found, None otherwise
        """
        try:
            dist = importlib.metadata.distribution(name)
            summary = dist.metadata.get("Summary")
            return summary if summary else None
        except Exception:
            return None

    def get_package_homepage(self, name: str, version: str) -> Optional[str]:
        """Get homepage URL for a package.

        Args:
            name: Package name
            version: Package version

        Returns:
            Homepage URL if found, None otherwise
        """
        try:
            dist = importlib.metadata.distribution(name)

            # Try Home-page field
            homepage = dist.metadata.get("Home-page")
            if homepage and homepage != "UNKNOWN":
                return homepage

            # Try Project-URL fields
            project_urls = dist.metadata.get_all("Project-URL") or []
            for url_entry in project_urls:
                if ", " in url_entry:
                    label, url = url_entry.split(", ", 1)
                    if label.lower() in ["homepage", "home", "website"]:
                        return url

        except Exception:
            pass

        return None

    def get_package_hash(self, name: str, version: str) -> Optional[str]:
        """Get SHA256 hash for a package from uv.lock.

        Args:
            name: Package name
            version: Package version

        Returns:
            SHA256 hash if found, None otherwise
        """
        if not self.lock_file_path.exists():
            logger.debug(
                f"Lock file not found: {self.lock_file_path}",
                extra={"lock_file": str(self.lock_file_path)},
            )
            return None

        try:
            # Load lock file if not already loaded
            if self._lock_data is None:
                with self.lock_file_path.open("rb") as f:
                    self._lock_data = tomllib.load(f)

            # Search for package in lock file
            packages = self._lock_data.get("package", [])
            for package in packages:
                if (
                    package.get("name", "").lower() == name.lower()
                    and package.get("version") == version
                ):
                    # Extract SHA256 from source or wheels
                    source = package.get("source", {})

                    # Check sdist hash
                    sdist = source.get("sdist")
                    if sdist and "hash" in sdist:
                        hash_value = sdist["hash"]
                        # Format: "sha256:abc123..."
                        if hash_value.startswith("sha256:"):
                            return hash_value.split(":", 1)[1]

                    # Check wheel hashes
                    wheels = package.get("wheels", [])
                    if wheels and len(wheels) > 0:
                        wheel_hash = wheels[0].get("hash")
                        if wheel_hash and wheel_hash.startswith("sha256:"):
                            return wheel_hash.split(":", 1)[1]

        except Exception as e:
            logger.debug(
                f"Failed to get hash for {name} from lock file: {e}",
                extra={"package": name, "error": str(e)},
            )

        return None

    def _normalize_license_id(self, license_text: str) -> str:
        """Normalize license text to SPDX-like identifier.

        Args:
            license_text: License text from package metadata

        Returns:
            Normalized SPDX identifier
        """
        # Common mappings
        mappings = {
            "MIT License": "MIT",
            "MIT": "MIT",
            "Apache Software License": "Apache-2.0",
            "Apache License 2.0": "Apache-2.0",
            "Apache 2.0": "Apache-2.0",
            "BSD License": "BSD-3-Clause",
            "BSD": "BSD-3-Clause",
            "GNU General Public License v3": "GPL-3.0",
            "GNU General Public License v3 or later (GPLv3+)": "GPL-3.0-or-later",
            "ISC License (ISCL)": "ISC",
            "ISC": "ISC",
            "Python Software Foundation License": "PSF-2.0",
            "Mozilla Public License 2.0 (MPL 2.0)": "MPL-2.0",
        }

        # Try exact match
        normalized = mappings.get(license_text)
        if normalized:
            return normalized

        # Try pattern matching
        text_lower = license_text.lower()

        if "mit" in text_lower:
            return "MIT"
        if "apache" in text_lower:
            return "Apache-2.0"
        if "bsd" in text_lower:
            if "2-clause" in text_lower:
                return "BSD-2-Clause"
            return "BSD-3-Clause"
        if "gpl" in text_lower:
            if "v3" in text_lower or "3.0" in text_lower:
                return "GPL-3.0"
            if "v2" in text_lower or "2.0" in text_lower:
                return "GPL-2.0"
            return "GPL"
        if "lgpl" in text_lower:
            return "LGPL"
        if "mpl" in text_lower:
            return "MPL-2.0"
        if "isc" in text_lower:
            return "ISC"

        # Return original if no match
        return license_text
