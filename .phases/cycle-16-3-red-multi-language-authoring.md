# Cycle 16-3: RED Phase - Multi-Language Authoring Requirements

**Cycle**: 3 of 8
**Phase**: RED (Write failing tests first)
**Duration**: ~3-4 days
**Focus**: Define Python and TypeScript federation decorators through failing tests

**Prerequisites**:
- Cycle 1-2 (Core Federation Runtime) complete and passing
- Federation types and entity resolver working
- Ready to build authoring layer

---

## Objective

Define federation authoring API for Python and TypeScript:
1. Python decorators (`@key`, `@extends`, `@external`, `@requires`, `@provides`)
2. TypeScript decorators (mirror Python API)
3. Schema JSON federation metadata
4. Compile-time validation

All tests must fail initially, proving they test new functionality.

---

## Requirements Definition

### Requirement 1: Python `@key` Decorator

**Description**: Mark type as having a federation key

**Syntax**:
```python
@fraiseql.type
@fraiseql.key("id")
class User:
    id: ID
    email: str
    created_at: str
```

**Multiple Keys**:
```python
@fraiseql.type
@fraiseql.key("tenant_id")
@fraiseql.key("id")
class Account:
    tenant_id: ID
    id: ID
    name: str
```

**Acceptance Criteria**:
- [ ] Decorator is recognized
- [ ] Key field(s) parsed correctly
- [ ] Multiple keys supported
- [ ] Non-existent fields rejected
- [ ] Schema JSON includes key metadata

---

### Requirement 2: Python `@extends` & `@external` Decorators

**Description**: Extend types from other subgraphs

**Syntax**:
```python
@fraiseql.type
@fraiseql.extends
@fraiseql.key("id")
class User:
    id: ID = fraiseql.external()  # Owned by other subgraph
    email: str = fraiseql.external()

    orders: list["Order"] = fraiseql.requires("email")
```

**Acceptance Criteria**:
- [ ] `@extends` marks type as extended
- [ ] `@external()` marks fields as external
- [ ] `@requires()` marks fields needing data resolution
- [ ] External fields cannot be queried directly
- [ ] Schema JSON includes extend metadata

---

### Requirement 3: Python `@requires` & `@provides` Decorators

**Description**: Declare field dependencies

**`@requires` Example** (need other field to resolve this one):
```python
@fraiseql.type
class User:
    id: ID
    email: str

    # Need 'email' from other subgraph to resolve this
    profile: "UserProfile" = fraiseql.requires("email")
```

