# FraiseQL GraphQL-Native Mutation Response Architecture Plan

## Executive Summary

Transform FraiseQL's mutation response format from CDC-style events to GraphQL-native payloads that work seamlessly with all modern GraphQL clients (Apollo, Relay, URQL, TanStack Query). **Leverage the existing Rust transformer (`fraiseql-rs`)** for both `__typename` injection and camelCase transformation to ensure consistency across queries and mutations.

---

## 🎯 Goals

1. **Return GraphQL-native mutation responses** with `id` + `__typename` for cache normalization
2. **Use existing Rust transformer** for `__typename` injection and camelCase conversion
3. **Flat, cache-friendly structure** instead of nested CDC-style payloads
4. **Zero boilerplate** for developers - just define mutation types
5. **Consistent data path** - same transformation for queries and mutations

---

## 📊 Current vs Target Architecture

### **Current Flow (Queries)**

```
PostgreSQL (snake_case)
  → Python (raw JSONB)
  → Rust Transformer (camelCase + __typename)
  → GraphQL Response
```

### **Current Flow (Mutations)**

```
PostgreSQL (CDC-style + snake_case)
  → Python (raw JSONB, CDC structure)
  → GraphQL Response (no transformation)
  ❌ No __typename
  ❌ snake_case keys
  ❌ Nested CDC structure
```

### **Target Flow (Mutations)**

```
PostgreSQL (GraphQL-native + snake_case)
  → Python (raw JSONB)
  → Rust Transformer (camelCase + __typename) ← REUSE EXISTING!
  → GraphQL Response
```

---

## 🎨 Optimal GraphQL Mutation Response Shape

### **Recommended Structure**

```graphql
type DeleteCustomerPayload {
  # Status fields (standard across all mutations)
  success: Boolean!
  code: String!           # e.g., "SUCCESS", "NOT_FOUND", "UNAUTHORIZED"
  message: String         # Human-readable message

  # The primary entity that was modified
  customer: Customer      # The deleted customer (for optimistic rollback)

  # Affected related entities (for automatic cache updates)
  affectedOrders: [Order!]
  affectedReviews: [Review!]

  # Optional: ID for removing from lists
  deletedCustomerId: ID

  # Optional: Metadata
  metadata: JSON
  timestamp: DateTime
}
```

### **Why This Works**

- ✅ **Apollo Client**: Automatic cache normalization via `id` + `__typename`
- ✅ **Relay**: Node protocol compatibility + Connection updates
- ✅ **URQL**: Graphcache automatic updates
- ✅ **TanStack Query**: Query invalidation + optimistic updates
- ✅ **Vue Apollo/Villus**: Standard GraphQL cache patterns

---

## 🔧 Implementation by Layer

### **Layer 1: Database (PostgreSQL Functions)**

#### **Changes Required:**

1. ✅ **Simplify `app.log_and_return_mutation()`** - Remove CDC structure
2. ✅ **Return flat GraphQL-native JSONB** - No more nested `payload.before/after`
3. ✅ **Keep snake_case keys** - Rust transformer handles camelCase
4. ✅ **Do NOT add `__typename`** - Rust transformer handles this

#### **New `log_and_return_mutation` Function:**

