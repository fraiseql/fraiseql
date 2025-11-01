# FraiseQL Python Version Requirement Analysis

**Date:** October 30, 2025
**Current pyproject.toml:** `requires-python = ">=3.13"`
**Current README.md:** States `Python 3.10+`

## Executive Summary

**Finding:** The codebase uses **Python 3.11+ features** but **NOT Python 3.13-specific features**.

**Recommendation:** Change requirement to `>=3.11` (not 3.10, not 3.13)

---

## Detailed Analysis

### Python 3.11+ Features Found in Codebase

#### 1. **`typing.Self` (Python 3.11+)**
**Introduced:** Python 3.11 (PEP 673)
**Alternative for 3.10:** `typing_extensions.Self`

**Usage in codebase:**
```python
# src/fraiseql/types/definitions.py:5
from typing import TYPE_CHECKING, Any, Optional, Self

# Line 128
def __new__(cls) -> Self:
    """Ensure only one instance of UnsetType exists."""
```

```python
# src/fraiseql/core/registry.py:6
from typing import TYPE_CHECKING, Any, Self, cast

# Line 18
def __new__(cls) -> Self:
    """Create singleton instance."""
```

**Impact:** This is the PRIMARY blocker for Python 3.10 support.

---

#### 2. **`typing.ParamSpec` (Python 3.10+)**
**Introduced:** Python 3.10 (PEP 612)
**Alternative for 3.9:** `typing_extensions.ParamSpec`

**Usage in codebase:**
```python
# src/fraiseql/auth/decorators.py:5
from typing import Any, ParamSpec, TypeVar

P = ParamSpec("P")
T = TypeVar("T")

def requires_auth(
    func: Callable[P, Coroutine[Any, Any, T]],
) -> Callable[P, Coroutine[Any, Any, T]]:
```

**Impact:** This is compatible with Python 3.10+.

---

### Python 3.10+ Features (Compatible)

These features are used extensively and work from Python 3.10+:

1. **PEP 604 Union Syntax (`X | Y`)** - Python 3.10+
   ```python
   str | None  # Instead of Optional[str]
   int | str   # Instead of Union[int, str]
   ```

2. **PEP 585 Generics (`list[T]`, `dict[K, V]`)** - Python 3.10+ (with `from __future__ import annotations` for 3.9)
   ```python
   list[User]      # Instead of List[User]
   dict[str, Any]  # Instead of Dict[str, Any]
   ```

---

### Python 3.13 Features (NOT USED)

These features are NOT found in the codebase:

1. **PEP 695 Type Parameter Syntax** - Python 3.12+
   - New syntax for type parameters
   - Example: `def func[TypeVar](param)` notation
   - NOT FOUND in codebase

2. **PEP 702 `@deprecated` decorator** - Python 3.13+
   ```python
   # NOT FOUND in codebase
   from warnings import deprecated
   @deprecated("Use new_function instead")
   ```

3. **PEP 722 `ExceptionGroup` enhancements** - Python 3.11+, improved 3.13
   ```python
   # NOT FOUND in codebase
   ```

4. **`@override` decorator** - Python 3.12+
   ```python
   # NOT FOUND in codebase (only found as comments/strings)
   from typing import override
   ```

---

## Version Compatibility Matrix

| Feature | 3.10 | 3.11 | 3.12 | 3.13 | In FraiseQL? |
|---------|------|------|------|------|--------------|
| `list[T]`, `X \| Y` syntax | ✅ | ✅ | ✅ | ✅ | ✅ YES |
| `ParamSpec` | ✅ | ✅ | ✅ | ✅ | ✅ YES |
| `typing.Self` | ❌ | ✅ | ✅ | ✅ | ✅ YES |
| `TaskGroup`, `ExceptionGroup` | ❌ | ✅ | ✅ | ✅ | ❌ NO |
| `type` statement (PEP 695) | ❌ | ❌ | ✅ | ✅ | ❌ NO |
| `@override` decorator | ❌ | ❌ | ✅ | ✅ | ❌ NO |
| `@deprecated` decorator | ❌ | ❌ | ❌ | ✅ | ❌ NO |

**Minimum Required Version: Python 3.11** (due to `typing.Self`)

---

## Files Using Python 3.11+ Features

### Critical Files (blocking 3.10 support):
1. **`src/fraiseql/types/definitions.py`** - Uses `typing.Self` (line 5, 128)
2. **`src/fraiseql/core/registry.py`** - Uses `typing.Self` (line 6, 18, 24)

