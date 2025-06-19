import subprocess
import sys

print("Testing skipped integration tests in isolated environment...")
print("=" * 60)

# Count skipped tests
result = subprocess.run(
    [sys.executable, "-m", "pytest", "--collect-only", "-q", 
     "tests/integration/", "tests/e2e/", "tests/use_cases/etl/"],
    capture_output=True,
    text=True
)

# Parse output
lines = result.stdout.strip().split('\n')
total_collected = 0
total_skipped = 0

for line in lines:
    if "selected" in line or "deselected" in line:
        parts = line.split()
        if "selected" in line:
            total_collected = int(parts[0])
        if "deselected" in line:
            total_skipped = int(parts[0])

print(f"Total integration/E2E tests found: {total_collected}")
print(f"Tests marked as skipped: {total_skipped}")
print("\nThese tests require:")
print("- Full database setup with schemas")
print("- Seeded test data")
print("- GraphQL functions deployed")
print("- ETL fixtures and configurations")
print("\nTo run them, you would need:")
print("1. A PostgreSQL container with full schema")
print("2. Database migrations applied")
print("3. Test data seeded")
print("4. Proper environment configuration")
