#!/usr/bin/env python3
"""
Detect system CPU capabilities and determine appropriate benchmark profile.
"""

import contextlib
import json
import multiprocessing
import re
import subprocess
from pathlib import Path

try:
    import psutil

    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


def get_cpu_info():
    """Get CPU information from various sources."""
    info = {
        "cores": multiprocessing.cpu_count(),
        "model": "Unknown",
        "frequency_mhz": 0,
        "cache_size": 0,
        "memory_gb": 0,
    }

    # Get CPU model and cache from lscpu
    try:
        lscpu_output = subprocess.check_output(["lscpu"], text=True)

        # Extract CPU model
        model_match = re.search(r"Model name:\s+(.+)", lscpu_output)
        if model_match:
            info["model"] = model_match.group(1).strip()

        # Extract cache size (L3 or last level cache)
        cache_match = re.search(r"L3 cache:\s+(\d+)", lscpu_output)
        if cache_match:
            info["cache_size"] = int(cache_match.group(1))
        else:
            # Try to find any cache info
            cache_match = re.search(r"cache:\s+(\d+)K", lscpu_output, re.IGNORECASE)
            if cache_match:
                info["cache_size"] = int(cache_match.group(1))

        # Extract CPU MHz
        freq_match = re.search(r"CPU MHz:\s+(\d+)", lscpu_output)
        if freq_match:
            info["frequency_mhz"] = int(float(freq_match.group(1)))
        else:
            # Try CPU max MHz
            freq_match = re.search(r"CPU max MHz:\s+(\d+)", lscpu_output)
            if freq_match:
                info["frequency_mhz"] = int(float(freq_match.group(1)))
    except (subprocess.CalledProcessError, FileNotFoundError):
        pass

    # Get memory info
    if HAS_PSUTIL:
        with contextlib.suppress(Exception):
            info["memory_gb"] = round(psutil.virtual_memory().total / (1024**3), 1)

    if info["memory_gb"] == 0:
        try:
            # Fallback to /proc/meminfo
            meminfo_path = Path("/proc/meminfo")
            with meminfo_path.open() as f:
                for line in f:
                    if line.startswith("MemTotal:"):
                        kb = int(re.search(r"(\d+)", line).group(1))
                        info["memory_gb"] = round(kb / (1024**2), 1)
                        break
        except Exception:
            pass

    return info


def calculate_cpu_score(cpu_info):
    """Calculate a performance score based on CPU characteristics."""
    score = 0

    # Core count (max 100 points)
    score += min(cpu_info["cores"] * 10, 100)

    # Frequency (max 100 points, normalized around 3GHz)
    if cpu_info["frequency_mhz"] > 0:
        score += min((cpu_info["frequency_mhz"] / 3000) * 100, 100)

    # Cache size (max 50 points, normalized around 8MB)
    if cpu_info["cache_size"] > 0:
        cache_mb = cpu_info["cache_size"] / 1024
        score += min((cache_mb / 8) * 50, 50)

    # Memory (max 50 points, normalized around 16GB)
    score += min((cpu_info["memory_gb"] / 16) * 50, 50)

    # CPU generation/features bonus based on model
    model_lower = cpu_info["model"].lower()
    if "xeon" in model_lower or "epyc" in model_lower:
        score += 50  # Server CPU bonus
    elif "i9" in model_lower or "ryzen 9" in model_lower:
        score += 40
    elif "i7" in model_lower or "ryzen 7" in model_lower:
        score += 30
    elif "i5" in model_lower or "ryzen 5" in model_lower:
        score += 20
    elif "i3" in model_lower or "ryzen 3" in model_lower:
        score += 10

    # Modern CPU architecture bonus
    if any(gen in model_lower for gen in ["12th", "13th", "14th", "zen 3", "zen 4", "zen 5"]):
        score += 30
    elif any(gen in model_lower for gen in ["10th", "11th", "zen 2"]):
        score += 20
    elif any(gen in model_lower for gen in ["8th", "9th", "zen", "zen+"]):
        score += 10

    return score


def determine_profile(cpu_info, score):
    """Determine the appropriate benchmark profile based on CPU score."""
    profiles = {
        "minimal": {
            "name": "Minimal (Low-spec laptop)",
            "users": 100,
            "products": 500,
            "orders": 200,
            "description": "For testing and low-end hardware",
        },
        "small": {
            "name": "Small (Standard laptop)",
            "users": 1_000,
            "products": 5_000,
            "orders": 2_000,
            "description": "For typical developer laptops",
        },
        "medium": {
            "name": "Medium (High-end laptop/Desktop)",
            "users": 10_000,
            "products": 50_000,
            "orders": 20_000,
            "description": "For powerful laptops and standard desktops",
        },
        "large": {
            "name": "Large (Workstation)",
            "users": 50_000,
            "products": 200_000,
            "orders": 100_000,
            "description": "For workstations and small servers",
        },
        "xlarge": {
            "name": "Extra Large (Server)",
            "users": 100_000,
            "products": 1_000_000,
            "orders": 5_000_000,
            "description": "For production servers and benchmarking",
        },
    }

    # Determine profile based on score
    if score < 100:
        profile = "minimal"
    elif score < 200:
        profile = "small"
    elif score < 300:
        profile = "medium"
    elif score < 400:
        profile = "large"
    else:
        profile = "xlarge"

    # Memory-based override - don't use xlarge on systems with less than 8GB
    if profile == "xlarge" and cpu_info["memory_gb"] < 8:
        profile = "large"
    if profile == "large" and cpu_info["memory_gb"] < 4:
        profile = "medium"
    if profile == "medium" and cpu_info["memory_gb"] < 2:
        profile = "small"

    return profile, profiles[profile]


def main():
    """Main function to detect and recommend benchmark profile."""
    print("=== FraiseQL Benchmark Profile Detector ===\n")

    # Get CPU info
    cpu_info = get_cpu_info()
    print(f"CPU Model: {cpu_info['model']}")
    print(f"CPU Cores: {cpu_info['cores']}")
    print(f"CPU Frequency: {cpu_info['frequency_mhz']} MHz")
    print(f"Cache Size: {cpu_info['cache_size']} KB")
    print(f"System Memory: {cpu_info['memory_gb']} GB")

    # Calculate score
    score = calculate_cpu_score(cpu_info)
    print(f"\nPerformance Score: {score}/430")

    # Determine profile
    profile_name, profile_details = determine_profile(cpu_info, score)

    print(f"\nRecommended Profile: {profile_details['name']}")
    print(f"Description: {profile_details['description']}")
    print("Data Scale:")
    print(f"  - Users: {profile_details['users']:,}")
    print(f"  - Products: {profile_details['products']:,}")
    print(f"  - Orders: {profile_details['orders']:,}")

    # Save configuration
    config = {
        "profile": profile_name,
        "cpu_info": cpu_info,
        "score": score,
        "data_scale": {
            "users": profile_details["users"],
            "products": profile_details["products"],
            "orders": profile_details["orders"],
        },
    }

    config_file = Path("benchmark_profile.json")
    with config_file.open("w") as f:
        json.dump(config, f, indent=2)

    print(f"\nConfiguration saved to: {config_file}")

    # Also export as environment variable for scripts
    print("\nTo use this profile, run:")
    print(f"export BENCHMARK_PROFILE={profile_name}")

    return profile_name


if __name__ == "__main__":
    profile = main()
    exit(0)
