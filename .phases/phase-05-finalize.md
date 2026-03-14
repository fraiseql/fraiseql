# Phase 05: Finalize & Release Prep

## Status
[ ] Not Started

## Objective
Prepare `dev` for merge to `main` and cut the next patch/minor release.
Remove all development scaffolding; leave the repository in "Eternal Sunshine" state.

## Dependencies
- Phases 01–04 all complete
- All CI jobs green on `dev`

## Branch Strategy
Work on a feature branch (e.g. `chore/finalize-v2.1`), merge to `dev` via PR.
SDK publishing is user-initiated only — do not publish without explicit confirmation.

---

## Cycle 1 — Repository archaeology

### Remove development markers
```bash
git grep -i "phase\|todo\|fixme\|hack\|dbg!\|println!" -- ':!*.md' ':!.phases/' | grep -v '// Reason:'
```

Each hit must be either:
- Fixed (for FIXME/TODO describing a real gap)
- Deleted (for development breadcrumbs)
- Suppressed with `// Reason: intentional CLI output` (for `println!`)

### Remove `.phases/` directory
```bash
git rm -r .phases/
```

This directory must not appear in the final release commit.

---

## Cycle 2 — SDK releases (issue #84)

### Problem
C#, Elixir, and F# SDKs are implemented but not published to package registries
(NuGet, Hex). Users cannot install them via standard package managers.

### Fix

#### Step 1 — Dry-run (mandatory before real publish)

**C# and F# → NuGet**
```bash
cd sdks/official/fraiseql-csharp
dotnet pack --configuration Release
# Dry-run: push to local feed or use --dry-run flag
dotnet nuget push **/*.nupkg --source /tmp/nuget-local-feed

cd sdks/official/fraiseql-fsharp
dotnet pack --configuration Release
dotnet nuget push **/*.nupkg --source /tmp/nuget-local-feed
```

**Elixir → Hex**
```bash
cd sdks/official/fraiseql-elixir
mix hex.publish --dry-run
```

Confirm all metadata (name, version, description, license) is correct before proceeding.

#### Step 2 — Real publish (user-initiated only)

Only proceed after user explicitly confirms dry-run output looks good.

```bash
# C# / F#
dotnet nuget push **/*.nupkg --api-key $NUGET_API_KEY --source https://api.nuget.org/v3/index.json

# Elixir
mix hex.publish
```

The `release.yml` CI workflow should automate this on `v*` tags. Verify the
workflow covers all three SDKs, not just Python/TypeScript/Go/Java.

### Verification
- Dry-run output reviewed and approved by user
- `dotnet add package FraiseQL.CSharp` works from a fresh project
- `mix deps.get` with `{:fraiseql, "~> 0.1"}` resolves from Hex

---

## Cycle 3 — Final verification

```bash
# Zero development markers
git grep -i "phase\|todo\|fixme\|hack" -- ':!*.md' ':!CHANGELOG.md'  # must return nothing

# All CI passes (check GitHub Actions)
gh run list --branch dev --limit 10

# Release build
cargo build --release --workspace

# Documentation
cargo doc --workspace --no-deps --all-features

# Nightly fmt
RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check
```

---

## Success Criteria
- [ ] `.phases/` directory removed from repository
- [ ] `git grep` returns nothing for `phase\|todo\|fixme\|hack\|dbg!\|println!` (in source)
- [ ] All CI jobs green on `dev`
- [ ] `cargo build --release --workspace` succeeds
- [ ] `RUSTUP_TOOLCHAIN=nightly cargo fmt --all --check` exits 0
- [ ] C# SDK published to NuGet as `FraiseQL.CSharp`
- [ ] F# SDK published to NuGet as `FraiseQL.FSharp`
- [ ] Elixir SDK published to Hex as `fraiseql`
- [ ] Release tag `v2.1.x` created and pushed
- [ ] `CHANGELOG.md` updated with all changes since v2.1.0

## Closes
- Issue #84 (C#, Elixir, F# SDK releases on package registries)
