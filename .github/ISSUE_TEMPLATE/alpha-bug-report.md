---
name: Alpha Bug Report
about: Report a bug in FraiseQL v2.0.0-alpha.1
title: "[ALPHA] "
labels: ["alpha", "bug"]
assignees: ''
---

## Description
Brief description of the bug you encountered.

---

## Environment

- **FraiseQL version**: 2.0.0-alpha.1
- **Language**: Python / TypeScript / Go / PHP
- **Database**: PostgreSQL / MySQL / SQLite / SQL Server (version)
- **OS**: Linux / macOS / Windows
- **Rust version** (if building from source):

---

## Steps to Reproduce

1. Define schema with...
2. Compile with...
3. Run query/mutation...
4. Observe the bug...

**Include code snippets if possible:**

```python
# Your schema
from fraiseql import type as fraiseql_type

@fraiseql_type
class Example:
    id: int
```

---

## Expected Behavior
What should have happened?

---

## Actual Behavior
What actually happened instead?

---

## Error Message
If applicable, paste the full error message:

```
error: ...
```

---

## Additional Context

- Is this blocking your alpha testing? (yes/no)
- Can you work around this? (yes/no, and how?)
- Any related issues you found?

---

## Checklist

- [ ] I've searched for existing issues with similar problems
- [ ] I've included steps to reproduce
- [ ] I've provided environment details
- [ ] I've included error messages where applicable
- [ ] This is specific to alpha (not a feature request)

---

## Label This Issue
Please add the **`alpha`** label. Other useful labels:

- `bug` — for broken functionality
- `performance` — if it's a performance issue
- `security` — if it's a security concern
- `database-[postgres|mysql|sqlite|sqlserver]` — for database-specific issues
