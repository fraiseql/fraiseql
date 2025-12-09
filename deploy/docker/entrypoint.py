#!/usr/bin/env python3
"""Python-based entrypoint for distroless containers.

Since distroless images don't include a shell, this Python script
serves as the entrypoint for container initialization and health checks.
"""

import os
import subprocess
import sys


def check_env_vars() -> None:
    """Validate required environment variables."""
    required_vars = {
        "DATABASE_URL": "PostgreSQL connection string",
        # Add other required vars here
    }

    missing = []
    for var, description in required_vars.items():
        if not os.getenv(var):
            missing.append(f"{var} ({description})")

    if missing:
        print("ERROR: Missing required environment variables:", file=sys.stderr)
        for var in missing:
            print(f"  - {var}", file=sys.stderr)
        sys.exit(1)


def run_migrations() -> None:
    """Run database migrations if enabled."""
    if os.getenv("FRAISEQL_RUN_MIGRATIONS", "false").lower() == "true":
        print("Running database migrations...")
        # Add migration logic here if needed
        # For now, skip since FraiseQL doesn't have migrations


def main() -> None:
    """Main entrypoint logic."""
    print("FraiseQL Container Starting...")
    print(f"Python version: {sys.version}")
    print(f"Production mode: {os.getenv('FRAISEQL_PRODUCTION', 'false')}")

    # Validate environment
    check_env_vars()

    # Run migrations if needed
    run_migrations()

    # Execute the command passed to the container
    if len(sys.argv) > 1:
        cmd = sys.argv[1:]
        print(f"Executing command: {' '.join(cmd)}")
        try:
            subprocess.execvp(cmd[0], cmd)
        except FileNotFoundError:
            print(f"ERROR: Command not found: {cmd[0]}", file=sys.stderr)
            sys.exit(127)
        except Exception as e:
            print(f"ERROR: Failed to execute command: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        print("ERROR: No command specified", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
