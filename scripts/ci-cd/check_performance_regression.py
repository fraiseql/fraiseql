#!/usr/bin/env python3
"""
Performance Regression Checker for FraiseQL CI/CD

This script compares current performance metrics against a baseline
and fails the CI if performance has regressed beyond acceptable thresholds.
"""

import json
import sys
from pathlib import Path
from typing import Dict, Any, Tuple


def load_json_results(file_path: str) -> Dict[str, Any]:
    """Load performance results from JSON file."""
    try:
        with open(file_path, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"❌ Error loading {file_path}: {e}")
        sys.exit(1)


def compare_metrics(current: Dict[str, Any], baseline: Dict[str, Any]) -> Tuple[bool, str]:
    """
    Compare current metrics against baseline.

    Returns:
        (passed: bool, message: str)
    """
    issues = []
    warnings = []

    # Define acceptable regression thresholds (percentage)
    THRESHOLDS = {
        "query_time": 10.0,  # 10% slower is acceptable
        "memory_usage": 15.0,  # 15% more memory is acceptable
        "throughput": -5.0,  # 5% lower throughput is acceptable
        "latency_p95": 15.0,  # 15% higher latency is acceptable
    }

    # Compare key metrics
    for metric, threshold in THRESHOLDS.items():
        if metric in current and metric in baseline:
            current_val = current[metric]
            baseline_val = baseline[metric]

            if baseline_val == 0:
                continue  # Skip division by zero

            # Calculate percentage change
            if metric == "throughput":
                # For throughput, higher is better
                change_pct = ((current_val - baseline_val) / baseline_val) * 100
                if change_pct < threshold:  # threshold is negative for throughput
                    issues.append(
                        f"🚨 {metric}: {change_pct:.1f}% change (threshold: {threshold}%)"
                    )
            else:
                # For other metrics, lower is better
                change_pct = ((current_val - baseline_val) / baseline_val) * 100
                if change_pct > threshold:  # threshold is positive for degradation
                    issues.append(
                        f"🚨 {metric}: {change_pct:.1f}% change (threshold: {threshold}%)"
                    )
                elif change_pct > (threshold * 0.5):  # Warning at half threshold
                    warnings.append(f"⚠️  {metric}: {change_pct:.1f}% change approaching threshold")

    # Check for missing metrics
    current_metrics = set(current.keys())
    baseline_metrics = set(baseline.keys())

    missing_in_current = baseline_metrics - current_metrics
    missing_in_baseline = current_metrics - baseline_metrics

    if missing_in_current:
        warnings.append(f"⚠️  Metrics missing in current results: {', '.join(missing_in_current)}")

    if missing_in_baseline:
        warnings.append(f"⚠️  New metrics in current results: {', '.join(missing_in_baseline)}")

    # Build result message
    message_parts = []

    if issues:
        message_parts.append("🚨 PERFORMANCE REGRESSION DETECTED:")
        message_parts.extend(issues)
        return False, "\n".join(message_parts)

    if warnings:
        message_parts.append("⚠️  Performance warnings:")
        message_parts.extend(warnings)

    message_parts.append("✅ No performance regressions detected")

    return True, "\n".join(message_parts)


def main():
    """Main entry point."""
    if len(sys.argv) != 3:
        print(
            "Usage: python check_performance_regression.py <current_results.json> <baseline_results.json>"
        )
        sys.exit(1)

    current_file = sys.argv[1]
    baseline_file = sys.argv[2]

    print("🔍 Checking for performance regressions...")
    print(f"Current results: {current_file}")
    print(f"Baseline results: {baseline_file}")
    print()

    # Load results
    current_results = load_json_results(current_file)
    baseline_results = load_json_results(baseline_file)

    # Compare metrics
    passed, message = compare_metrics(current_results, baseline_results)

    print(message)

    if not passed:
        print("\n❌ Performance regression check FAILED")
        print("💡 Consider optimizing the code or updating baseline thresholds")
        sys.exit(1)
    else:
        print("\n✅ Performance regression check PASSED")
        sys.exit(0)


if __name__ == "__main__":
    main()
