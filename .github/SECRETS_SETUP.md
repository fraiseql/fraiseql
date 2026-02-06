# GitHub Actions Secrets Setup Guide

**Purpose:** Configure credentials for automated publication workflow

**Required Time:** 10-15 minutes

---

## 🔐 Required Secrets

The release workflow requires 4 secrets to be configured in GitHub Actions:

| Secret | Purpose | Obtainable From |
|--------|---------|-----------------|
| `CARGO_TOKEN` | Publish to crates.io | https://crates.io/me |
| `PYPI_TOKEN` | Publish to PyPI | https://pypi.org/account/tokens/ |
| `DOCKER_USERNAME` | Docker Hub authentication | Your Docker Hub username |
| `DOCKER_TOKEN` | Docker Hub authentication | https://hub.docker.com/settings/security |

**Note:** `GITHUB_TOKEN` is automatically provided by GitHub Actions.

---

## 📝 Step-by-Step Setup

### Step 1: Get crates.io Token

1. Go to https://crates.io/me
2. Click **API Tokens** in the left sidebar
3. Click **New Token**
4. Enter token name: `fraiseql-ci`
5. Click **Create**
6. Copy the generated token (starts with `crates_...`)
7. Save it safely (you won't be able to view it again)

### Step 2: Get PyPI Token

1. Go to https://pypi.org/account/tokens/
2. Click **Add API token**
3. Token name: `fraiseql-ci`
4. Scope: **Entire account** (or project-specific)
5. Click **Create token**
6. Copy the generated token (starts with `pypi-...`)
7. Save it safely

### Step 3: Get Docker Hub Credentials

1. Go to https://hub.docker.com/settings/security
2. Click **New Access Token**
3. Token description: `fraiseql-ci`
4. Permissions: **Read & Write**
5. Click **Create**
6. Copy the generated token
7. Note your Docker Hub username
8. Save both safely

### Step 4: Add Secrets to GitHub

1. Go to your repository on GitHub
2. Click **Settings** (top menu)
3. Click **Secrets and variables** → **Actions** (left sidebar)
4. Click **New repository secret**

Repeat for each secret:

#### Secret 1: CARGO_TOKEN

- **Name:** `CARGO_TOKEN`
- **Value:** (paste token from Step 1)
- Click **Add secret**

#### Secret 2: PYPI_TOKEN

- **Name:** `PYPI_TOKEN`
- **Value:** (paste token from Step 2, including `pypi-` prefix)
- Click **Add secret**

#### Secret 3: DOCKER_USERNAME

- **Name:** `DOCKER_USERNAME`
- **Value:** (your Docker Hub username)
- Click **Add secret**

#### Secret 4: DOCKER_TOKEN

- **Name:** `DOCKER_TOKEN`
- **Value:** (paste token from Step 3)
- Click **Add secret**

---

## ✅ Verification

### Verify Secrets Are Set

```bash
# List secrets (GitHub CLI)
gh secret list

# Output should show:
# CARGO_TOKEN          Updated just now
# DOCKER_TOKEN         Updated just now
# DOCKER_USERNAME      Updated just now
# PYPI_TOKEN           Updated just now
```

### Verify Each Secret

Test each credential before triggering a release:

#### Test crates.io Token

```bash
# Set token
export CARGO_TOKEN="your-token"

# Test authentication
cargo login $CARGO_TOKEN
cargo owner --list fraiseql-core 2>/dev/null && echo "✅ crates.io token valid"
```

#### Test PyPI Token

```bash
# Create test file
echo -e "[distutils]\nindex-servers = testpypi\n\n[testpypi]\nrepository = https://test.pypi.org/legacy/\nusername = __token__\npassword = pypi-your-token" > ~/.pypirc

# Test (would fail if invalid)
twine check 2>/dev/null && echo "✅ PyPI token valid"
```

#### Test Docker Credentials

```bash
# Test login
echo "your-token" | docker login -u your-username --password-stdin 2>/dev/null && echo "✅ Docker credentials valid"
```

---

## 🔄 Token Rotation

Tokens should be rotated periodically for security.

### When to Rotate

- Annually (recommended)
- If compromised
- When team member leaves
- During security audit

### How to Rotate

1. Generate new token on platform
2. Update GitHub secret with new token:
   - Go to Settings → Secrets and variables → Actions
   - Click the secret name
   - Click **Update**
   - Paste new token
   - Click **Update secret**
3. Test the new token
4. Revoke old token on the platform

---

## 🚨 Security Best Practices

### Secrets Handling

- ✅ **DO** rotate tokens annually
- ✅ **DO** use CI/CD specific tokens (not personal)
- ✅ **DO** limit token scope to minimum needed
- ✅ **DO** revoke tokens immediately if compromised
- ✅ **DO** store backup tokens in secure vault

- ❌ **DON'T** commit tokens to git
- ❌ **DON'T** share tokens via email/chat
- ❌ **DON'T** use production tokens in CI (if possible)
- ❌ **DON'T** create tokens with excessive scope

### GitHub Secret Security

- Secrets are encrypted at rest
- Secrets are masked in logs
- Secrets only accessible to Actions
- Secrets visible in:
  - Actions environment variables
  - PR reviews (not in PR diff)
  - Repository secrets list (masked value)

### Token Scope Guidelines

| Service | Recommended Scope |
|---------|-------------------|
| crates.io | Publish only, no deletion |
| PyPI | Upload new versions only |
| Docker Hub | Repository specific |
| GitHub | Standard `GITHUB_TOKEN` |

---

## 🆘 Troubleshooting

### Secret Not Available in Workflow

**Problem:** Workflow fails with "secret not found"

**Solution:**

1. Verify secret name matches exactly (case-sensitive)
2. Verify secret is in repository (not organization)
3. Verify repository has Actions enabled
4. Workflows in forked repos may not have access

**Check:**

```bash
gh secret list
# Should show your secret
```

### Token Authorization Failed

**Problem:** Workflow fails with "401 Unauthorized"

**Solution:**

1. Verify token is correct (copy-paste from platform)
2. Verify token hasn't expired
3. Verify token has correct scope
4. Try creating a new token

### Secret Visible in Logs

**Problem:** Secret appears in workflow logs

**Solution:**

1. GitHub automatically masks secrets in logs
2. If visible, token may be compromised
3. Immediately revoke token on platform
4. Create new token
5. Update GitHub secret
6. Check workflow logs don't contain plain text

---

## 📚 Additional Resources

### Official Documentation

- [GitHub Secrets Documentation](https://docs.github.com/en/actions/security-guides/encrypted-secrets)
- [crates.io API Documentation](https://docs.rs/crates-io/latest/crates_io/)
- [PyPI API Documentation](https://warehouse.pypa.io/api-reference/)
- [Docker Hub API](https://docs.docker.com/docker-hub/api/latest/)

### Related Setup Guides

- [GitHub Actions Workflow Guide](.github/workflows/release.yml)
- [Publication Guide](../PUBLICATION_GUIDE.md)
- [Security Policy](../SECURITY.md)

---

## ✅ Final Checklist

Before triggering a release:

- [ ] CARGO_TOKEN set in GitHub Actions secrets
- [ ] PYPI_TOKEN set in GitHub Actions secrets
- [ ] DOCKER_USERNAME set in GitHub Actions secrets
- [ ] DOCKER_TOKEN set in GitHub Actions secrets
- [ ] Each token tested locally (optional but recommended)
- [ ] Git tag created: `v2.0.0-a1`
- [ ] Tag pushed to GitHub: `git push origin v2.0.0-a1`
- [ ] Workflow triggered automatically
- [ ] Monitor workflow in GitHub Actions tab

---

## 📞 Support

If you encounter issues:

1. Check the official documentation links above
2. Review GitHub Actions logs for error messages
3. Verify token is still valid on the platform
4. Create a new token if necessary
5. Open an issue on GitHub if persistent problems

---

**Setup Date:** January 19, 2026
**Last Updated:** January 19, 2026
**Status:** ✅ Ready for Release Workflow

[← Back to Publication Guide](../PUBLICATION_GUIDE.md)