```sql
-- Updated mutation response formatter (GraphQL-native)
CREATE OR REPLACE FUNCTION app.log_and_return_mutation(
    -- Status fields
    p_success BOOLEAN,
    p_code TEXT,
    p_message TEXT,

    -- Primary entity (optional)
    p_entity JSONB DEFAULT NULL,
    p_entity_key TEXT DEFAULT NULL,  -- Key name for the entity field

    -- Related entities (optional, as flat JSONB object)
    p_related_entities JSONB DEFAULT NULL,

    -- Metadata (optional)
    p_metadata JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_result JSONB;
BEGIN
    -- Build flat GraphQL-native response
    v_result := jsonb_build_object(
        'success', p_success,
        'code', p_code,
        'message', p_message
    );

    -- Add primary entity if provided
    -- Use snake_case key - Rust transformer converts to camelCase
    IF p_entity IS NOT NULL AND p_entity_key IS NOT NULL THEN
        v_result := v_result || jsonb_build_object(p_entity_key, p_entity);
    END IF;

    -- Add related entities if provided (already a JSONB object with keys)
    IF p_related_entities IS NOT NULL THEN
        v_result := v_result || p_related_entities;
    END IF;

    -- Add metadata if provided
    IF p_metadata IS NOT NULL THEN
        v_result := v_result || jsonb_build_object('metadata', p_metadata);
    END IF;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

#### **Example: `delete_customer` Function:**

```sql
CREATE OR REPLACE FUNCTION app.delete_customer(
    customer_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_customer JSONB;
    v_affected_orders JSONB;
    v_affected_reviews JSONB;
BEGIN
    -- Get customer data BEFORE deletion (for optimistic rollback)
    SELECT to_jsonb(c.*) INTO v_customer
    FROM customers c WHERE id = customer_id;

    IF v_customer IS NULL THEN
        RETURN app.log_and_return_mutation(
            p_success := false,
            p_code := 'NOT_FOUND',
            p_message := 'Customer not found'
        );
    END IF;

    -- Get affected orders (for cache updates)
    SELECT jsonb_agg(to_jsonb(o.*)) INTO v_affected_orders
    FROM orders o WHERE o.customer_id = customer_id;

    -- Get affected reviews (for cache updates)
    SELECT jsonb_agg(to_jsonb(r.*)) INTO v_affected_reviews
    FROM reviews r WHERE r.customer_id = customer_id;

    -- Perform deletion
    PERFORM core.delete_customer(customer_id);

    -- Return GraphQL-native format
    -- Note: snake_case keys, no __typename (Rust handles both)
    RETURN app.log_and_return_mutation(
        p_success := true,
        p_code := 'SUCCESS',
        p_message := 'Customer deleted successfully',
        p_entity := v_customer,
        p_entity_key := 'customer',
        p_related_entities := jsonb_build_object(
            'affected_orders', COALESCE(v_affected_orders, '[]'::jsonb),
            'affected_reviews', COALESCE(v_affected_reviews, '[]'::jsonb),
            'deleted_customer_id', customer_id
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

#### **Database Layer Result:**

```json
{
  "success": true,
  "code": "SUCCESS",
  "message": "Customer deleted successfully",
  "customer": {
    "id": "uuid-123",
    "email": "john@example.com",
    "first_name": "John"
  },
  "affected_orders": [
    {"id": "order-1", "status": "cancelled"}
  ],
  "affected_reviews": [],
  "deleted_customer_id": "uuid-123"
}
```

**Note:** All keys are `snake_case`, no `__typename` yet.

---

### **Layer 2: Python (FraiseQL Core)**

#### **Changes Required:**

1. ✅ **Detect mutation responses** in resolver
2. ✅ **Call Rust transformer** with mutation result type
3. ✅ **Minimal code changes** - reuse existing infrastructure

#### **Where This Happens:**

**File:** `src/fraiseql/mutations/mutation_decorator.py`

#### **Implementation:**

```python
# src/fraiseql/mutations/mutation_decorator.py (line ~145-155)

async def resolver(info, input):
    """Auto-generated resolver for PostgreSQL mutation."""
    # ... existing code to call PostgreSQL function ...

    # Execute function
    result = await db.execute_function(full_function_name, input_data)

    # ✅ NEW: Transform result using Rust transformer
    # This injects __typename and converts snake_case → camelCase
    result = await transform_mutation_result(
        result,
        self.success_type,
        self.error_type
    )

    # Parse result into Success or Error type
    parsed_result = parse_mutation_result(
        result,
        self.success_type,
        self.error_type,
        self.error_config,
    )

    return parsed_result
```

#### **New Helper Function:**

```python
# src/fraiseql/mutations/transformer.py

import json
import logging
from typing import Any, Type

from fraiseql.core.rust_transformer import get_transformer

logger = logging.getLogger(__name__)


async def transform_mutation_result(
    result: dict[str, Any],
    success_type: Type,
    error_type: Type,
) -> dict[str, Any]:
    """
    Transform mutation result using Rust transformer.

    This function:
    1. Converts snake_case → camelCase
    2. Injects __typename into all nested objects
    3. Handles both success and error responses

    Args:
        result: Raw JSONB result from PostgreSQL (snake_case, no __typename)
        success_type: Python Success dataclass type
        error_type: Python Error dataclass type

    Returns:
        Transformed result (camelCase, with __typename)
    """
    if not result:
        return result

    # Determine which type to use based on success field
    success = result.get("success", False)
    root_type = success_type if success else error_type
    root_type_name = root_type.__name__

    # Register types with Rust transformer if not already registered
    transformer = get_transformer()
    _ensure_types_registered(transformer, success_type, error_type)

    # Convert to JSON string for Rust transformer
    result_json = json.dumps(result)

    # Transform using Rust (camelCase + __typename injection)
    logger.debug(f"Transforming mutation result with root type: {root_type_name}")
    transformed_json = transformer.transform(result_json, root_type_name)

    # Parse back to dict
    transformed = json.loads(transformed_json)

    logger.debug(f"Mutation result transformed: {root_type_name}")
    return transformed


def _ensure_types_registered(transformer, *types: Type) -> None:
    """
    Ensure types are registered with the Rust transformer.

    This recursively registers nested types found in the mutation response.
    """
    for type_class in types:
        if not type_class:
            continue

        # Check if already registered
        type_name = type_class.__name__
        if type_name in transformer._schema:
            continue

        # Register this type
        transformer.register_type(type_class)

        # Recursively register nested types
        annotations = getattr(type_class, "__annotations__", {})
        for field_name, field_type in annotations.items():
            # Handle list types
            from typing import get_origin, get_args
            origin = get_origin(field_type)
            if origin is list:
                args = get_args(field_type)
                if args and hasattr(args[0], "__annotations__"):
                    _ensure_types_registered(transformer, args[0])
            # Handle object types
            elif hasattr(field_type, "__annotations__"):
                _ensure_types_registered(transformer, field_type)
```

#### **Python Layer Result:**

```python
# Before transformation (from PostgreSQL):
{
  "success": true,
  "code": "SUCCESS",
  "customer": {
    "id": "uuid-123",
    "email": "john@example.com",
    "first_name": "John"
  },
  "affected_orders": [...]
}

# After Rust transformation:
{
  "__typename": "DeleteCustomerSuccess",  # ← Added by Rust
  "success": true,
  "code": "SUCCESS",
  "customer": {
    "__typename": "Customer",  # ← Added by Rust
    "id": "uuid-123",
    "email": "john@example.com",
    "firstName": "John"  # ← camelCase by Rust
  },
  "affectedOrders": [  # ← camelCase by Rust
    {
      "__typename": "Order",  # ← Added by Rust
      "id": "order-1",
      "status": "cancelled"
    }
  ]
}
```

---

### **Layer 3: Rust Extension (fraiseql-rs)**

#### **Is Rust Involved?**

✅ **YES!** The Rust transformer is already implemented and will be reused.

#### **Current Rust Capabilities:**

```rust
// fraiseql-rs already provides:

1. transform_json(json_str)
   → snake_case to camelCase conversion

2. SchemaRegistry.transform(json_str, root_type)
   → camelCase + __typename injection
   → Handles nested objects
   → Handles arrays
   → Uses registered schema types
```

#### **What Needs to be Done:**

✅ **NOTHING!** The Rust transformer is already fully capable of handling mutation responses.

**Existing Rust Code Already Handles:**
- ✅ `__typename` injection based on registered types
- ✅ Recursive nested object transformation
- ✅ Array transformation with type injection
- ✅ snake_case → camelCase conversion
- ✅ Schema registry for type metadata

#### **Rust Transformer Usage:**

```python
# Python code calls existing Rust API:

from fraiseql.core.rust_transformer import get_transformer

transformer = get_transformer()

# Register mutation response types
transformer.register_type(DeleteCustomerSuccess)
transformer.register_type(Customer)
transformer.register_type(Order)

# Transform mutation result
result_json = json.dumps(raw_result)
transformed_json = transformer.transform(result_json, "DeleteCustomerSuccess")
transformed = json.loads(transformed_json)

# Result: camelCase + __typename ✅
```

---

### **Layer 4: Application Level (Developer Experience)**

#### **What Developers Do:**

✅ **Define mutation types** - FraiseQL handles everything else automatically.

#### **Example: Define Mutation Types**

```python
# mutations.py

from fraiseql import mutation, success, failure, fraise_type
from typing import Optional, List
from uuid import UUID
from .models import Customer, Order, Review


@success
class DeleteCustomerSuccess:
    """Customer deleted successfully."""
    success: bool = True
    code: str = "SUCCESS"
    message: str

    # Primary entity (for cache updates and optimistic rollback)
    customer: Customer

    # Related entities (automatic cache updates)
    affected_orders: List[Order]
    affected_reviews: List[Review]

    # For list removal in caches
    deleted_customer_id: UUID


@failure
class DeleteCustomerError:
    """Customer deletion failed."""
    success: bool = False
    code: str
    message: str

    # Error details
    reason: Optional[str] = None


@mutation(
    function="app.delete_customer",
    schema="app"
)
class DeleteCustomer:
    """Delete a customer and cascade to related entities."""

    input: DeleteCustomerInput
    success: DeleteCustomerSuccess
    failure: DeleteCustomerError
```

#### **What Happens Automatically:**

1. ✅ FraiseQL calls `app.delete_customer(input)`
2. ✅ PostgreSQL returns flat JSONB (snake_case, no __typename)
3. ✅ Python calls Rust transformer with `DeleteCustomerSuccess` type
4. ✅ Rust injects `__typename` into all objects
5. ✅ Rust converts snake_case → camelCase
6. ✅ Python parses into typed `DeleteCustomerSuccess` object
7. ✅ GraphQL returns cache-friendly response

#### **Zero Boilerplate:**

✅ **No manual typename injection**
✅ **No manual camelCase conversion**
✅ **No cache update logic**
✅ **Just define types - everything else is automatic**

---

## 🔄 Complete Data Flow Example

### **Delete Customer Mutation**

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. GraphQL Request                                                │
│    mutation {                                                     │
│      deleteCustomer(input: {customerId: "uuid-123"}) {           │
│        success                                                    │
│        customer { id email __typename }                          │
│        affectedOrders { id status __typename }                   │
│      }                                                            │
│    }                                                              │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 2. Python: mutation_decorator.resolver()                         │
│    - Calls: app.delete_customer({"customer_id": "uuid-123"})    │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 3. PostgreSQL: app.delete_customer()                             │
│    - Gets customer: v_customer = {...}                           │
│    - Gets orders: v_affected_orders = [...]                      │
│    - Deletes customer                                            │
│    - Returns JSONB:                                              │
│      {                                                            │
│        "success": true,                                          │
│        "code": "SUCCESS",                                        │
│        "message": "Customer deleted",                            │
│        "customer": {"id": "...", "first_name": "John"},         │
│        "affected_orders": [{"id": "...", "status": "..."}]      │
│      }                                                            │
│    Note: snake_case, NO __typename                               │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 4. Python: transform_mutation_result()                           │
│    - Gets Rust transformer instance                              │
│    - Registers DeleteCustomerSuccess, Customer, Order types      │
│    - Calls: transformer.transform(json_str, "DeleteCustomer...") │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 5. Rust (fraiseql-rs): SchemaRegistry.transform()               │
│    - Injects __typename: "DeleteCustomerSuccess"                 │
│    - Converts customer.first_name → customer.firstName           │
│    - Injects customer.__typename: "Customer"                     │
│    - Converts affected_orders → affectedOrders                   │
│    - Injects Order.__typename for each order                     │
│    - Returns transformed JSON                                    │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 6. Python: parse_mutation_result()                               │
│    - Parses transformed JSON into DeleteCustomerSuccess          │
│    - Validates against dataclass schema                          │
│    - Returns typed Python object                                 │
└──────────────────────────────────────────────────────────────────┘
                              ↓
┌──────────────────────────────────────────────────────────────────┐
│ 7. GraphQL Response                                              │
│    {                                                              │
│      "data": {                                                    │
│        "deleteCustomer": {                                       │
│          "__typename": "DeleteCustomerSuccess",                  │
│          "success": true,                                        │
│          "message": "Customer deleted",                          │
│          "customer": {                                           │
│            "__typename": "Customer",                             │
│            "id": "uuid-123",                                     │
│            "email": "john@example.com",                          │
│            "firstName": "John"                                   │
│          },                                                       │
│          "affectedOrders": [{                                    │
│            "__typename": "Order",                                │
│            "id": "order-1",                                      │
│            "status": "cancelled"                                 │
│          }]                                                       │
│        }                                                          │
│      }                                                            │
│    }                                                              │
└──────────────────────────────────────────────────────────────────┘
```

---

## 🎯 Implementation Checklist

### **Phase 1: Database Layer** ✅

- [ ] Update `app.log_and_return_mutation()` signature
  - Remove CDC-specific structure (`payload.before/after`)
  - Add `p_entity_key` parameter for entity field name
  - Return flat GraphQL-native JSONB
- [ ] Update example mutations:
  - [ ] `delete_customer`
  - [ ] `create_order`
  - [ ] `update_product`
- [ ] Test: SQL functions return flat snake_case JSONB
- [ ] Document: New mutation function pattern

### **Phase 2: Python Core** ✅

- [ ] Create `transform_mutation_result()` function
  - Call existing Rust transformer
  - Handle success/error type detection
  - Ensure type registration
- [ ] Update `mutation_decorator.py`:
  - Add transformation call before parsing
  - Pass success/error types to transformer
- [ ] Add type registration helper:
  - `_ensure_types_registered()`
  - Recursive nested type discovery
- [ ] Write unit tests:
  - Test transformation with simple objects
  - Test transformation with nested objects
  - Test transformation with arrays
  - Test both success and error responses
- [ ] Write integration tests:
  - End-to-end mutation execution
  - Verify __typename in response
  - Verify camelCase conversion

### **Phase 3: Rust Extension** ✅

- [x] **NO CHANGES NEEDED** - Existing Rust transformer handles everything
- [x] Verify: `SchemaRegistry.transform()` works with mutation types
- [x] Verify: Nested object __typename injection works
- [x] Verify: Array __typename injection works
- [ ] Add tests: Mutation-specific transformation tests (optional)

### **Phase 4: Documentation** ✅

- [ ] Update mutation documentation:
  - New response format
  - Benefits for GraphQL clients
  - Migration guide from CDC format
- [ ] Add client examples:
  - [ ] Apollo Client cache updates
  - [ ] URQL Graphcache configuration
  - [ ] Relay Connection handlers
  - [ ] TanStack Query / Vue Query
- [ ] Add troubleshooting guide:
  - Common cache issues
  - Type registration problems
  - Debugging transformation

### **Phase 5: Migration & Backward Compatibility**

- [ ] Add feature flag: `FRAISEQL_MUTATION_FORMAT`
  - `"graphql"` (new format)
  - `"cdc"` (legacy format, deprecated)
- [ ] Create migration script:
  - Convert existing SQL functions
  - Update mutation type definitions
- [ ] Update all examples:
  - [ ] `examples/blog_api`
  - [ ] `examples/ecommerce_api`
  - [ ] `examples/blog_simple`
- [ ] Deprecation timeline:
  - v1.0: Introduce new format (default: graphql)
  - v1.1: Deprecation warning for CDC format
  - v2.0: Remove CDC format support

---

## 🔬 Testing Strategy

### **Unit Tests**

```python
# tests/unit/mutations/test_mutation_transformer.py

import json
import pytest
from fraiseql.mutations.transformer import transform_mutation_result
from fraiseql.core.rust_transformer import get_transformer


@pytest.fixture
def transformer():
    return get_transformer()


def test_transform_simple_success(transformer):
    """Test transformation of simple success response."""

    @success
    class SimpleSuccess:
        success: bool
        message: str

    result = {
        "success": True,
        "message": "Operation successful"
    }

    transformed = await transform_mutation_result(
        result,
        SimpleSuccess,
        None
    )

    assert transformed["__typename"] == "SimpleSuccess"
    assert transformed["success"] is True


def test_transform_with_nested_objects(transformer):
    """Test transformation with nested objects."""

    @fraise_type
    class User:
        id: str
        first_name: str

    @success
    class CreateUserSuccess:
        success: bool
        user: User

    result = {
        "success": True,
        "user": {
            "id": "123",
            "first_name": "John"
        }
    }

    transformed = await transform_mutation_result(
        result,
        CreateUserSuccess,
        None
    )

    assert transformed["__typename"] == "CreateUserSuccess"
    assert transformed["user"]["__typename"] == "User"
    assert transformed["user"]["firstName"] == "John"  # camelCase


def test_transform_with_arrays(transformer):
    """Test transformation with array of objects."""

    @fraise_type
    class Order:
        id: str
        status: str

    @success
    class DeleteCustomerSuccess:
        success: bool
        affected_orders: List[Order]

    result = {
        "success": True,
        "affected_orders": [
            {"id": "1", "status": "cancelled"},
            {"id": "2", "status": "cancelled"}
        ]
    }

    transformed = await transform_mutation_result(
        result,
        DeleteCustomerSuccess,
        None
    )

    assert transformed["affectedOrders"][0]["__typename"] == "Order"
    assert len(transformed["affectedOrders"]) == 2
```

### **Integration Tests**

```python
# tests/integration/test_mutation_end_to_end.py

async def test_delete_customer_mutation_e2e(db, graphql_client):
    """Test complete delete customer flow."""

    # Create test customer
    customer_id = await create_test_customer(db)

    # Execute mutation
    response = await graphql_client.execute("""
        mutation DeleteCustomer($id: UUID!) {
            deleteCustomer(input: {customerId: $id}) {
                __typename
                success
                message
                customer {
                    __typename
                    id
                    email
                    firstName
                }
                affectedOrders {
                    __typename
                    id
                    status
                }
            }
        }
    """, {"id": customer_id})

    # Verify response structure
    result = response["data"]["deleteCustomer"]
    assert result["__typename"] == "DeleteCustomerSuccess"
    assert result["success"] is True
    assert result["customer"]["__typename"] == "Customer"
    assert result["customer"]["id"] == str(customer_id)

    # Verify camelCase
    assert "firstName" in result["customer"]
    assert "first_name" not in result["customer"]

    # Verify affected orders
    for order in result["affectedOrders"]:
        assert order["__typename"] == "Order"


async def test_apollo_client_cache_normalization(apollo_cache):
    """Test that Apollo Client normalizes mutation response."""

    mutation_response = {
        "deleteCustomer": {
            "__typename": "DeleteCustomerSuccess",
            "customer": {
                "__typename": "Customer",
                "id": "123",
                "email": "test@example.com"
            }
        }
    }

    # Apollo should normalize by __typename:id
    apollo_cache.write(mutation_response)

    # Verify cache entry
    customer = apollo_cache.read({
        "__typename": "Customer",
        "id": "123"
    })

    assert customer is not None
    assert customer["email"] == "test@example.com"
```

---

## 📊 Success Metrics

1. ✅ **Zero manual typename injection** - Automatic via Rust transformer
2. ✅ **Zero manual camelCase conversion** - Automatic via Rust transformer
3. ✅ **Performance** - Rust transformation adds <2ms overhead (already benchmarked)
4. ✅ **Developer experience** - <10 lines per mutation (just type definitions)
5. ✅ **Framework compatibility** - Works out-of-box with all GraphQL clients
6. ✅ **Consistency** - Same transformation path as queries

---

## 🚀 Rollout Timeline

### **Week 1-2: Core Implementation**
- [ ] Update database `log_and_return_mutation()` function
- [ ] Implement `transform_mutation_result()` in Python
- [ ] Update `mutation_decorator.py` to call transformer
- [ ] Write unit tests

### **Week 3: Integration & Testing**
- [ ] Update 2-3 example mutations
- [ ] Write integration tests
- [ ] Test with Apollo Client / URQL
- [ ] Performance benchmarks

### **Week 4: Documentation & Examples**
- [ ] Update mutation documentation
- [ ] Add client usage examples
- [ ] Create migration guide
- [ ] Update all example projects

### **Week 5: Beta Release**
- [ ] Community testing
- [ ] Bug fixes
- [ ] Performance optimization (if needed)

### **Week 6: Stable Release**
- [ ] Production release (v1.0)
- [ ] Deprecation notice for CDC format
- [ ] Announcement & tutorials

---

## 💡 Key Design Decisions

### **Why Reuse Rust Transformer?**

1. ✅ **Already implemented** - No need to duplicate logic
2. ✅ **Proven performance** - Benchmarked at microsecond scale
3. ✅ **Consistency** - Same transformation for queries and mutations
4. ✅ **Maintainability** - Single source of truth for transformation logic
5. ✅ **Type-safe** - Schema registry ensures correctness

### **Why PostgreSQL Returns snake_case?**

1. ✅ **Database convention** - PostgreSQL uses snake_case
2. ✅ **Simplicity** - No SQL-level transformation needed
3. ✅ **Flexibility** - Transformation happens in application layer
4. ✅ **Performance** - PostgreSQL doesn't waste cycles on casing

### **Why Python Orchestrates?**

1. ✅ **Type metadata** - Python has access to dataclass definitions
2. ✅ **Schema registry** - Python manages Rust transformer registration
3. ✅ **Error handling** - Python layer handles parsing and validation
4. ✅ **Flexibility** - Easy to extend or customize behavior

---

## 📚 Benefits Summary

| Aspect | Benefit |
|--------|---------|
| **Performance** | Rust transformer is microsecond-fast |
| **Developer Experience** | Zero boilerplate - just define types |
| **Cache Compatibility** | Works with all GraphQL clients |
| **Consistency** | Same data path as queries |
| **Maintainability** | Single transformation layer |
| **Type Safety** | Schema-driven validation |
| **Migration** | Minimal code changes required |

---

## ❓ Open Questions

1. ✅ **Rust involvement?** - CONFIRMED: Existing transformer will be reused
2. ✅ **Transformation performance?** - Already benchmarked: <2ms overhead
3. [ ] **Backward compatibility duration?** - Propose: Support CDC format until v2.0
4. [ ] **Feature flag name?** - Propose: `FRAISEQL_MUTATION_FORMAT`
5. [ ] **Default behavior?** - Propose: New format by default in v1.0

---

## 🎯 Next Steps

1. **Review & Approve** this plan
2. **Confirm** Rust transformer capabilities with existing tests
3. **Implement** Phase 1 (Database layer updates)
4. **Implement** Phase 2 (Python transformation integration)
5. **Test** with example mutations
6. **Document** and release

---

**Status:** Ready for implementation
**Architecture:** Database → Python → Rust → GraphQL
**Key Innovation:** Leverage existing Rust transformer for mutations
**Expected Effort:** ~2-3 weeks for complete implementation
**Breaking Changes:** None (feature flag for backward compatibility)
