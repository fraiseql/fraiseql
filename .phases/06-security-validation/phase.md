# Phase 06: Create Security Validation Script

**Priority:** LOW (bonus task if time permits)
**Time Estimate:** 1 hour
**Impact:** +0.5 point to Testing score (maintain 15/15)
**Status:** ⬜ Not Started

---

## Problem Statement

While FraiseQL has excellent test coverage (990+ tests), a quick security validation script for production deployments would help operators verify security configuration before going live or after configuration changes.

---

## Objective

Create a Python script that validates security configuration in deployed FraiseQL environments:
1. TLS 1.3 enforcement
2. GraphQL introspection disabled (for classified)
3. APQ required mode (for classified)
4. Rate limiting active
5. Error detail level (no information leakage)
6. Security headers present

**Deliverable:** `scripts/validate_security_config.py`

---

## Context Files

**Review these files (orchestrator will copy to `context/`):**
- `docs/security/PROFILES.md` - Security profile definitions
- `.phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md` - IL4/IL5 requirements (if completed)
- `scripts/` directory - Existing script patterns
- Any existing validation or test scripts

---

## Deliverable

**File:** `.phases/06-security-validation/output/validate_security_config.py`

**Target Location:** `scripts/validate_security_config.py`

---

## Script Requirements

### Command-Line Interface

```bash
# Usage examples
python scripts/validate_security_config.py --url https://api.example.com --profile STANDARD
python scripts/validate_security_config.py --url https://api.example.com --profile IL4
python scripts/validate_security_config.py --url https://api.example.com --profile IL5 --insecure
```

**Arguments:**
- `--url` (required): Base URL of FraiseQL deployment
- `--profile` (optional): Security profile to validate against (STANDARD, REGULATED, RESTRICTED, IL4, IL5)
- `--insecure` (optional): Disable SSL certificate verification (for testing with self-signed certs)

**Exit Codes:**
- `0`: All checks passed
- `1`: One or more checks failed with ERROR severity

### Validation Checks (6 Required)

#### 1. TLS Configuration
- Verify TLS 1.3 is enabled
- Check SSL certificate validity (unless --insecure)
- **ERROR for IL4/IL5** if TLS 1.3 not enabled
- **WARNING for STANDARD/REGULATED** if TLS 1.3 not enabled

#### 2. GraphQL Introspection
- Send introspection query: `{ __schema { types { name } } }`
- Verify introspection is disabled
- **ERROR for IL4/IL5/RESTRICTED** if introspection enabled
- **WARNING for STANDARD/REGULATED** if introspection enabled

#### 3. APQ (Automatic Persisted Queries)
- Send non-persisted query: `{ __typename }`
- For IL4/IL5: Verify APQ "required" mode (query should fail)
- **ERROR for IL4/IL5** if arbitrary queries allowed
- **INFO for others**

#### 4. Rate Limiting
- Send 20 rapid requests
- Verify rate limiting triggers (HTTP 429)
- **ERROR for IL4/IL5** if no rate limiting detected
- **WARNING for others** if no rate limiting detected

#### 5. Error Detail Level
- Send invalid query to trigger error
- Check for information leakage (stack traces, file paths, line numbers)
- **ERROR for IL4/IL5** if sensitive info in errors
- **WARNING for others** if sensitive info in errors

#### 6. Security Headers
- Check for security headers:
  - `Strict-Transport-Security` (HSTS)
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options` (DENY or SAMEORIGIN)
  - `Content-Security-Policy`
- **WARNING** if any header missing (not ERROR, as headers might be set by proxy)

---

## Script Structure

### Classes

**ValidationResult:**
```python
class ValidationResult:
    def __init__(self, check: str, passed: bool, message: str, severity: str = "ERROR"):
        self.check = check
        self.passed = passed
        self.message = message
        self.severity = severity  # ERROR, WARNING, INFO
```

**SecurityValidator:**
```python
class SecurityValidator:
    def __init__(self, base_url: str, profile: str, insecure: bool = False):
        self.base_url = base_url
        self.profile = profile
        self.verify_ssl = not insecure
        self.results: List[ValidationResult] = []

    def add_result(self, check: str, passed: bool, message: str, severity: str = "ERROR"):
        # Add validation result

    def validate_tls(self) -> None:
        # Check TLS configuration

    def validate_introspection(self) -> None:
        # Check GraphQL introspection

    def validate_apq(self) -> None:
        # Check APQ configuration

    def validate_rate_limiting(self) -> None:
        # Check rate limiting

    def validate_error_details(self) -> None:
        # Check error detail level

    def validate_security_headers(self) -> None:
        # Check security headers

    def run_all_validations(self) -> bool:
        # Run all checks and return overall pass/fail

    def print_summary(self) -> bool:
        # Print summary and return pass/fail
