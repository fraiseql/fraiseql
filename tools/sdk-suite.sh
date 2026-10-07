#!/usr/bin/env bash
# Run one official SDK's own test suite and linters: the one definition of them.
#
#   tools/sdk-suite.sh <sdk>
#
# <sdk> is a key of sdks/official/conformance/manifest.json (python, typescript, go, php,
# java, csharp, fsharp, elixir, ruby, dart, rust). Runs from the SDK's directory, whatever
# the caller's, and assumes the SDK's toolchain is installed. `.github/workflows/sdk-suites.yml`
# calls this after its setup steps (#1467), and `make test-sdks` calls it for every SDK
# (tools/test-sdks.sh, which falls back to a container when a toolchain is missing).
#
# Why one script (#1346): SDK conformance drives what each SDK *emits*, which is a
# different check from each SDK's own unit suite, and no local target ran the latter.
# Four SDK workflows went red on a push that every local gate had passed. With the
# commands here, the local run and the CI run cannot drift apart.
set -euo pipefail

sdk="${1:?usage: tools/sdk-suite.sh <sdk>}"
root="$(cd "$(dirname "$0")/.." && pwd)"

run() { echo "+ $*"; "$@"; }

case "$sdk" in
python)
    cd "$root/sdks/official/fraiseql-python"
    # --locked: a lockfile that disagrees with pyproject.toml fails here instead of being
    # silently re-resolved for the commands below (#1225).
    run uv sync --all-extras --locked
    # `conformance` and `examples` are shipped Python that authors copy; leaving them out
    # of the lint is how three examples rotted into code that does not run (#925).
    run uv run ruff check src tests conformance examples
    run uv run ruff format --check src tests conformance examples
    run uv run ty check src
    run uv run pytest tests -q
    ;;
typescript)
    cd "$root/sdks/official/fraiseql-typescript"
    run npm ci
    run npm run lint
    run npm run typecheck
    run npm test
    run npm run build
    ;;
go)
    cd "$root/sdks/official/fraiseql-go"
    run go mod verify
    run go vet ./fraiseql/...
    run go run honnef.co/go/tools/cmd/staticcheck@v0.6.0 ./...
    run go test -race ./fraiseql/...
    run go build ./...
    ;;
php)
    cd "$root/sdks/official/fraiseql-php"
    run composer install --no-progress --prefer-dist
    # PSR-4 compliance, which nothing else checks (#1184). A plain install's autoloader
    # cannot find a class whose file name does not match it; `--optimize` alone hides
    # that, so a production deploy would not reproduce it. `--strict-psr` exits 1.
    run composer dump-autoload --optimize --strict-psr
    run vendor/bin/phpstan analyse src --level 8
    run vendor/bin/php-cs-fixer check src --diff
    run vendor/bin/phpunit
    ;;
java)
    cd "$root/sdks/official/fraiseql-java"
    run mvn -B verify
    # A real gate: `pom.xml` declares checkstyle against `checkstyle.xml` with
    # `violationSeverity=warning` and the tree is clean. It once ran under
    # `continue-on-error` over 2149 violations and rendered green (#1252).
    run mvn -B checkstyle:check
    ;;
csharp | fsharp)
    cd "$root/sdks/official/fraiseql-$sdk"
    run dotnet restore
    run dotnet build --configuration Release --no-restore
    # FRAISEQL_SDK_COVERAGE=1 collects coverage (the F# workflow uploads it).
    if [ "${FRAISEQL_SDK_COVERAGE:-}" = 1 ]; then
        run dotnet test --no-build --configuration Release --collect "XPlat Code Coverage"
    else
        run dotnet test --no-build --configuration Release
    fi
    ;;
elixir)
    cd "$root/sdks/official/fraiseql-elixir"
    run mix deps.get
    run mix credo
    run mix test
    ;;
ruby)
    cd "$root/sdks/official/fraiseql-ruby"
    # No Gemfile: the SDK depends on the standard library only, its tests on minitest.
    gem list -i minitest >/dev/null || run gem install minitest --no-document
    shopt -s nullglob
    files=(test/*_test.rb)
    # Loud when the glob matches nothing: an empty run is a false green.
    [ ${#files[@]} -gt 0 ] || { echo "no test/*_test.rb files: the suite would pass vacuously" >&2; exit 1; }
    run ruby -Ilib -Itest -e 'ARGV.each { |f| require File.expand_path(f) }' "${files[@]}"
    # `gem build` evaluates the gemspec, which loads lib/fraiseql/version (#853).
    run gem build ./fraiseql.gemspec
    rm -f ./fraiseql-*.gem
    ;;
dart)
    cd "$root/sdks/official/fraiseql-dart"
    run dart pub get
    run dart analyze --fatal-infos
    run dart format --output=none --set-exit-if-changed .
    run dart test
    ;;
rust)
    # Its own cargo workspace: the root `cargo fmt --all` never descends into it.
    cd "$root/sdks/official/fraiseql-rust"
    run cargo fmt --check
    # --locked on the FIRST lock-touching command: a bare clippy would quietly rewrite a
    # stale Cargo.lock and let the later `--locked` test pass on the repaired lock (#1225).
    run cargo clippy --locked --all-targets --all-features -- -D warnings
    run cargo test --locked --all-features
    ;;
*)
    echo "unknown SDK: $sdk" >&2
    exit 2
    ;;
esac
