# Publishing fraiseql-wire to crates.io

This document explains how the automatic publishing workflow works and how to set it up.

## Automatic Publishing Pipeline

The project uses GitHub Actions to automatically publish new releases to crates.io when you push a git tag.

### How It Works

1. **Push a version tag** (e.g., `v0.2.0`)

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

2. **GitHub Actions automatically:**
   - Runs all tests
   - Checks code formatting
   - Runs clippy linter
   - Verifies the tag version matches Cargo.toml
   - Does a dry-run publish (checks for errors)
   - Publishes to crates.io
   - Creates a GitHub Release with CHANGELOG.md

3. **Result:**
   - Package appears on <https://crates.io/crates/fraiseql-wire>
   - GitHub Release created automatically
   - Documentation updated on docs.rs

## Setup Instructions

### 1. Get crates.io API Token

1. Go to <https://crates.io/me>
2. Click "API Tokens"
3. Create a new token
4. Copy the token (you won't see it again)

### 2. Add Token to GitHub

1. Go to your repository settings:

   ```
   https://github.com/fraiseql/fraiseql-wire/settings/secrets/actions
   ```

2. Click "New repository secret"

3. Add the secret:
   - **Name:** `CARGO_REGISTRY_TOKEN`
   - **Value:** Paste your crates.io API token

4. Save

### 3. Verify Setup

Check that the workflow file exists:

```bash
cat .github/workflows/publish.yml
```

The workflow should:

- Trigger on tags matching `v*` (e.g., v0.1.0, v0.2.0)
- Run tests before publishing
- Verify version matches tag
- Publish to crates.io
- Create GitHub Release

## Publishing a Release

### Process

1. **Update version in Cargo.toml** (if not already done):

   ```toml
   [package]
   version = "0.2.0"  # Change this
   ```

2. **Update CHANGELOG.md** with release notes

3. **Commit changes:**

   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "chore: Prepare v0.2.0 release"
   ```

4. **Create and push git tag:**

   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```

5. **Watch the workflow:**
   - Go to <https://github.com/fraiseql/fraiseql-wire/actions>
   - Click the workflow run
   - Watch logs in real-time

### Example Release

```bash
# 1. Bump version
sed -i 's/version = "0.1.0"/version = "0.2.0"/' Cargo.toml

# 2. Update changelog (manually)
vim CHANGELOG.md

# 3. Commit
git add Cargo.toml CHANGELOG.md
git commit -m "chore: Prepare v0.2.0 release"

# 4. Tag and push
git tag -a v0.2.0 -m "Release v0.2.0 - Description here"
git push origin v0.2.0

# 5. Watch on GitHub
# https://github.com/fraiseql/fraiseql-wire/actions
```

## Workflow Steps Explained

### Test Job

Runs before publishing to catch issues early:

- Unit and integration tests
- Code formatting check
- Clippy linter warnings

**Prevents publishing broken code.**

### Publish Job

Only runs if Test job succeeds:

1. **Verify version matches tag**
   - Reads `Cargo.toml` version
   - Compares with git tag (e.g., `v0.2.0` → `0.2.0`)
   - Fails if they don't match
   - **Prevents accidental version mismatches**

2. **Publish dry-run**
   - Checks package validity
   - Verifies all dependencies exist
   - **Catches packaging issues before real publish**

3. **Publish to crates.io**
   - Uses `CARGO_REGISTRY_TOKEN` secret
   - Uploads package
   - **Makes it available to users**

4. **Wait for indexing**
   - Crates.io needs ~10 seconds to index
   - **Ensures GitHub Release sees the published crate**

5. **Create GitHub Release**
   - Creates release on GitHub
   - Attaches README.md and CHANGELOG.md
   - Uses CHANGELOG.md as release notes
   - **Makes release visible on GitHub**

## Troubleshooting

### Workflow Failed

Check the GitHub Actions logs:

1. Go to <https://github.com/fraiseql/fraiseql-wire/actions>
2. Click the failed workflow
3. Look at the error message
4. Common issues:
   - Version mismatch (tag v0.2.0 but Cargo.toml says 0.1.0)
   - Tests failing
   - Code formatting issues
   - Invalid API token

### Token Expired/Invalid

1. Go to <https://crates.io/me>
2. Regenerate your API token
3. Update the GitHub secret:
   - Go to repository settings → Secrets → Actions
   - Click `CARGO_REGISTRY_TOKEN`
   - Update with new token

### Package Already Published

If you get "cannot overwrite published crate":

- You can't republish the same version
- Use the next version number (e.g., 0.2.1)
- Update Cargo.toml and tag again

### Docs.rs Not Updating

Documentation on docs.rs updates automatically:

1. Check build status at <https://docs.rs/fraiseql-wire>
2. If build failed, see why on docs.rs build logs
3. Usually due to doc comments with syntax errors

## Manual Publishing (If Needed)

If the workflow fails and you need to publish manually:

```bash
# 1. Verify everything locally
cargo test
cargo publish --dry-run

# 2. Publish
cargo publish --token YOUR_TOKEN_HERE

# 3. Create GitHub Release manually
gh release create v0.2.0 \
  --title "fraiseql-wire v0.2.0" \
  --notes-file CHANGELOG.md
```

## Semantic Versioning

Follow [semver](https://semver.org/):

- **0.1.0 → 0.2.0**: Minor version bump (new features)
- **0.1.0 → 0.1.1**: Patch version bump (bug fixes)
- **0.1.0 → 1.0.0**: Major version bump (breaking changes)

## Security Notes

- **Never commit API tokens** to the repository
- **Use GitHub Secrets** for sensitive values
- **Rotate tokens regularly** (crates.io best practice)
- **Limit token scope** (publish-only, not account admin)

## See Also

- [crates.io publishing guide](https://doc.rust-lang.org/cargo/publish/)
- [Semantic versioning](https://semver.org/)
- [GitHub Actions documentation](https://docs.github.com/en/actions)
