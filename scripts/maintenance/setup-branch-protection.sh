#!/bin/bash
# Script to set up branch protection rules via GitHub CLI
# Requires: gh CLI tool (https://cli.github.com/)

set -e

echo "Setting up branch protection for main branch..."
echo "Note: You need to have admin access to the repository"
echo ""

# Check if gh is installed
if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI (gh) is not installed"
    echo "Install from: https://cli.github.com/"
    exit 1
fi

# Check if authenticated
if ! gh auth status &> /dev/null; then
    echo "Error: Not authenticated with GitHub"
    echo "Run: gh auth login"
    exit 1
fi

# Get repository name
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
echo "Repository: $REPO"
echo ""

# Confirm before proceeding
read -p "Set up branch protection for main branch? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 0
fi

# Create branch protection rule
echo "Creating branch protection rule for main..."

gh api \
  --method PUT \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  /repos/$REPO/branches/main/protection \
  -f "required_status_checks[strict]=true" \
  -f "required_status_checks[contexts][]=test / test (3.11)" \
  -f "required_status_checks[contexts][]=test / test (3.12)" \
  -f "required_status_checks[contexts][]=test / test (3.13)" \
  -f "required_status_checks[contexts][]=test / lint" \
  -f "required_status_checks[contexts][]=test / type-check" \
  -f "enforce_admins=true" \
  -f "required_pull_request_reviews[required_approving_review_count]=1" \
  -f "required_pull_request_reviews[dismiss_stale_reviews]=true" \
  -f "required_pull_request_reviews[require_code_owner_reviews]=false" \
  -f "required_pull_request_reviews[require_last_push_approval]=true" \
  -f "required_linear_history=true" \
  -f "allow_force_pushes=false" \
  -f "allow_deletions=false" \
  -f "required_conversation_resolution=true" \
  -f "lock_branch=false" \
  -f "allow_fork_syncing=true"

echo "Branch protection rule created successfully!"
echo ""
echo "Additional manual steps:"
echo "1. Go to Settings → Branches in your repository"
echo "2. Review the branch protection rules"
echo "3. Adjust any settings as needed"
echo ""
echo "To protect tags, go to Settings → Tags → Add rule"
echo "Pattern: v*"
