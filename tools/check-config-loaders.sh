#!/usr/bin/env bash
# check-config-loaders.sh — fail if a typed TOML config loader exists without a
# config-coverage manifest that owns its keys.
#
# Background (issue #909). `fraiseql_core::config::FraiseQLConfig` deserialized a full
# `[server]`/`[database]`/`[cors]`/`[auth]`/`[rate_limit]`/`[cache]` TOML tree, validated
# it, round-tripped it in its own tests — and no code outside its own module ever read a
# field. An operator who wrote
#
#     [cache]
#     response_cache_enabled = true
#
# got a config that parsed, validated, and did nothing.
#
# The two loaders an operator actually reaches — `ServerConfig` (server.toml) and
# `TomlProjectConfig`/`TomlSchema` (fraiseql.toml) — each have a checked-in coverage
# manifest that maps every accepted key to the subsystem that consumes it (#612 item M).
# The third loader had none, which is exactly why nobody noticed it consumed nothing.
#
# The rule this gate enforces: **a TOML surface an operator can write must have a manifest
# naming each key's consumer.** A new typed loader is either allow-listed here against a
# real coverage test, or it is an unconsumed surface waiting to happen.
#
# Untyped introspection (`toml::Value` / `toml::Table`) is not a config surface — it reads
# a file the typed loader already owns (feature-hint enrichment, `fraiseql run`'s
# ignored-section advisory, doctor, sbom) — and is always allowed.
#
# Adding a loader? Put it in tools/config-loaders.allow with the manifest test that owns
# its keys, and write that manifest. Not the other way round.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

ALLOW='tools/config-loaders.allow'
found=0

# --- Rule 1: every typed loader is allow-listed against a manifest ------------------
echo "→ typed TOML config loaders"

# Test code is not a config surface: a fixture parsed in a test has no operator.
is_test_path() {
    case "$1" in
    */tests/* | *tests.rs | *test.rs) return 0 ;;
    *) return 1 ;;
    esac
}

while IFS= read -r hit; do
    file=${hit%%:*}
    rest=${hit#*:}
    line=${rest%%:*}

    is_test_path "$file" && continue

    # Untyped read: the type appears turbofished on the call or annotated on the
    # binding the call feeds, which `toml::from_str` splits across two lines when
    # rustfmt wraps it. Two lines of context covers both forms.
    ctx=$(sed -n "$((line > 1 ? line - 1 : 1)),$((line + 1))p" "$file")
    case "$ctx" in
    *"toml::Value"* | *"toml::Table"*) continue ;;
    esac

    # Allow-listed? Entries are `<path>  # <manifest-or-reason>`.
    entry=$(grep -v '^[[:space:]]*#' "$ALLOW" | grep -F "$file" || true)
    if [ -z "$entry" ]; then
        echo "✗ typed TOML loader with no config-coverage manifest: $file:$line"
        echo "    every key an operator can write must have a named consumer (#909, #612)."
        echo "    Add a coverage manifest test, then list the loader in $ALLOW."
        found=1
        continue
    fi

    # The manifest an entry names must exist. An entry pointing at nothing is the
    # same unverified claim the gate exists to refuse.
    manifest=$(printf '%s\n' "$entry" | sed -n 's/.*#[[:space:]]*manifest:[[:space:]]*//p' | head -1)
    if [ -n "$manifest" ] && [ ! -f "$manifest" ]; then
        echo "✗ $file is allow-listed against a manifest that does not exist: $manifest"
        found=1
    fi
done < <(grep -rn --include='*.rs' 'toml::from_str' crates/*/src/ || true)

# --- Rule 2: fraiseql-core owns no operator TOML surface ----------------------------
# Authoring config is the CLI's (`fraiseql.toml`); runtime config is the server's
# (`server.toml`). A loader in the engine crate has no entry point to be reached from,
# which is how #909's tree stayed inert through four releases.
echo "→ no operator TOML surface in fraiseql-core"
if hits=$(grep -rn --include='*.rs' 'toml::from_str' crates/fraiseql-core/src/ | grep -v 'tests\.rs' || true); [ -n "$hits" ]; then
    echo "✗ fraiseql-core parses operator TOML — that surface belongs to fraiseql-cli"
    echo "  (authoring) or fraiseql-server (runtime), each of which has a coverage gate:"
    echo "$hits"
    found=1
fi

if [ "$found" -ne 0 ]; then
    echo ""
    echo "check-config-loaders: FAILED — see the header of tools/check-config-loaders.sh"
    exit 1
fi

echo "✓ check-config-loaders: every TOML config surface has a manifest"