```

---

## Output Format

### Progress Output

```
============================================================
FraiseQL Security Configuration Validator
Target: https://api.example.com
Profile: IL4
============================================================

[1/6] Validating TLS Configuration...
[2/6] Validating GraphQL Introspection...
[3/6] Validating APQ Configuration...
[4/6] Validating Rate Limiting...
[5/6] Validating Error Detail Level...
[6/6] Validating Security Headers...

============================================================
Validation Results:
============================================================

[✓ PASS] TLS Version: TLS 1.3 enabled
[✗ FAIL (ERROR)] Introspection Disabled: Introspection is enabled (security risk for classified)
[✓ PASS] APQ Required Mode: APQ 'required' mode enabled (only pre-registered queries allowed)
[✗ FAIL (ERROR)] Rate Limiting: Rate limiting not detected (sent 20 requests without throttling)
[✓ PASS] Error Detail Level: Error messages do not leak sensitive information
[✗ FAIL (WARNING)] Header: Content-Security-Policy: CSP header missing

============================================================
Summary: 3/6 checks passed
Errors: 2, Warnings: 1
============================================================

❌ VALIDATION FAILED - Fix errors before deploying to production
```

### Exit Codes

```python
if errors > 0:
    print("❌ VALIDATION FAILED - Fix errors before deploying to production")
    sys.exit(1)
elif warnings > 0:
    print("⚠️  VALIDATION PASSED WITH WARNINGS - Review warnings before deploying")
    sys.exit(0)
else:
    print("✅ VALIDATION PASSED - Security configuration looks good")
    sys.exit(0)
```

---

## Implementation Details

### Dependencies

**Use only standard library + requests:**
```python
import argparse
import json
import sys
from typing import List
import requests
```

**Optional:** Use `urllib3` to disable SSL warnings when `--insecure` flag is used.

### Error Handling

- Catch `requests.exceptions.SSLError` for TLS issues
- Catch `requests.exceptions.ConnectionError` for network issues
- Catch `requests.exceptions.Timeout` for slow responses
- Don't let exceptions crash the script - report as validation failures

### Timeout Configuration

- Use 5-second timeout for most requests
- Use 2-second timeout for rate limiting test (need to be fast)

---

## Code Template

```python
#!/usr/bin/env python3
"""
FraiseQL Security Configuration Validator

Validates security configuration for FraiseQL deployments.
Use before going live to ensure security settings are correct.

Usage:
    python scripts/validate_security_config.py --url https://api.example.com --profile IL4
"""

import argparse
import json
import sys
from typing import List
import requests

# Disable SSL warnings for testing with --insecure
from requests.packages.urllib3.exceptions import InsecureRequestWarning
requests.packages.urllib3.disable_warnings(InsecureRequestWarning)


class ValidationResult:
    """Result of a single validation check."""

    def __init__(self, check: str, passed: bool, message: str, severity: str = "ERROR"):
        self.check = check
        self.passed = passed
        self.message = message
        self.severity = severity

    def __repr__(self):
        status = "✓ PASS" if self.passed else f"✗ FAIL ({self.severity})"
        return f"[{status}] {self.check}: {self.message}"


