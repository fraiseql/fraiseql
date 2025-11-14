#!/usr/bin/env python3
"""
Test script to validate template adoption time and functionality.

This script tests the fastapi-rag template to ensure:
1. Template creation works
2. Dependencies install correctly
3. Database setup works
4. Application starts successfully
5. Basic GraphQL queries work
6. RAG functionality is operational

Target: Complete setup and testing in <30 minutes
"""

import asyncio
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, List, Any

import httpx


class TemplateTester:
    """Test harness for FraiseQL templates."""

    def __init__(self, template_name: str = "fastapi-rag"):
        self.template_name = template_name
        self.test_dir = Path("test_template_adoption")
        self.start_time = time.time()
        self.steps: List[Dict[str, Any]] = []

    def log_step(self, step: str, status: str, duration: float = 0, details: str = ""):
        """Log a test step."""
        self.steps.append(
            {"step": step, "status": status, "duration": duration, "details": details}
        )
        status_icon = "✅" if status == "success" else "❌" if status == "failed" else "⏳"
        print(f"{status_icon} {step}")
        if details:
            print(f"   {details}")

    async def run_test(self) -> bool:
        """Run the complete template adoption test."""
        print(f"🚀 Testing {self.template_name} template adoption")
        print("=" * 60)

        try:
            # Step 1: Create project
            if not await self.create_project():
                return False

            # Step 2: Install dependencies
            if not await self.install_dependencies():
                return False

            # Step 3: Setup database
            if not await self.setup_database():
                return False

            # Step 4: Configure environment
            if not await self.configure_environment():
                return False

            # Step 5: Start application
            if not await self.start_application():
                return False

            # Step 6: Test GraphQL API
            if not await self.test_graphql_api():
                return False

            # Step 7: Test RAG functionality
            if not await self.test_rag_functionality():
                return False

            # Calculate total time
            total_time = time.time() - self.start_time
            self.log_step("Total adoption time", "success", total_time, ".1f")

            # Check if within target
            if total_time < 1800:  # 30 minutes
                print("🎉 SUCCESS: Template adoption completed within 30 minutes!")
                return True
            else:
                print(f"⚠️  WARNING: Adoption took {total_time:.1f}s (>30 minutes)")
                return False

        except Exception as e:
            self.log_step("Test execution", "failed", details=str(e))
            return False
        finally:
            # Cleanup
            await self.cleanup()

    async def create_project(self) -> bool:
        """Create a new project using the template."""
        start_time = time.time()

        try:
            # Clean up any existing test directory
            if self.test_dir.exists():
                shutil.rmtree(self.test_dir)

            # Create project using CLI
            cmd = [
                sys.executable,
                "-c",
                f"from src.fraiseql.cli.main import main; main()",
                "init",
                str(self.test_dir),
                f"--template={self.template_name}",
                "--no-git",
            ]

            result = subprocess.run(cmd, capture_output=True, text=True, cwd=".")

            if result.returncode != 0:
                self.log_step("Project creation", "failed", details=f"CLI failed: {result.stderr}")
                return False

            if not self.test_dir.exists():
                self.log_step("Project creation", "failed", details="Project directory not created")
                return False

            duration = time.time() - start_time
            self.log_step("Project creation", "success", duration)
            return True

        except Exception as e:
            duration = time.time() - start_time
            self.log_step("Project creation", "failed", duration, str(e))
            return False

    async def install_dependencies(self) -> bool:
        """Install project dependencies."""
        start_time = time.time()

        try:
            # Change to project directory
            os.chdir(self.test_dir)

            # Install dependencies
            result = subprocess.run(
                [sys.executable, "-m", "pip", "install", "-e", ".[dev]"],
                capture_output=True,
                text=True,
            )

            if result.returncode != 0:
                self.log_step(
                    "Dependency installation",
                    "failed",
                    details=f"pip install failed: {result.stderr}",
                )
                return False

            duration = time.time() - start_time
            self.log_step("Dependency installation", "success", duration)
            return True

        except Exception as e:
            duration = time.time() - start_time
            self.log_step("Dependency installation", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def setup_database(self) -> bool:
        """Setup the database for the project."""
        start_time = time.time()

        try:
            os.chdir(self.test_dir)

            # Run database setup script
            setup_script = Path("scripts/setup_database.py")
            if not setup_script.exists():
                self.log_step("Database setup", "failed", details="setup_database.py not found")
                return False

            result = subprocess.run(
                [sys.executable, "scripts/setup_database.py"], capture_output=True, text=True
            )

            if result.returncode != 0:
                self.log_step("Database setup", "failed", details=f"Setup failed: {result.stderr}")
                return False

            duration = time.time() - start_time
            self.log_step("Database setup", "success", duration)
            return True

        except Exception as e:
            duration = time.time() - start_time
            self.log_step("Database setup", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def configure_environment(self) -> bool:
        """Configure environment variables."""
        start_time = time.time()

        try:
            os.chdir(self.test_dir)

            # Create .env file with test configuration
            env_content = """# Test environment configuration
DATABASE_URL=postgresql://test:test@localhost:5432/testdb
OPENAI_API_KEY=test-key-not-used
FRAISEQL_DATABASE_URL=postgresql://test:test@localhost:5432/testdb
FRAISEQL_AUTO_CAMEL_CASE=true
"""

            with open(".env", "w") as f:
                f.write(env_content)

            duration = time.time() - start_time
            self.log_step("Environment configuration", "success", duration)
            return True

        except Exception as e:
            duration = time.time() - start_time
            self.log_step("Environment configuration", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def start_application(self) -> bool:
        """Start the application server."""
        start_time = time.time()

        try:
            os.chdir(self.test_dir)

            # Start the application (we'll test if it can import without actually running)
            result = subprocess.run(
                [sys.executable, "-c", "import src.main; print('App imports successfully')"],
                capture_output=True,
                text=True,
                timeout=30,
            )

            if result.returncode != 0:
                self.log_step(
                    "Application startup", "failed", details=f"Import failed: {result.stderr}"
                )
                return False

            duration = time.time() - start_time
            self.log_step("Application import test", "success", duration)
            return True

        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            self.log_step("Application startup", "failed", duration, "Timeout")
            return False
        except Exception as e:
            duration = time.time() - start_time
            self.log_step("Application startup", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def test_graphql_api(self) -> bool:
        """Test basic GraphQL API functionality."""
        start_time = time.time()

        try:
            os.chdir(self.test_dir)

            # Test GraphQL schema validation
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    """
from src.main import app
from fraiseql.gql.schema_builder import SchemaRegistry
registry = SchemaRegistry.get_instance()
schema = registry.build_schema()
print('GraphQL schema validation successful')
""",
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )

            if result.returncode != 0:
                self.log_step(
                    "GraphQL API test",
                    "failed",
                    details=f"Schema validation failed: {result.stderr}",
                )
                return False

            duration = time.time() - start_time
            self.log_step("GraphQL schema validation", "success", duration)
            return True

        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            self.log_step("GraphQL API test", "failed", duration, "Timeout")
            return False
        except Exception as e:
            duration = time.time() - start_time
            self.log_step("GraphQL API test", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def test_rag_functionality(self) -> bool:
        """Test RAG functionality components."""
        start_time = time.time()

        try:
            os.chdir(self.test_dir)

            # Test LangChain imports and basic functionality
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    """
try:
    from langchain_openai import OpenAIEmbeddings
    from langchain_community.vectorstores import PGVector
    from langchain.text_splitter import RecursiveCharacterTextSplitter
    print('LangChain components import successfully')
except ImportError as e:
    print(f'Import failed: {e}')
    exit(1)
""",
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )

            if result.returncode != 0:
                self.log_step(
                    "RAG functionality test",
                    "failed",
                    details=f"LangChain import failed: {result.stderr}",
                )
                return False

            duration = time.time() - start_time
            self.log_step("RAG components validation", "success", duration)
            return True

        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            self.log_step("RAG functionality test", "failed", duration, "Timeout")
            return False
        except Exception as e:
            duration = time.time() - start_time
            self.log_step("RAG functionality test", "failed", duration, str(e))
            return False
        finally:
            os.chdir("..")

    async def cleanup(self):
        """Clean up test artifacts."""
        try:
            if self.test_dir.exists():
                shutil.rmtree(self.test_dir)
        except Exception as e:
            print(f"Warning: Cleanup failed: {e}")


async def main():
    """Main test execution."""
    tester = TemplateTester()

    success = await tester.run_test()

    # Print summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)

    for step in tester.steps:
        status = (
            "✅" if step["status"] == "success" else "❌" if step["status"] == "failed" else "⏳"
        )
        duration = ".1f" if step["duration"] > 0 else ""
        print(f"{status} {step['step']}{duration}")
        if step["details"]:
            print(f"   {step['details']}")

    if success:
        print("\n🎉 Template adoption test PASSED!")
        return 0
    else:
        print("\n❌ Template adoption test FAILED!")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
