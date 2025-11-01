# Branch Protection Rules for main

This document outlines the branch protection rules for the `main` branch. These settings should be configured in GitHub repository settings.

## Branch Protection Settings

Navigate to: Settings → Branches → Add rule

### Rule Pattern: `main`

#### 1. Protect matching branches
- [x] **Require a pull request before merging**
  - [x] Require approvals: **1**
  - [x] Dismiss stale pull request approvals when new commits are pushed
  - [x] Require review from CODEOWNERS (if applicable)
  - [x] Require approval of the most recent reviewable push

#### 2. Status checks
- [x] **Require status checks to pass before merging**
  - [x] Require branches to be up to date before merging

  **Required status checks:**
  - `test / test (3.11)`
  - `test / test (3.12)`
  - `test / test (3.13)`
  - `test / lint`
  - `test / type-check`

#### 3. Conversation resolution
- [x] **Require conversation resolution before merging**

#### 4. Signed commits (optional)
- [ ] Require signed commits

#### 5. Linear history
- [x] **Require linear history** (enforces rebasing instead of merge commits)

#### 6. Restrictions
- [ ] Restrict who can push to matching branches (only if needed)
- [x] **Restrict who can push to matching branches**
  - Users/teams with push access: Repository administrators only
  - [x] Include administrators

#### 7. Force pushes
- [x] **Allow force pushes**
  - [x] Specify who can force push: Repository administrators only
- [x] **Allow deletions** (admins only)

## Additional Recommendations

1. **Tag Protection** (Settings → Tags → Add rule)
   - Pattern: `v*`
   - Only allow users with write access to create/delete matching tags

2. **Required Reviews for GitHub Actions**
   - Require approval for workflow runs from external contributors

3. **Branch Naming Convention**
   Encourage these patterns:
   - `feature/*` - New features
   - `fix/*` - Bug fixes
   - `docs/*` - Documentation updates
   - `refactor/*` - Code refactoring
   - `test/*` - Test additions/changes
   - `chore/*` - Maintenance tasks
