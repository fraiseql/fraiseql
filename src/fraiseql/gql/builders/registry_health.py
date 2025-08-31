"""Registry health monitoring for detecting and diagnosing issues."""

from __future__ import annotations

import inspect
from dataclasses import dataclass
from typing import Any, Dict, List


@dataclass
class RegistryHealthResult:
    """Result of a registry health check."""

    is_healthy: bool
    issues: List[str]
    warnings: List[str]
    diagnostic_info: Dict[str, Any]
    severity: str  # "healthy", "warning", "critical"

    @property
    def has_critical_issues(self) -> bool:
        """Check if there are critical issues that would prevent operation."""
        return self.severity == "critical"

    @property
    def summary(self) -> str:
        """Get a human-readable summary of the health check."""
        if self.is_healthy:
            total_registrations = self._get_total_registrations()
            return f"Registry healthy with {total_registrations} total registrations"

        issues_count = len(self.issues)
        warnings_count = len(self.warnings)

        return (
            f"Registry {self.severity}: {issues_count} issues, {warnings_count} warnings. "
            f"See diagnostic_info for details."
        )

    def _get_total_registrations(self) -> int:
        """Get total number of registrations from diagnostic info."""
        return (
            self.diagnostic_info.get("query_count", 0)
            + self.diagnostic_info.get("mutation_count", 0)
            + self.diagnostic_info.get("subscription_count", 0)
            + self.diagnostic_info.get("type_count", 0)
        )


class RegistryHealthChecker:
    """Health checker for SchemaRegistry instances."""

    def __init__(self, registry) -> None:
        """Initialize health checker with registry reference."""
        self.registry = registry

    def check_health(self) -> RegistryHealthResult:
        """Perform comprehensive health check on the registry.

        Returns:
            RegistryHealthResult with detailed health information
        """
        issues = []
        warnings = []
        diagnostic_info = {}

        # Basic registry state checks
        query_count = len(self.registry.queries)
        mutation_count = len(self.registry.mutations)
        subscription_count = len(self.registry.subscriptions)
        type_count = len(self.registry.types)

        diagnostic_info.update(
            {
                "query_count": query_count,
                "mutation_count": mutation_count,
                "subscription_count": subscription_count,
                "type_count": type_count,
                "total_registrations": query_count
                + mutation_count
                + subscription_count
                + type_count,
            }
        )

        # Check for empty registry (critical issue)
        if query_count == 0 and mutation_count == 0 and subscription_count == 0:
            issues.append(
                "Registry appears completely empty. This often indicates:\n"
                "  - Database connection issues\n"
                "  - Duplicate query registrations corrupting the registry\n"
                "  - Import path conflicts\n"
                "  - Schema building process not completed"
            )

        # Check for suspiciously empty query registry
        if query_count == 0 and (mutation_count > 0 or subscription_count > 0):
            issues.append(
                "Query registry is empty but other types are registered. "
                "This may indicate query registration corruption."
            )

        # Check for duplicate function detection
        duplicate_info = self._check_for_potential_duplicates()
        if duplicate_info["potential_duplicates"]:
            warnings.extend(duplicate_info["warnings"])
            diagnostic_info["potential_duplicates"] = duplicate_info["potential_duplicates"]

        # Check for function signature issues
        signature_issues = self._check_function_signatures()
        if signature_issues:
            warnings.extend(signature_issues)
            diagnostic_info["signature_issues"] = len(signature_issues)

        # Check for import path issues
        import_issues = self._check_import_paths()
        if import_issues:
            warnings.extend(import_issues)
            diagnostic_info["import_issues"] = len(import_issues)

        # Determine overall health
        is_healthy = len(issues) == 0
        if len(issues) > 0:
            severity = "critical"
        elif len(warnings) > 0:
            severity = "warning"
        else:
            severity = "healthy"

        return RegistryHealthResult(
            is_healthy=is_healthy,
            issues=issues,
            warnings=warnings,
            diagnostic_info=diagnostic_info,
            severity=severity,
        )

    def _check_for_potential_duplicates(self) -> Dict[str, Any]:
        """Check for potential duplicate registrations."""
        potential_duplicates = []
        warnings = []

        # Group functions by module to detect potential issues
        module_groups: Dict[str, List[str]] = {}
        for name, func in self.registry.queries.items():
            if hasattr(func, "__module__"):
                module = func.__module__
                if module not in module_groups:
                    module_groups[module] = []
                module_groups[module].append(name)

        # Look for modules with multiple functions (not necessarily bad, but worth noting)
        for module, functions in module_groups.items():
            if len(functions) > 10:  # Arbitrary threshold
                warnings.append(
                    f"Module '{module}' has {len(functions)} registered queries. "
                    "Consider checking for potential duplicate imports."
                )

        return {
            "potential_duplicates": potential_duplicates,
            "warnings": warnings,
            "module_groups": module_groups,
        }

    def _check_function_signatures(self) -> List[str]:
        """Check for common function signature issues."""
        issues = []

        for name, func in self.registry.queries.items():
            try:
                sig = inspect.signature(func)
                params = list(sig.parameters.keys())

                # Check for missing 'info' parameter
                if not params or params[0] != "info":
                    issues.append(
                        f"Query function '{name}' may be missing 'info' as first parameter"
                    )

                # Check for missing return type annotation
                if sig.return_annotation == inspect.Signature.empty:
                    issues.append(f"Query function '{name}' is missing return type annotation")

            except Exception as e:
                issues.append(f"Could not inspect signature of query '{name}': {e}")

        return issues

    def _check_import_paths(self) -> List[str]:
        """Check for import path related issues."""
        issues = []

        # Check for functions from test modules in production registries
        for name, func in self.registry.queries.items():
            if hasattr(func, "__module__"):
                module = func.__module__
                if any(
                    test_indicator in module.lower()
                    for test_indicator in ["test_", "_test", "tests."]
                ):
                    issues.append(
                        f"Query '{name}' appears to be from a test module: {module}. "
                        "This may indicate incorrect imports."
                    )

        return issues

    def generate_diagnostic_report(self) -> str:
        """Generate a comprehensive diagnostic report."""
        health = self.check_health()

        report = []
        report.append("=" * 50)
        report.append("FraiseQL Registry Health Report")
        report.append("=" * 50)
        report.append(f"Overall Status: {health.severity.upper()}")
        report.append(f"Summary: {health.summary}")
        report.append("")

        # Registry contents
        report.append("Registry Contents:")
        report.append(f"  Queries: {health.diagnostic_info['query_count']}")
        report.append(f"  Mutations: {health.diagnostic_info['mutation_count']}")
        report.append(f"  Subscriptions: {health.diagnostic_info['subscription_count']}")
        report.append(f"  Types: {health.diagnostic_info['type_count']}")
        report.append("")

        # Issues
        if health.issues:
            report.append("CRITICAL ISSUES:")
            for i, issue in enumerate(health.issues, 1):
                report.append(f"  {i}. {issue}")
            report.append("")

        # Warnings
        if health.warnings:
            report.append("WARNINGS:")
            for i, warning in enumerate(health.warnings, 1):
                report.append(f"  {i}. {warning}")
            report.append("")

        # Diagnostic info
        if health.diagnostic_info:
            report.append("DIAGNOSTIC INFORMATION:")
            for key, value in health.diagnostic_info.items():
                if key not in ["query_count", "mutation_count", "subscription_count", "type_count"]:
                    report.append(f"  {key}: {value}")

        report.append("=" * 50)

        return "\n".join(report)