class SecurityValidator:
    """Validates FraiseQL security configuration."""

    def __init__(self, base_url: str, profile: str, insecure: bool = False):
        self.base_url = base_url.rstrip("/")
        self.profile = profile.upper()
        self.verify_ssl = not insecure
        self.results: List[ValidationResult] = []

    def add_result(self, check: str, passed: bool, message: str, severity: str = "ERROR"):
        """Add a validation result."""
        self.results.append(ValidationResult(check, passed, message, severity))

    def validate_tls(self) -> None:
        """Validate TLS configuration."""
        print("\n[1/6] Validating TLS Configuration...")

        try:
            response = requests.get(
                f"{self.base_url}/health",
                verify=self.verify_ssl,
                timeout=5
            )

            # Check TLS version (if available)
            if hasattr(response, "raw") and hasattr(response.raw, "connection"):
                conn = response.raw.connection
                if hasattr(conn, "sock") and hasattr(conn.sock, "version"):
                    ssl_version = conn.sock.version()
                    if ssl_version == "TLSv1.3":
                        self.add_result("TLS Version", True, "TLS 1.3 enabled")
                    else:
                        severity = "ERROR" if self.profile in ["IL4", "IL5", "RESTRICTED"] else "WARNING"
                        self.add_result("TLS Version", False, f"TLS 1.3 not enabled (found {ssl_version})", severity)
                else:
                    self.add_result("TLS Version", False, "Could not determine TLS version", "INFO")

        except requests.exceptions.SSLError as e:
            self.add_result("TLS Connection", False, f"SSL Error: {str(e)[:100]}", "ERROR")
        except requests.exceptions.ConnectionError as e:
            self.add_result("TLS Connection", False, f"Connection Error: {str(e)[:100]}", "ERROR")
        except Exception as e:
            self.add_result("TLS Connection", False, f"Unexpected error: {str(e)[:100]}", "WARNING")

    def validate_introspection(self) -> None:
        """Validate GraphQL introspection is disabled."""
        print("[2/6] Validating GraphQL Introspection...")

        # ... implementation ...

    def validate_apq(self) -> None:
        """Validate APQ configuration."""
        print("[3/6] Validating APQ Configuration...")

        # ... implementation ...

    def validate_rate_limiting(self) -> None:
        """Validate rate limiting is configured."""
        print("[4/6] Validating Rate Limiting...")

        # ... implementation ...

    def validate_error_details(self) -> None:
        """Validate error detail level."""
        print("[5/6] Validating Error Detail Level...")

        # ... implementation ...

    def validate_security_headers(self) -> None:
        """Validate security headers are present."""
        print("[6/6] Validating Security Headers...")

        # ... implementation ...

    def run_all_validations(self) -> bool:
        """Run all validation checks."""
        print(f"\n{'='*60}")
        print(f"FraiseQL Security Configuration Validator")
        print(f"Target: {self.base_url}")
        print(f"Profile: {self.profile}")
        print(f"{'='*60}")

        self.validate_tls()
        self.validate_introspection()
        self.validate_apq()
        self.validate_rate_limiting()
        self.validate_error_details()
        self.validate_security_headers()

        return self.print_summary()

    def print_summary(self) -> bool:
        """Print validation summary and return overall pass/fail."""
        print(f"\n{'='*60}")
        print("Validation Results:")
        print(f"{'='*60}\n")

        errors = 0
        warnings = 0
        passed = 0

        for result in self.results:
            print(result)
            if not result.passed:
                if result.severity == "ERROR":
                    errors += 1
                else:
                    warnings += 1
            else:
                passed += 1

        total = len(self.results)
        print(f"\n{'='*60}")
        print(f"Summary: {passed}/{total} checks passed")
        print(f"Errors: {errors}, Warnings: {warnings}")
        print(f"{'='*60}\n")

        if errors > 0:
            print("❌ VALIDATION FAILED - Fix errors before deploying to production")
            return False
        elif warnings > 0:
            print("⚠️  VALIDATION PASSED WITH WARNINGS - Review warnings before deploying")
            return True
        else:
            print("✅ VALIDATION PASSED - Security configuration looks good")
            return True


