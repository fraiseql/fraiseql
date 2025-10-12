# FraiseQL v0.9.3 Release Notes

## ✨ Built-in Tenant-Aware APQ Caching

We're excited to announce FraiseQL v0.9.3, which introduces native tenant isolation support for Automatic Persisted Queries (APQ), enabling secure multi-tenant SaaS applications without requiring custom implementations.

### 🎯 What's New

**Automatic Tenant Isolation**: FraiseQL now automatically isolates cached APQ responses by tenant, preventing cross-tenant data leakage and ensuring each tenant only sees their own cached data.

### 🚀 Key Features

#### Zero Configuration Required
Simply pass context with tenant_id - isolation happens automatically:
```python
context = {"user": {"metadata": {"tenant_id": "acme-corp"}}}
response = backend.get_cached_response(hash, context=context)
```

#### Built-in Backend Support

**MemoryAPQBackend**:
- Tenant-specific cache keys: `{tenant_id}:{hash}`
- Separate cache spaces per tenant
- Global cache for non-tenant requests

**PostgreSQLAPQBackend**:
- New `tenant_id` column in responses table
- Composite primary key for tenant isolation
- Indexed for optimal performance

### 🔒 Security Benefits

- **Data Isolation**: Each tenant's cached responses are completely isolated
- **No Configuration Errors**: Security by default, not by configuration
- **JWT Integration**: Seamlessly works with JWT-based authentication
- **Validated**: Comprehensive test suite ensures no data leakage

### 📚 Documentation & Examples

- **Guide**: `docs/apq_tenant_context_guide.md` - Complete implementation guide
- **Example**: `examples/apq_multi_tenant.py` - Working multi-tenant application
- **Tests**: Full test coverage with tenant isolation validation

### 💻 Migration

No breaking changes! Existing applications continue to work. To enable tenant isolation:

1. Ensure your JWT includes `tenant_id` in metadata
2. Pass context to APQ operations
3. That's it - isolation is automatic!

### 🙏 Acknowledgments

Thanks to our beta testers who identified the need for built-in tenant isolation in APQ caching. Your feedback drives FraiseQL's evolution as a production-ready GraphQL framework.

### 📦 Installation

```bash
pip install --upgrade fraiseql==0.9.3
```

### 🔗 Links

- [Documentation](https://fraiseql.dev)
- [GitHub Repository](https://github.com/fraiseql/fraiseql)
- [Issue Tracker](https://github.com/fraiseql/fraiseql/issues)

---

*FraiseQL: Production-ready GraphQL for PostgreSQL with built-in multi-tenancy support*