**`@provides` Example** (this field helps resolve other subgraph's field):
```python
@fraiseql.type
class User:
    id: ID
    email: str

    # This field provides data for Order.owner_email reference
    owner_profile: "UserProfile" = fraiseql.provides("Order.owner_email")
```

**Acceptance Criteria**:
- [ ] `@requires()` declares field dependencies
- [ ] `@provides()` declares provided data
- [ ] Circular dependencies detected
- [ ] Missing required fields validated at compile time
- [ ] Schema JSON includes requires/provides metadata

---

### Requirement 4: TypeScript Federation Decorators

**Description**: TypeScript API mirrors Python exactly

**Syntax**:
```typescript
@Key("id")
@Type()
class User {
  id: string;
  email: string;
  createdAt: string;
}

@Extends()
@Key("id")
@Type()
class User {
  @External() id: string;
  @External() email: string;

  @Requires("email") orders: Order[];
}
```

**Acceptance Criteria**:
- [ ] Decorators match Python API
- [ ] Same validation rules as Python
- [ ] Schema JSON generation identical
- [ ] Works with TypeScript interfaces
- [ ] Type-safe implementation

---

### Requirement 5: Schema JSON Federation Metadata

**Description**: Generated schema.json includes federation directives

**Schema JSON Extension**:
```json
{
  "types": [
    {
      "name": "User",
      "kind": "OBJECT",
      "fields": [
        {
          "name": "id",
          "type": "ID",
          "federation": {
            "external": false,
            "requires": null
          }
        },
        {
          "name": "email",
          "type": "String",
          "federation": {
            "external": false
          }
        }
      ],
      "federation": {
        "keys": [
          {
            "fields": ["id"],
            "resolvable": true
          }
        ],
        "extend": false,
        "shareable": false,
        "external_fields": [],
        "provides_data": []
      }
    }
  ],
  "federation": {
    "enabled": true,
    "version": "v2"
  }
}
```

**Acceptance Criteria**:
- [ ] Federation metadata present in schema.json
- [ ] All directives captured
- [ ] Keys correctly represented
- [ ] External fields marked
- [ ] Requires/provides relationships documented

---

### Requirement 6: Compile-Time Validation

**Description**: Catch federation errors at schema compilation time

**Validation Rules**:
```
✓ All @key fields must exist in type
✗ @key("nonexistent") → Error
✓ @requires fields must be valid queries
✗ @requires("missing_field") → Error
✓ @external fields must be in @extends type
✗ Regular type with @external → Error
✓ No circular @requires dependencies
✗ A requires B, B requires A → Error
✓ @provides must reference valid fields
✗ @provides("UnknownType.field") → Error
```

**Acceptance Criteria**:
- [ ] Invalid key fields rejected
- [ ] Invalid requires fields rejected
- [ ] External on non-extended type rejected
- [ ] Circular requires detected
- [ ] Invalid provides targets rejected
- [ ] Clear error messages

---

### Requirement 7: Field Inheritance & Validation

**Description**: Validate field relationships across subgraphs

**Example**:
```python
# In subgraph A (owns User)
@fraiseql.type
@fraiseql.key("id")
class User:
    id: ID
    email: str

# In subgraph B (extends User)
@fraiseql.type
@fraiseql.extends
@fraiseql.key("id")
class User:
    id: ID = fraiseql.external()
    email: str = fraiseql.external()

    orders: list["Order"]  # NEW field in extension
```

**Acceptance Criteria**:
- [ ] External fields match original type
- [ ] Type compatibility validated
- [ ] New fields in extensions allowed
- [ ] Field count can increase in extensions

---

## Test Files to Create

### 1. Python Decorator Tests: `fraiseql-python/tests/test_federation_decorators.py`

```python
import pytest
from fraiseql import type, key, extends, external, requires, provides


class TestKeyDecorator:
    def test_key_single_field(self):
        """@key("id") marks type as having federation key"""
        @type
        @key("id")
        class User:
            id: str

        # Assert: metadata includes key
        assert hasattr(User, "__fraiseql_federation__")
        assert User.__fraiseql_federation__["keys"] == [{"fields": ["id"]}]

    def test_key_multiple_fields(self):
        """Multiple @key decorators for composite keys"""
        @type
        @key("tenant_id")
        @key("id")
        class Account:
            tenant_id: str
            id: str

        # Assert: both keys present
        keys = Account.__fraiseql_federation__["keys"]
        assert len(keys) == 2
        assert {"fields": ["tenant_id"]} in keys
        assert {"fields": ["id"]} in keys

    def test_key_nonexistent_field(self):
        """@key with non-existent field raises error"""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @type
            @key("nonexistent")
            class User:
                id: str


class TestExtendsDecorator:
    def test_extends_marks_type(self):
        """@extends marks type as extended"""
        @type
        @extends
        @key("id")
        class User:
            id: str = external()

        assert User.__fraiseql_federation__["extend"] is True

    def test_external_field_in_extends(self):
        """@external() marks field as external"""
        @type
        @extends
        @key("id")
        class User:
            id: str = external()
            email: str = external()

        ext_fields = User.__fraiseql_federation__["external_fields"]
        assert "id" in ext_fields
        assert "email" in ext_fields

    def test_external_on_non_extended_type(self):
        """@external on non-extended type raises error"""
        with pytest.raises(ValueError, match="@external can only be used with @extends"):
            @type
            @key("id")
            class User:
                id: str = external()


class TestRequiresDecorator:
    def test_requires_marks_dependency(self):
        """@requires("field") marks field as needing data resolution"""
        @type
        class User:
            id: str
            email: str
            profile: "UserProfile" = requires("email")

        assert User.__fraiseql_federation__["requires"]["profile"] == "email"

    def test_requires_nonexistent_field(self):
        """@requires("nonexistent") raises error"""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            @type
            class User:
                id: str
                profile: "UserProfile" = requires("nonexistent")

    def test_requires_circular_dependency(self):
        """Circular @requires dependencies detected"""
        # This requires compile-time check
        # Schema: A requires B, B requires A
        schema = """
        type A { id: ID, b: B = @requires("x") }
        type B { id: ID, a: A = @requires("y") }
        """
        with pytest.raises(ValueError, match="Circular dependency"):
            compile_federation_schema(schema)


class TestProvidesDecorator:
    def test_provides_marks_data_provider(self):
        """@provides marks field as providing data for other subgraph"""
        @type
        class User:
            id: str
            email: str
            owner_profile: "UserProfile" = provides("Order.owner_email")

        provides_data = User.__fraiseql_federation__["provides_data"]
        assert "Order.owner_email" in provides_data


class TestSchemaJSONGeneration:
    def test_schema_json_includes_federation_metadata(self):
        """schema.json includes federation directives"""
        @type
        @key("id")
        class User:
            id: str
            email: str

        schema_json = generate_schema_json([User])

        assert schema_json["federation"]["enabled"] is True
        assert schema_json["federation"]["version"] == "v2"

        user_type = schema_json["types"][0]
        assert user_type["federation"]["keys"] == [{"fields": ["id"]}]

    def test_schema_json_external_fields(self):
        """schema.json marks external fields"""
        @type
        @extends
        @key("id")
        class User:
            id: str = external()
            email: str = external()

        schema_json = generate_schema_json([User])
        user_type = schema_json["types"][0]

        assert "id" in user_type["federation"]["external_fields"]
        assert "email" in user_type["federation"]["external_fields"]


class TestCompileTimeValidation:
    def test_invalid_key_field_rejected(self):
        """Invalid key field rejected at compile time"""
        with pytest.raises(ValueError, match="Field 'nonexistent' not found"):
            compile_schema([
                ("User", {"id": "ID"}, {"key": "nonexistent"})
            ])

    def test_external_without_extends_rejected(self):
        """External field without @extends rejected"""
        with pytest.raises(ValueError, match="@external requires @extends"):
            compile_schema([
                ("User", {"id": "ID"}, {"external": ["id"]})
            ])

    def test_circular_requires_rejected(self):
        """Circular requires dependencies rejected"""
        with pytest.raises(ValueError, match="Circular"):
            compile_schema([
                ("A", {}, {"requires": {"b": "x"}}),
                ("B", {}, {"requires": {"a": "y"}})
            ])
```

### 2. TypeScript Decorator Tests: `fraiseql-typescript/tests/federation.test.ts`

```typescript
import { Type, Key, Extends, External, Requires, Provides } from '../src/federation';

describe('TypeScript Federation Decorators', () => {
    describe('@Key decorator', () => {
        it('marks type with federation key', () => {
            @Key('id')
            @Type()
            class User {
                id: string;
                email: string;
            }

            expect(User.__fraiseqlFederation__.keys).toEqual([{ fields: ['id'] }]);
        });

        it('supports multiple keys', () => {
            @Key('tenant_id')
            @Key('id')
            @Type()
            class Account {
                tenant_id: string;
                id: string;
            }

            const keys = Account.__fraiseqlFederation__.keys;
            expect(keys).toHaveLength(2);
            expect(keys).toContainEqual({ fields: ['tenant_id'] });
        });

        it('rejects non-existent fields', () => {
            expect(() => {
                @Key('nonexistent')
                @Type()
                class User {
                    id: string;
                }
            }).toThrow('Field \'nonexistent\' not found');
        });
    });

    describe('@Extends decorator', () => {
        it('marks type as extended', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
            }

            expect(User.__fraiseqlFederation__.extend).toBe(true);
        });

        it('works with @External decorator', () => {
            @Extends()
            @Key('id')
            @Type()
            class User {
                @External() id: string;
                @External() email: string;
            }

            expect(User.__fraiseqlFederation__.external_fields).toContain('id');
            expect(User.__fraiseqlFederation__.external_fields).toContain('email');
        });
    });

    describe('@Requires decorator', () => {
        it('marks field dependencies', () => {
            @Type()
            class User {
                id: string;
                email: string;

                @Requires('email')
                profile: UserProfile;
            }

            expect(User.__fraiseqlFederation__.requires.profile).toBe('email');
        });

        it('rejects non-existent fields', () => {
            expect(() => {
                @Type()
                class User {
                    id: string;

                    @Requires('nonexistent')
                    profile: UserProfile;
                }
            }).toThrow('Field \'nonexistent\' not found');
        });
    });
});
```

### 3. Schema JSON Tests: `fraiseql-python/tests/test_federation_schema.py`

```python
def test_schema_json_federation_metadata():
    """Generated schema.json includes federation metadata"""
    @type
    @key("id")
    class User:
        id: ID
        email: str

    schema = Schema(types=[User])
    json_data = schema.to_json()

    assert json_data["federation"]["enabled"] is True
    assert json_data["federation"]["version"] == "v2"

    user_type = next(t for t in json_data["types"] if t["name"] == "User")
    assert user_type["federation"]["keys"] == [{"fields": ["id"]}]


def test_schema_json_extends_metadata():
    """Extended types marked in schema.json"""
    @type
    @extends
    @key("id")
    class User:
        id: ID = external()

    schema = Schema(types=[User])
    json_data = schema.to_json()

    user_type = next(t for t in json_data["types"] if t["name"] == "User")
    assert user_type["federation"]["extend"] is True
    assert "id" in user_type["federation"]["external_fields"]
```

### 4. Integration Tests: `fraiseql-python/tests/test_federation_compilation.py`

```python
def test_federation_schema_compilation():
    """Complete federation schema compiles successfully"""
    @type
    @key("id")
    class User:
        id: ID
        email: str

    @type
    @key("id")
    class Order:
        id: ID
        user_id: ID
        total: float

    schema = Schema(types=[User, Order])
    compiled = schema.compile()

    # Should include federation metadata
    assert compiled.federation is not None
    assert compiled.federation.enabled is True


def test_federation_schema_validation():
    """Federation schema validation catches errors"""
    with pytest.raises(FederationValidationError):
        @type
        @key("nonexistent")
        class User:
            id: ID

        Schema(types=[User]).compile()
```

---

## Running Tests

```bash
# Python tests
cd fraiseql-python
pytest tests/test_federation_decorators.py -v
pytest tests/test_federation_schema.py -v
pytest tests/test_federation_compilation.py -v

# Expected: All fail (no implementation yet)
test_key_single_field ... FAILED
test_key_multiple_fields ... FAILED
test_key_nonexistent_field ... FAILED
# ... etc

# TypeScript tests
cd fraiseql-typescript
npm test tests/federation.test.ts

# Expected: All fail
```

---

## Test File Structure

```
fraiseql-python/tests/
├── test_federation_decorators.py      (Python decorator tests)
├── test_federation_schema.py           (Schema JSON generation)
└── test_federation_compilation.py      (Compilation validation)

fraiseql-typescript/tests/
└── federation.test.ts                  (TypeScript decorator tests)
```

---

## Validation Checklist

- [ ] All Python decorator tests written (20+ tests)
- [ ] All TypeScript tests written (10+ tests)
- [ ] All schema JSON tests written (10+ tests)
- [ ] All tests fail with clear error messages
- [ ] Each test focused on single requirement
- [ ] Tests are not interdependent
- [ ] Error cases covered

---

**Status**: [~] In Progress (Writing tests)
**Next**: GREEN Phase - Implement Python/TypeScript decorators
