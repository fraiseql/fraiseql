#!/usr/bin/env bash
# Red-capability pin for tools/check-doctest-skip.py.
#
# Run directly:  bash tools/tests/doctest_skip_test.sh
# Exits non-zero if any assertion fails.
#
# Each case is a fixture tree with no `.git`, holding every crate the check skips with
# a clean library, and one change to one of them. The refused cases are each way a
# doctest can be written; the accepted ones are documentation the check must not
# mistake for one.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-doctest-skip.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mapfile -t SKIPPED < <(python3 "$GATE")
if ((${#SKIPPED[@]} == 0)); then
    echo "FAIL: the check printed no skipped crate on this tree" >&2
    exit 1
fi
TARGET="${SKIPPED[0]}"

# new_case <id>: every skipped crate, each a library with documentation and no doctest.
new_case() {
    root="$WORK/$1"
    for name in "${SKIPPED[@]}"; do
        mkdir -p "$root/crates/$name/src"
        printf '[package]\nname = "%s"\n\n[dependencies]\n' "$name" >"$root/crates/$name/Cargo.toml"
        printf '//! A crate.\n\n/// Returns one.\npub const fn one() -> u8 {\n    1\n}\n' \
            >"$root/crates/$name/src/lib.rs"
    done
    lib="$root/crates/$TARGET/src/lib.rs"
}

# run_case <id>: the check's exit code in $rc and its output in $out.
run_case() {
    out=$(DOCTEST_SKIP_ROOT="$WORK/$1" python3 "$GATE" 2>&1)
    rc=$?
}

refused() {
    local id=$1 what=$2 needle=$3
    TESTS_RUN=$((TESTS_RUN + 1))
    run_case "$id"
    if ((rc != 1)); then
        echo "FAIL [$id]: $what: expected exit 1, got $rc" >&2
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    elif [[ $out != *"$needle"* ]]; then
        echo "FAIL [$id]: $what: exit 1, but the output does not name '$needle'" >&2
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

accepted() {
    local id=$1 what=$2
    TESTS_RUN=$((TESTS_RUN + 1))
    run_case "$id"
    if ((rc != 0)); then
        echo "FAIL [$id]: $what: expected exit 0, got $rc" >&2
        printf '%s\n' "$out" | sed 's/^/    /' >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    elif [[ $out != *"$TARGET"* ]]; then
        echo "FAIL [$id]: $what: exit 0, but $TARGET is not in the printed list" >&2
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# doc_block <id> <prefix> <info> : a fenced block under `one`'s doc comment.
doc_block() {
    new_case "$1"
    printf '%s Example.\n%s\n%s %s\n%s let _ = 1;\n%s %s\n' \
        "$2" "$2" "$2" "$3" "$2" "$2" "${3%%[!\`~]*}" >>"$lib"
}

new_case clean
accepted clean "libraries with prose only"

doc_block bare '///' '```'
refused bare "a bare fence" "lib.rs:9"
doc_block inner '//!' '```'
refused inner "a bare fence in an inner doc comment" "lib.rs:9"
doc_block rust '///' '```rust'
refused rust "a \`rust\` fence" "rust"
doc_block no-run '///' '```no_run'
refused no-run "a \`no_run\` fence" "no_run"
doc_block ignore '///' '```ignore'
refused ignore "an \`ignore\` fence, one word from running" "ignore"
doc_block mixed '///' '```json,no_run'
refused mixed "a known word beside a rustdoc attribute" "json,no_run"
doc_block unknown '///' '```rs'
refused unknown "a word the check does not know" "rs"
doc_block long '///' '````'
refused long "a four-backtick fence" "lib.rs:9"
doc_block tilde '///' '~~~'
refused tilde "a tilde fence" "lib.rs:9"
doc_block indented-fence '///' '   ```'
refused indented-fence "a fence indented three spaces" "lib.rs:9"

new_case indented
printf '/// Example:\n///\n///     let _ = 1;\npub const TWO: u8 = 2;\n' >>"$lib"
refused indented "an indented code block" "an indented code block"

new_case readme
printf '#![doc = include_str!("../README.md")]\n' >>"$lib"
refused readme "a README included as docs" "doc = …"

new_case cfg-attr
printf '#![cfg_attr(feature = "x", doc = "text")]\n' >>"$lib"
refused cfg-attr "a doc attribute under cfg_attr" "doc = …"

new_case block
printf '/** Two. */\npub const TWO: u8 = 2;\n' >>"$lib"
refused block "a /** block comment" "block comment"

new_case path
printf '#[path = "../../other/x.rs"]\nmod x;\n' >>"$lib"
refused path "a #[path] module" "#[path"

new_case lib-path
printf '\n[lib]\npath = "../other/lib.rs"\n' >>"$root/crates/$TARGET/Cargo.toml"
refused lib-path "a [lib] path" "[lib]"

new_case submodule
mkdir -p "$root/crates/$TARGET/src/a/b"
printf '/// ```\n/// let _ = 1;\n/// ```\npub fn f() {}\n' >"$root/crates/$TARGET/src/a/b/c.rs"
refused submodule "a fence in a nested module file" "src/a/b/c.rs:1"

new_case after-text
printf '/// ```text\n/// a\n/// ```\n///\n/// ```\n/// let _ = 1;\n/// ```\npub const TWO: u8 = 2;\n' >>"$lib"
refused after-text "a Rust fence after a closed text fence" "lib.rs:11"

new_case next-item
printf '/// ```text\n/// never closed\npub const TWO: u8 = 2;\n/// ```\n/// let _ = 1;\n/// ```\npub const THREE: u8 = 3;\n' >>"$lib"
refused next-item "a Rust fence in the doc block after an unclosed one" "lib.rs:10"

new_case gone
rm -r "${root:?}/crates/$TARGET"
refused gone "a skipped crate with no directory" "no crates/$TARGET/Cargo.toml"

for word in text json toml bash sh shell console sql graphql yaml; do
    doc_block "word-$word" '///' "\`\`\`$word"
    accepted "word-$word" "a \`$word\` fence"
done

new_case closed
printf '/// ```text\n/// a\n/// ```\n///\n/// - a list item\n///   continued\n///\n/// Then prose.\npub const TWO: u8 = 2;\n' >>"$lib"
accepted closed "a closed text fence, then a list with continuation lines"

new_case nested-fence
printf '/// ````text\n/// ```\n/// let _ = 1;\n/// ```\n/// ````\n///    three spaces of indent are text\npub const TWO: u8 = 2;\n' >>"$lib"
accepted nested-fence "a shorter fence inside a longer text one, and a three-space indent"

new_case tests-dir
mkdir -p "$root/crates/$TARGET/tests"
printf '/// ```\n/// let _ = 1;\n/// ```\n#[test]\nfn t() {}\n' >"$root/crates/$TARGET/tests/t.rs"
accepted tests-dir "a fence in an integration test, which rustdoc never reads"

new_case plain-comment
printf '// ```\n// let _ = 1;\n// ```\n//// ```\npub const TWO: u8 = 2;\n' >>"$lib"
accepted plain-comment "a fence in a plain or four-slash comment"

echo "doctest_skip_test: $((TESTS_RUN - TESTS_FAILED))/$TESTS_RUN passed"
((TESTS_FAILED == 0))
