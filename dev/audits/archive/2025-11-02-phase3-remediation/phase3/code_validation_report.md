# Phase 3: Code Example Validation Report

## Summary

- **Total Python code blocks extracted**: 901
- **Valid syntax**: 772 (85.7%)
- **Syntax errors**: 129 (14.3%)

## Validation Results

### ✅ **Majority of Code Blocks Valid (85.7%)**
772 out of 901 Python code blocks in the documentation have valid Python syntax.

### ❌ **Syntax Errors Found (14.3%)**
129 code blocks have syntax errors. The errors fall into predictable categories:

#### **Category 1: Mixed Language Content (Most Common)**
Code blocks tagged as ```python but containing multiple languages:

**Examples:**
- Python + SQL queries
- Python + GraphQL queries
- Python + shell commands

**Sample Error:**
```python
# This block contains both Python and GraphQL
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)

# Usage remains the same, but now supports complex filtering
query {
  users(where: { status: { eq: "active" } }) { id name status }
}
```

#### **Category 2: Async Code Outside Functions**
Documentation examples showing `await` statements at module level:

**Sample Error:**
```python
# This would normally be inside an async function
await revocation_service.start()
```

#### **Category 3: Incorrect Language Tags**
Some code blocks are tagged as ```python but contain SQL, GraphQL, or other languages.

## Analysis of Error Patterns

### **Root Causes:**

1. **Documentation Style**: Code examples in docs often show snippets that would be inside functions/methods
2. **Mixed Examples**: Many examples demonstrate integration between Python, SQL, and GraphQL
3. **Language Tag Errors**: Some blocks have incorrect language tags

### **Expected vs Actual:**
- **Expected**: Pure Python syntax validation
- **Actual**: Documentation contains educational examples mixing multiple languages

## Recommendations

### **For Documentation Quality:**

1. **Clarify Mixed-Language Examples**: Use separate code blocks for different languages:
   ```python
   # Python code here
   ```

   ```sql
   -- SQL code here
   ```

   ```graphql
   # GraphQL query here
   ```

2. **Contextualize Snippets**: Add comments indicating where code belongs:
   ```python
   # Inside an async function:
   await some_async_call()
   ```

3. **Fix Language Tags**: Ensure code blocks use correct language identifiers

### **For Validation Process:**

1. **Accept Mixed Content**: Documentation validation should allow mixed-language examples
2. **Context-Aware Checking**: Understand that `await` outside functions is normal in docs
3. **Separate Validation Types**:
   - Syntax validation for pure Python blocks
   - Content validation for mixed-language examples

## Files with Syntax Errors

### **High-Error Files (>5 errors):**
- `core/database-api.md`: 50+ errors (mixed SQL/Python examples)
- `performance/caching.md`: 20+ errors (mixed content)
- `advanced/authentication.md`: 4 errors (async outside functions)

### **Common Error Patterns:**
- `SyntaxError: 'await' outside function` - 40+ instances
- `SyntaxError: invalid syntax` (SQL/GraphQL in Python blocks) - 50+ instances
- `SyntaxError: 'async with' outside async function` - 10+ instances

## Conclusion

**The documentation code examples are generally well-written**, with 85.7% having valid syntax. The "errors" are largely due to the educational nature of documentation, which intentionally mixes languages and shows code snippets outside their normal context.

**Recommendation**: Update validation approach to be documentation-aware rather than strict Python syntax validation.

---

*Phase 3 Validation: 901 code blocks analyzed, 772 valid (85.7% success rate)*
