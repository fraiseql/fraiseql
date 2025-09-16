# FraiseQL v0.7.22 Release Notes

## 🎉 Session Variables for All Execution Modes

**Release Date**: 2025-01-17

### 🚀 Major Feature

#### Universal Session Variable Support
FraiseQL now sets PostgreSQL session variables (`app.tenant_id`, `app.contact_id`) consistently across **all** execution modes, enabling reliable multi-tenant database access patterns with Row-Level Security (RLS).

**Before v0.7.22:**
- ✅ TurboRouter mode: Session variables set automatically
- ❌ Normal mode: No session variables
- ❌ Passthrough mode: No session variables

**After v0.7.22:**
- ✅ TurboRouter mode: Session variables set automatically
- ✅ Normal mode: Session variables set automatically
- ✅ Passthrough mode: Session variables set automatically

### 💡 Problem Solved

Previously, when queries fell back from TurboRouter to normal or passthrough execution modes, session variables were not set. This caused queries relying on PostgreSQL RLS or tenant isolation to fail unexpectedly.

This was particularly problematic for multi-tenant SaaS applications where database-level security depends on these session variables being consistently available.

### 🔧 Technical Details

- **New Method**: Added `_set_session_variables` helper to `FraiseQLRepository`
- **Integration Points**: Session variables now set in all database execution paths
- **Database Support**: Works with both psycopg (cursor) and asyncpg (connection) interfaces
- **Transaction Scope**: Uses `SET LOCAL` to properly scope variables to the current transaction
- **Conditional Setting**: Only sets variables when present in the GraphQL context

### 📊 Testing

Comprehensive test coverage added:
- 9 new test cases covering all execution modes
- Parametrized tests ensuring consistency across modes
- Tests for conditional variable setting
- Verification of transaction-scoped `SET LOCAL` usage

### 🔄 Migration

**No migration required!** This change is fully backwards compatible:
- Existing TurboRouter behavior unchanged
- No breaking changes to APIs or interfaces
- Automatically benefits all existing queries

### 📝 Example Usage

When your GraphQL context includes tenant information:
```python
context = {
    "tenant_id": "abc-123",
    "contact_id": "user-456",
    # ... other context
}
```

FraiseQL will automatically execute:
```sql
SET LOCAL app.tenant_id = 'abc-123';
SET LOCAL app.contact_id = 'user-456';
```

Before every database query, regardless of execution mode.

### 🙏 Acknowledgments

This feature was requested by the PrintOptim Backend Team to address production issues with multi-tenant query reliability.

### 📦 Installation

```bash
pip install fraiseql==0.7.22
```

### 🐛 Bug Reports

Please report any issues at: https://github.com/fraiseql/fraiseql/issues

---

**Full Changelog**: [v0.7.21...v0.7.22](https://github.com/fraiseql/fraiseql/compare/v0.7.21...v0.7.22)
