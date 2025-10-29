#!/bin/bash
set -e

echo "🔍 Running local documentation validation..."

# Make sure script is executable
chmod +x scripts/validate-docs.sh

echo "📁 Testing file references..."
./scripts/validate-docs.sh files

echo "🏷️  Testing version consistency..."
./scripts/validate-docs.sh versions

echo "📦 Testing installation setup..."
./scripts/validate-docs.sh install

echo "🚀 Testing user onboarding..."
python3 -c "
import sys, os, subprocess, time
start_time = time.time()

# Check README has 'Is this for me' section
if subprocess.run(['grep', '-q', 'Is this for me', 'README.md']).returncode != 0:
    print('❌ Missing Is this for me section in README')
    sys.exit(1)

# Check quickstart exists
if not os.path.isfile('docs/quickstart.md'):
    print('❌ Quickstart documentation missing')
    sys.exit(1)

# Test import
try:
    import fraiseql
    print('✅ FraiseQL import successful')
except ImportError as e:
    print(f'❌ Import failed: {e}')
    sys.exit(1)

duration = int(time.time() - start_time)
print(f'✅ Beginner onboarding test completed in {duration}s')
if duration > 1800:
    print('⚠️  WARNING: Onboarding took longer than 30 minutes target')
"

echo "📚 Testing examples..."
python3 -c "
import os, sys, subprocess
if not os.path.isdir('examples'):
    print('❌ Examples directory missing')
    sys.exit(1)
result = subprocess.run(['find', 'examples', '-name', '*.py'], capture_output=True, text=True)
count = len(result.stdout.strip().split('\n')) if result.stdout.strip() else 0
if count < 1:
    print('❌ No Python examples found')
    sys.exit(1)
print(f'✅ Found {count} example files')
"

echo "📋 Checking required docs..."
python3 -c "
import os, sys
required_docs = [
    'README.md',
    'INSTALLATION.md',
    'CONTRIBUTING.md',
    'AUDIENCES.md',
    'VERSION_STATUS.md',
    'docs/TESTING_CHECKLIST.md'
]
for doc in required_docs:
    if not os.path.isfile(doc):
        print(f'❌ Missing required documentation: {doc}')
        sys.exit(1)
print('✅ All required documentation files exist')
"

echo "🎉 All documentation validations passed!"
