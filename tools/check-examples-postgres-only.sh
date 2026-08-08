#!/usr/bin/env bash
# check-examples-postgres-only.sh — fail if an example provisions or points at a
# database backend FraiseQL does not support.
#
# Background (issue #940): FraiseQL removed the MySQL, SQLite and SQL Server adapters in
# v2.15.0 (#374) — they had never faced a real database, failed on the primary query
# shape, and shipped two security defects (docs/database-compatibility.md). A `mysql://`
# DATABASE_URL is a loud boot refusal.
#
# `examples/federation/saga-basic` nevertheless kept provisioning a MySQL 8.0 service and
# pointing two subgraphs at `mysql://` URLs for three phases afterwards. Its subgraphs are
# Python simulations, so the example still came up — which is worse than a crash: it
# demonstrated, in running form, a multi-database topology the engine explicitly refuses.
#
# The de-scope removed the adapters from `crates/`. Nothing checked `examples/`, which is
# the copy a reader actually runs. This gate is that check.
#
# Adding a backend is a compile-time-visible change at every `DatabaseType` match site
# (see .claude/CLAUDE.md §3). When one lands with a per-dialect integration matrix behind
# it, extend the allowlist below rather than deleting the gate.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

found=0

# Prose *about* the removal is legitimate and must stay greppable — a comment explaining
# why a backend is gone is the opposite of the defect, and this script's own header would
# otherwise trip it. Only lines that would provision, connect to, or import a driver for
# an unsupported backend fail, so comment lines (`#`, `//`, `--`) are dropped first.
# Markdown has no comment syntax, so a `mysql://` inside a README code block still fails.
drop_comments() { grep -vE '^[^:]+:[0-9]+:[[:space:]]*(#|//|--)'; }

echo "→ unsupported connection URLs in examples/"
if hits=$(grep -rniE '(mysql|mariadb|sqlite|sqlserver|mssql|jdbc:[a-z]+)://' examples/ \
    | drop_comments || true); [ -n "$hits" ]; then
    echo "✗ examples/ names a connection URL for a backend FraiseQL removed in #374:"
    echo "$hits"
    found=1
fi

echo "→ unsupported database services in examples/ compose files"
if hits=$(grep -rniE '^\s*image:\s*["'"'"']?(mysql|mariadb|mcr\.microsoft\.com/mssql)' \
    --include='docker-compose*.yml' --include='compose*.yml' examples/ \
    | drop_comments || true); [ -n "$hits" ]; then
    echo "✗ examples/ provisions a database server FraiseQL cannot talk to:"
    echo "$hits"
    found=1
fi

echo "→ unsupported database drivers in examples/"
if hits=$(grep -rniE '(mysql-connector-python|pymysql|mysqlclient|mysql2|mysql\.connector|sqlite3\.connect|pyodbc)' \
    examples/ | drop_comments || true); [ -n "$hits" ]; then
    echo "✗ examples/ installs or imports a driver for an unsupported backend:"
    echo "$hits"
    found=1
fi

if [ "$found" -ne 0 ]; then
    echo ""
    echo "check-examples-postgres-only: FAILED — see the header of this script"
    exit 1
fi

echo "✓ check-examples-postgres-only: every example targets PostgreSQL"