def main():
    parser = argparse.ArgumentParser(
        description="Validate FraiseQL security configuration",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Validate production deployment
  python validate_security_config.py --url https://api.example.com --profile IL4

  # Validate local deployment with self-signed cert
  python validate_security_config.py --url https://localhost:8000 --profile STANDARD --insecure
        """
    )
    parser.add_argument(
        "--url",
        required=True,
        help="Base URL of FraiseQL deployment (e.g., https://api.example.com)"
    )
    parser.add_argument(
        "--profile",
        choices=["STANDARD", "REGULATED", "RESTRICTED", "IL4", "IL5"],
        default="STANDARD",
        help="Security profile to validate against (default: STANDARD)"
    )
    parser.add_argument(
        "--insecure",
        action="store_true",
        help="Disable SSL certificate verification (for testing only)"
    )

    args = parser.parse_args()

    validator = SecurityValidator(args.url, args.profile, args.insecure)
    success = validator.run_all_validations()

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
```

---

## Documentation Addition

**Add to Operations Runbook (`.phases/01-operations-runbook/output/OPERATIONS_RUNBOOK.md`):**

```markdown
## Security Configuration Validation

Before deploying to production or after configuration changes, validate security settings:

**Usage:**
```bash
# Standard deployment
python scripts/validate_security_config.py --url https://api.example.com --profile STANDARD

# Regulated deployment (HIPAA, GDPR)
python scripts/validate_security_config.py --url https://api.example.com --profile REGULATED

# Classified deployment (IL4)
python scripts/validate_security_config.py --url https://api.example.com --profile IL4

# Classified deployment (IL5)
python scripts/validate_security_config.py --url https://api.example.com --profile IL5

# Testing with self-signed certs (dev only)
python scripts/validate_security_config.py --url https://localhost:8000 --profile STANDARD --insecure
```

**Exit Codes:**
- `0` - All checks passed (or passed with warnings only)
- `1` - One or more checks failed with ERROR severity

**Checks Performed:**
1. TLS 1.3 enabled
2. GraphQL introspection disabled (for IL4/IL5/RESTRICTED)
3. APQ "required" mode (for IL4/IL5)
4. Rate limiting active
5. Error detail level (no information leakage)
6. Security headers present (HSTS, X-Content-Type-Options, X-Frame-Options, CSP)

**When to Run:**
- Before initial production deployment
- After security configuration changes
- After upgrading FraiseQL versions
- As part of deployment pipeline (CI/CD)
- During security audits
```

---

## Verification (Orchestrator)

```bash
# 1. Check file exists and is executable
test -f .phases/06-security-validation/output/validate_security_config.py && echo "✓ File exists"

# 2. Make executable
chmod +x .phases/06-security-validation/output/validate_security_config.py

# 3. Verify shebang
head -1 .phases/06-security-validation/output/validate_security_config.py
# Should be: #!/usr/bin/env python3

# 4. Check Python syntax
python3 -m py_compile .phases/06-security-validation/output/validate_security_config.py

# 5. Test help output
python3 .phases/06-security-validation/output/validate_security_config.py --help

# 6. Test with invalid URL (should fail gracefully)
python3 .phases/06-security-validation/output/validate_security_config.py --url https://invalid-url-12345.example.com --profile STANDARD
echo "Exit code: $?"
# Should exit with 1 and show connection error

# 7. Verify all validation methods exist
grep -E "def validate_(tls|introspection|apq|rate_limiting|error_details|security_headers)" .phases/06-security-validation/output/validate_security_config.py | wc -l
# Should be 6
```

---

## Final Placement (Orchestrator)

```bash
# Create scripts directory if needed
mkdir -p scripts

# Move script to final location
cp .phases/06-security-validation/output/validate_security_config.py scripts/validate_security_config.py

# Make executable
chmod +x scripts/validate_security_config.py

# Update operations runbook (if Phase 01 is complete)
if [ -f OPERATIONS_RUNBOOK.md ]; then
  # Add security validation section
  cat >> OPERATIONS_RUNBOOK.md <<'EOF'

---

## Security Configuration Validation

[... add documentation from above ...]
EOF
fi

# Commit
git add scripts/validate_security_config.py OPERATIONS_RUNBOOK.md
git commit -m "feat(security): add security configuration validation script

Add automated security validation tool for deployments:
- Validates 6 security checks (TLS, introspection, APQ, rate limiting, error details, headers)
- Supports multiple security profiles (STANDARD, REGULATED, RESTRICTED, IL4, IL5)
- Returns appropriate exit codes for CI/CD integration
- Includes clear error and warning messages
- Documented in operations runbook

Usage:
  python scripts/validate_security_config.py --url <url> --profile <profile>

Checks:
  1. TLS 1.3 enforcement
  2. GraphQL introspection disabled (classified)
  3. APQ required mode (classified)
  4. Rate limiting active
  5. Error detail level (no info leakage)
  6. Security headers present

Impact: Maintains Testing & QA score (15/15), improves operational readiness

Refs: Pentagon-Readiness Assessment - Phase 06"
```

---

## Tips for Documentation Writer

1. **Keep it simple:** Use only standard library + requests (no exotic dependencies)
2. **Error handling:** Catch all exceptions - script should never crash
3. **Clear output:** Use emojis (✓✗⚠️) and formatting for readability
4. **Profile-aware:** Different profiles have different requirements (IL4 stricter than STANDARD)
5. **Timeouts:** Use short timeouts (2-5s) so script runs quickly
6. **Exit codes:** 0 for success (with warnings OK), 1 for failure (errors)
7. **Help text:** Make --help output clear and include examples

---

## Success Criteria

- [ ] File created: `validate_security_config.py`
- [ ] Script implements 6 validation checks
- [ ] CLI argument parsing works (--url, --profile, --insecure)
- [ ] Script returns correct exit codes (0 for pass, 1 for fail)
- [ ] Output is clear and formatted nicely
- [ ] Error handling is robust (no crashes)
- [ ] Shebang line present: `#!/usr/bin/env python3`
- [ ] Script is executable: `chmod +x`
- [ ] Documentation added to operations runbook (if Phase 01 complete)
- [ ] Python syntax is valid (no errors)