### Compatible Files (3.10+):
3. **`src/fraiseql/auth/decorators.py`** - Uses `ParamSpec` (3.10+ compatible)
4. **All other files** - Use `list[T]`, `X | Y` syntax (3.10+ compatible)

---

## Options for Python 3.10 Support

If Python 3.10 support is desired, there are two options:

### Option 1: Use `typing_extensions` (Recommended if 3.10 support needed)

**Change required files:**

```python
# src/fraiseql/types/definitions.py
# BEFORE:
from typing import TYPE_CHECKING, Any, Optional, Self

# AFTER:
from typing import TYPE_CHECKING, Any, Optional

try:
    from typing import Self  # Python 3.11+
except ImportError:
    from typing_extensions import Self  # Fallback for 3.10
```

**Add dependency:**
```toml
# pyproject.toml
dependencies = [
    # ... existing deps ...
    "typing-extensions>=4.5.0; python_version<'3.11'",
]
```

**Pros:**
- Enables Python 3.10 support
- Minimal code changes (2 files)
- Widely used pattern in Python ecosystem

**Cons:**
- Adds a conditional dependency
- Slightly more complex imports
- 3.10 is already 4+ years old (released Oct 2021)

---

### Option 2: Require Python 3.11+ (RECOMMENDED)

**Keep using native `typing.Self`**, update requirements to match:

```toml
# pyproject.toml
requires-python = ">=3.11"
```

**Pros:**
- Simple, no conditional imports needed
- Python 3.11 is well-established (released Oct 2022)
- Major distros now ship 3.11+ (Ubuntu 23.04+, Debian 12+, Fedora 37+)
- Performance improvements in 3.11+ (10-60% faster than 3.10)
- Better error messages in 3.11+

**Cons:**
- Excludes Ubuntu 22.04 LTS (ships Python 3.10, but users can install 3.11+)
- Slightly smaller user base than 3.10+

---

## Recommendation

**Use Python 3.11+ as minimum requirement.**

### Rationale:

1. **Code already uses `typing.Self`** - Would require refactoring to support 3.10
2. **Python 3.11 is mature** - Released Oct 2022, now 2+ years old
3. **Performance benefits** - 3.11 is significantly faster (10-60% improvement)
4. **Error messages** - 3.11+ has much better error messages for debugging
5. **Simplicity** - No need for `typing_extensions` compatibility layer
6. **Industry standard** - Most modern Python projects now target 3.11+

### Python 3.11 Adoption Status (October 2025):
- **Ubuntu 23.04+**: Python 3.11 (22.04 LTS has 3.10 but can install 3.11)
- **Debian 12+**: Python 3.11
- **Fedora 37+**: Python 3.11
- **RHEL 9.2+**: Python 3.11
- **macOS Homebrew**: Python 3.13 (default), 3.11+ available
- **Windows**: Python 3.13 available, 3.11+ widely installed
- **Docker**: All official Python images include 3.11+
- **Cloud platforms**: Python 3.11+ supported everywhere

---

## Required Changes

### 1. Update `pyproject.toml`

```toml
# Line 14 - CHANGE FROM:
requires-python = ">=3.13"

# TO:
requires-python = ">=3.11"

# Lines 16-23 - UPDATE classifiers:
classifiers = [
    "Development Status :: 5 - Production/Stable",
    "Intended Audience :: Developers",
    "Topic :: Software Development :: Libraries :: Python Modules",
    "License :: OSI Approved :: MIT License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.11",  # ADD
    "Programming Language :: Python :: 3.12",  # ADD
    "Programming Language :: Python :: 3.13",  # KEEP
    "Framework :: FastAPI",
    # ... rest unchanged
]

# Line 185 - UPDATE Black target:
[tool.black]
line-length = 100
target-version = ["py311", "py312", "py313"]  # CHANGE FROM: ["py313"]

# Line 189 - UPDATE Ruff target:
[tool.ruff]
src = ["src"]
target-version = "py311"  # CHANGE FROM: "py313"
```

### 2. Update `README.md`

```markdown
# Line 6 - UPDATE badge:
[![Python](https://img.shields.io/badge/Python-3.11+-blue.svg)](https://www.python.org/downloads/)

# Line 910 - UPDATE Prerequisites section:
### Prerequisites

- **Python 3.11+** (for `typing.Self` and modern type syntax)
- **PostgreSQL 13+**
```

