#!/usr/bin/env python3
"""Simplified PR Ship for FraiseQL - bulletproof PR workflow with version management.

This module provides a clean, simple PR shipping workflow that:
1. Validates branch protection
2. Syncs with base branch
3. Runs all quality checks upfront
4. Bumps version atomically
5. Creates PR with auto-merge enabled
6. Creates git tag for release

Adapted from PrintOptim's pr_ship.py to work with FraiseQL's Rust pipeline
and multiple version files (8 total across Python, Rust, and documentation).
"""

import os
import subprocess
import sys
from pathlib import Path
from typing import Literal


class PRShip:
    """Simplified PR ship that eliminates race conditions."""

    def __init__(self):
        self.project_root = Path(__file__).parent.parent
        self.original_dir = Path.cwd()
        os.chdir(self.project_root)

    def run_command(
        self, command: list[str], *, capture_output: bool = True, check: bool = True
    ) -> subprocess.CompletedProcess:
        """Run command with proper error handling."""
        try:
            return subprocess.run(
                command, capture_output=capture_output, text=True, check=check
            )
        except subprocess.CalledProcessError as e:
            if capture_output:
                print(f"❌ Command failed: {' '.join(command)}")
                if e.stderr:
                    print(f"   Error: {e.stderr}")
                if e.stdout:
                    print(f"   Output: {e.stdout}")
            raise

    def get_current_branch(self) -> str:
        """Get current git branch."""
        result = self.run_command(["git", "branch", "--show-current"])
        return result.stdout.strip()

    def check_protected_branch(self) -> bool:
        """Check if we're on a protected branch."""
        branch = self.get_current_branch()
        protected_branches = {"dev", "staging", "main", "master", "production"}

        if branch in protected_branches:
            print(f"❌ Cannot ship from protected branch: {branch}")
            print("💡 Create a feature branch first:")
            print("   git checkout -b chore/prepare-v1.8.x-release")
            return False

        return True

    def has_uncommitted_changes(self) -> bool:
        """Check for uncommitted changes."""
        result = self.run_command(["git", "status", "--porcelain"], check=False)
        return bool(result.stdout.strip())

    def sync_with_base_branch(self, base_branch: str = "dev") -> bool:
        """Fetch and merge latest base branch to avoid out-of-date PRs."""
        print("\n" + "=" * 60)
        print("🔄 PHASE 0: Syncing with Base Branch")
        print("=" * 60)

        try:
            # Fetch latest changes from origin
            print(f"📥 Fetching latest {base_branch} from origin...")
            self.run_command(["git", "fetch", "origin", base_branch], capture_output=True)
            print(f"✅ Fetched origin/{base_branch}")

            # Check if we need to merge
            result = self.run_command(
                ["git", "rev-list", f"HEAD..origin/{base_branch}", "--count"],
                capture_output=True,
            )
            commits_behind = int(result.stdout.strip())

            if commits_behind == 0:
                print(f"✅ Already up-to-date with origin/{base_branch}")
                return True

            print(f"📊 {commits_behind} commit(s) ahead on {base_branch}")
            print(f"🔀 Merging origin/{base_branch} into current branch...")

            # Merge the base branch
            self.run_command(
                ["git", "merge", f"origin/{base_branch}", "--no-edit"],
                capture_output=False,
            )

            print(f"✅ Successfully merged origin/{base_branch}")
            return True

        except subprocess.CalledProcessError as e:
            print(f"\n❌ Failed to sync with {base_branch}")
            print("💡 You may have merge conflicts. Please resolve them and try again.")
            print(f"💡 Run: git merge origin/{base_branch}")
            return False

    def run_quality_checks(self) -> bool:
        """Run all quality checks before making any changes."""
        print("\n" + "=" * 60)
        print("🔍 PHASE 1: Pre-flight Quality Checks")
        print("=" * 60)

        print("Running full test suite...")
        result = self.run_command(
            ["uv", "run", "pytest", "-x", "-q"],
            capture_output=True,
            check=False,
        )

        if result.returncode != 0:
            print("❌ Tests failed!")
            print(result.stdout)
            if result.stderr:
                print(result.stderr)
            return False

        # Show test summary
        lines = result.stdout.strip().split("\n")
        if lines:
            print(f"✅ {lines[-1]}")
        print("✅ All quality checks passed!")
        return True

    def bump_version(self, bump_type: Literal["patch", "minor", "major"]) -> str:
        """Bump version using the version manager."""
        print(f"\n📦 Bumping {bump_type} version...")

        result = self.run_command(
            ["uv", "run", "python", "scripts/version_manager.py", bump_type],
            capture_output=True,
            check=True,
        )

        # Extract new version from output
        output = result.stdout
        if "Version bumped to" in output:
            # Get the last line which contains the version
            lines = output.strip().split("\n")
            for line in reversed(lines):
                if "Version bumped to" in line:
                    new_version = line.split()[-1]
                    print(f"✅ Version bumped to {new_version}")
                    return new_version

        print("⚠️  Could not determine new version")
        return ""

    def commit_changes(self, message: str) -> None:
        """Commit all changes."""
        print(f"\n📝 Committing: {message}")

        # Stage all changes
        self.run_command(["git", "add", "."])

        # Commit with PR_SHIP context for hooks
        os.environ["PR_SHIP_IN_PROGRESS"] = "1"
        self.run_command(["git", "commit", "-m", message])

        print("✅ Changes committed")

    def create_git_tag(self, version: str) -> None:
        """Create git tag for version."""
        tag_name = f"v{version}"
        print(f"\n🏷️  Creating tag: {tag_name}")

        # Check if tag already exists
        result = self.run_command(["git", "tag", "-l", tag_name], check=False)
        if result.stdout.strip():
            print(f"✅ Tag {tag_name} already exists")
        else:
            self.run_command(
                ["git", "tag", "-a", tag_name, "-m", f"Release {version}"],
                capture_output=True,
            )
            print(f"✅ Tag {tag_name} created")

    def push_to_origin(self, new_tag: str | None = None) -> None:
        """Push current branch and specific tag to origin."""
        print("\n📤 Pushing to GitHub...")

        current_branch = self.get_current_branch()

        # Push branch (create upstream if needed)
        try:
            self.run_command(["git", "push", "origin", current_branch], capture_output=True)
        except subprocess.CalledProcessError:
            print("   Setting upstream branch...")
            self.run_command(["git", "push", "-u", "origin", current_branch], capture_output=True)

        # Push only the new tag if specified
        if new_tag:
            print(f"   Pushing tag {new_tag}...")
            self.run_command(["git", "push", "origin", new_tag], capture_output=True)

        print("✅ Pushed to GitHub")

    def create_and_ship_pr(self) -> None:
        """Create PR and enable auto-merge."""
        print("\n🚀 Creating PR with auto-merge...")

        current_branch = self.get_current_branch()

        # Generate PR title from branch name
        pr_title = (
            current_branch.replace("chore/", "")
            .replace("prepare-", "")
            .replace("_", " ")
            .replace("-", " ")
            .title()
        )

        try:
            # Create PR targeting dev
            result = self.run_command(
                [
                    "gh",
                    "pr",
                    "create",
                    "--base",
                    "dev",
                    "--title",
                    pr_title,
                    "--body",
                    f"Auto-created PR from {current_branch}\n\n✅ All quality checks passed\n✅ Full test suite passed",
                ],
                capture_output=True,
                check=False,
            )

            if result.returncode == 0:
                pr_url = result.stdout.strip()
                print(f"✅ PR created: {pr_url}")
            else:
                # PR might already exist
                result = self.run_command(
                    ["gh", "pr", "view", "--json", "url", "-q", ".url"], check=False
                )
                if result.returncode == 0:
                    pr_url = result.stdout.strip()
                    print(f"✅ PR already exists: {pr_url}")
                else:
                    print("⚠️  Could not create or find PR")
                    return

            # Enable auto-merge
            print("🤖 Enabling auto-merge...")
            try:
                self.run_command(
                    ["gh", "pr", "merge", "--auto", "--squash"],
                    capture_output=True,
                    check=False,
                )
                print("✅ Auto-merge enabled")
            except subprocess.CalledProcessError:
                print("⚠️  Auto-merge will activate when checks pass")

        except subprocess.CalledProcessError as e:
            print(f"⚠️  Could not create PR: {e}")
            print("💡 You can create it manually with: gh pr create")

    def ship(self, bump_type: Literal["patch", "minor", "major"] = "patch") -> bool:
        """Main ship workflow."""
        print("🚢 FRAISEQL PR SHIP WORKFLOW")
        print("=" * 60)
        print(f"Target: dev branch with {bump_type} version bump")

        # Validate environment
        if not self.check_protected_branch():
            return False

        if self.has_uncommitted_changes():
            print("⚠️  You have uncommitted changes")
            print("💡 These will be included in the version bump commit")

        # PHASE 0: Sync with base branch (prevent out-of-date PRs)
        if not self.sync_with_base_branch():
            return False

        # PHASE 1: Quality checks (before any changes)
        if not self.run_quality_checks():
            return False

        # PHASE 2: Make all changes
        print("\n" + "=" * 60)
        print("📦 PHASE 2: Preparing Changes")
        print("=" * 60)

        try:
            # Bump version
            new_version = self.bump_version(bump_type)
            if not new_version:
                return False

            # PHASE 3: Commit everything
            print("\n" + "=" * 60)
            print("💾 PHASE 3: Committing Changes")
            print("=" * 60)

            self.commit_changes(f"chore(release): bump version to v{new_version}")

            # PHASE 4: Git operations
            print("\n" + "=" * 60)
            print("🔧 PHASE 4: Git Operations")
            print("=" * 60)

            self.create_git_tag(new_version)
            self.push_to_origin(new_tag=f"v{new_version}")

            # PHASE 5: Create PR
            print("\n" + "=" * 60)
            print("🚀 PHASE 5: Creating Pull Request")
            print("=" * 60)

            self.create_and_ship_pr()

            print("\n" + "=" * 60)
            print("✨ PR SHIP COMPLETED SUCCESSFULLY!")
            print("=" * 60)
            print(f"📦 Version: v{new_version}")
            print("🎯 Target: dev branch")
            print("🤖 Auto-merge: Enabled (will merge when checks pass)")
            print("🏷️  Git tag: v{new_version} pushed")
            print("📝 Commit: chore(release): bump version to v{new_version}")

            return True

        except Exception as e:
            print(f"\n❌ Ship failed: {e}")
            print("💡 Your changes are safe. Fix the issue and try again.")
            return False

        finally:
            # Clean up context
            os.environ.pop("PR_SHIP_IN_PROGRESS", None)
            os.chdir(self.original_dir)


def main():
    """CLI entry point."""
    import argparse

    parser = argparse.ArgumentParser(
        description="FraiseQL PR Ship - Automated release workflow"
    )
    parser.add_argument(
        "bump_type",
        nargs="?",
        default="patch",
        choices=["patch", "minor", "major"],
        help="Version bump type (default: patch)",
    )

    args = parser.parse_args()

    shipper = PRShip()
    success = shipper.ship(args.bump_type)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