### 3. Update Website (`fraiseql.dev/getting-started.html`)

```html
<!-- Prerequisites section - UPDATE: -->
<div class="bg-white rounded-lg shadow p-6 text-center">
    <h3 class="text-xl font-bold mb-2 text-gray-800">Python</h3>
    <p class="text-2xl font-bold text-red-600 mb-2">3.11+</p>
    <p class="text-gray-600 text-sm">Modern type system with Self type</p>
</div>
```

### 4. Update Website Assessment Document

```markdown
# fraiseql.dev/WEBSITE_UPDATE_ASSESSMENT.md

# Section 2 - UPDATE Python version reference from:
<p class="text-2xl font-bold text-red-600 mb-2">3.10+</p>

# TO:
<p class="text-2xl font-bold text-red-600 mb-2">3.11+</p>
```

---

## Testing Recommendations

### Verify Python 3.11 Compatibility:

1. **Test with Python 3.11 specifically:**
   ```bash
   uv run --python 3.11 pytest
   ```

2. **Add Python 3.11 to CI/CD matrix:**
   ```yaml
   # .github/workflows/test.yml
   strategy:
     matrix:
       python-version: ['3.11', '3.12', '3.13']
   ```

3. **Test installation on Python 3.11:**
   ```bash
   python3.11 -m venv venv-test
   source venv-test/bin/activate
   pip install fraiseql==1.1.0
   python -c "import fraiseql; print(fraiseql.__version__)"
   ```

---

## Impact Analysis

### Breaking Change?
**NO** - This is not a breaking change for existing users because:
1. Most users are likely on Python 3.11+ already (released 2+ years ago)
2. Users on 3.13 (current pyproject.toml requirement) are unaffected
3. We're **loosening** the requirement (3.13 → 3.11), not tightening it

### Who Benefits?
- Users on Python 3.11 or 3.12 (currently excluded by 3.13 requirement)
- CI/CD systems still on 3.11 or 3.12
- Ubuntu 23.04-23.10 users (ship with Python 3.11)
- Debian 12 users (ships with Python 3.11)

### Who Might Be Affected?
- Users on Python 3.10 (would need to upgrade to 3.11)
- Ubuntu 22.04 LTS users (ships with 3.10, but can install 3.11 via deadsnakes PPA)

---

## Alternative: Python 3.10 Support Path

If Python 3.10 support is critical for business reasons:

### Changes Required:

1. **Add `typing-extensions` dependency:**
   ```toml
   dependencies = [
       # ... existing ...
       "typing-extensions>=4.5.0; python_version<'3.11'",
   ]
   ```

2. **Update imports in 2 files:**
   ```python
   # src/fraiseql/types/definitions.py
   # src/fraiseql/core/registry.py

   try:
       from typing import Self
   except ImportError:
       from typing_extensions import Self
   ```

3. **Test thoroughly on Python 3.10:**
   ```bash
   python3.10 -m pytest
   ```

### Trade-offs:
- **Benefit:** Wider compatibility (Ubuntu 22.04 LTS)
- **Cost:** Added complexity, conditional imports, extra dependency
- **Risk:** Python 3.10 reaches end-of-life in October 2026 (1 year away)

---

## Conclusion

**RECOMMENDED ACTION:**
1. ✅ Update `requires-python = ">=3.11"` in `pyproject.toml`
2. ✅ Update README.md to state Python 3.11+
3. ✅ Update website to show Python 3.11+
4. ✅ Update Black/Ruff to target py311
5. ✅ Add Python 3.11, 3.12 to classifiers
6. ✅ Test on Python 3.11 specifically

**Current Status:**
- ❌ `pyproject.toml` incorrectly states 3.13 (too restrictive)
- ❌ `README.md` incorrectly states 3.10 (not compatible with codebase)
- ❌ Website inconsistent (shows 3.13+)

**After Changes:**
- ✅ All documentation consistent (3.11+)
- ✅ Matches actual code requirements (`typing.Self`)
- ✅ Wider compatibility than current 3.13 requirement
- ✅ Simple, no conditional imports needed

---

**Next Steps:**
1. Review and approve this analysis
2. Implement the 4 required changes above
3. Test on Python 3.11, 3.12, 3.13
4. Update CHANGELOG.md noting corrected Python version requirement
5. Release updated documentation

---

**Document Version:** 1.0
**Author:** Claude Code Analysis
**Last Updated:** October 30, 2025
