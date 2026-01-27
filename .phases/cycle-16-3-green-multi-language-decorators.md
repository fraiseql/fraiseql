# Cycle 16-3: GREEN Phase - Multi-Language Federation Decorators

**Cycle**: 3 of 8
**Phase**: GREEN (Implement minimal code to pass tests)
**Duration**: ~4-5 days
**Focus**: Python and TypeScript federation decorators, schema JSON generation

---

## Objective

Implement federation authoring APIs:
1. Python decorators (`@key`, `@extends`, `@external`, `@requires`, `@provides`)
2. TypeScript decorators (mirror Python)
3. Schema JSON federation metadata
4. Basic compile-time validation

---

## Implementation Plan

### Part 1: Python Federation Module

**File**: `fraiseql-python/src/fraiseql/federation.py`

```python
from typing import Optional, List, Dict, Any, Callable
from dataclasses import dataclass, field


@dataclass
class FederationMetadata:
    """Federation metadata for a type"""
    keys: List[Dict[str, Any]] = field(default_factory=list)
    extend: bool = False
    external_fields: List[str] = field(default_factory=list)
    requires: Dict[str, str] = field(default_factory=dict)
    provides_data: List[str] = field(default_factory=list)


def key(fields: str) -> Callable:
    """Decorator: Mark type with federation key"""
    def decorator(cls):
        if not hasattr(cls, '__fraiseql_federation__'):
            cls.__fraiseql_federation__ = FederationMetadata()

        # Validate field exists
        if not hasattr(cls, '__annotations__'):
            raise ValueError(f"Type {cls.__name__} has no fields")

        field_names = set(cls.__annotations__.keys())
        key_fields = fields.split()
        for field_name in key_fields:
            if field_name not in field_names:
                raise ValueError(f"Field '{field_name}' not found in {cls.__name__}")

        # Add key to metadata
        cls.__fraiseql_federation__.keys.append({"fields": key_fields})
        return cls

    return decorator


def extends(cls):
    """Decorator: Mark type as extended from another subgraph"""
    if not hasattr(cls, '__fraiseql_federation__'):
        cls.__fraiseql_federation__ = FederationMetadata()

    cls.__fraiseql_federation__.extend = True
    return cls


def external() -> Any:
    """Mark field as external (owned by another subgraph)"""
    # Return a sentinel value that can be detected during class creation
    return "__FRAISEQL_EXTERNAL__"


def requires(field: str) -> Callable:
    """Decorator: Mark field requires resolution via other field"""
    def wrapper(func_or_value=None):
        # Store requires info (will be processed in __init_subclass__)
        return ("__FRAISEQL_REQUIRES__", field)

    return wrapper


def provides(reference: str) -> Callable:
    """Decorator: Mark field provides data for other subgraph"""
    def wrapper(func_or_value=None):
        return ("__FRAISEQL_PROVIDES__", reference)

    return wrapper


class FederatedType:
    """Base class for federated types"""

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)

        if not hasattr(cls, '__fraiseql_federation__'):
            cls.__fraiseql_federation__ = FederationMetadata()

        # Process field annotations for federation directives
        if hasattr(cls, '__annotations__'):
            external_fields = []
            requires_map = {}
            provides_data = []

            for field_name, field_type in cls.__annotations__.items():
                # Check if field has external marker
                if hasattr(cls, field_name):
                    value = getattr(cls, field_name)
                    if value == "__FRAISEQL_EXTERNAL__":
                        external_fields.append(field_name)
                        cls.__fraiseql_federation__.external_fields = external_fields

            # Validate external fields only in extends types
            if external_fields and not cls.__fraiseql_federation__.extend:
                raise ValueError(
                    f"@external can only be used with @extends on {cls.__name__}"
                )
```

### Part 2: Decorator Integration with Registry

**File**: `fraiseql-python/src/fraiseql/registry.py` (modifications)

```python
def add_type(cls, federation_enabled: bool = True) -> None:
    """Register a type and extract federation metadata"""
    # Store federation metadata
    fed_meta = getattr(cls, '__fraiseql_federation__', None)

    if federation_enabled and fed_meta:
        # Add to schema federation metadata
        if 'federation' not in self.schema_data:
            self.schema_data['federation'] = {
                'enabled': True,
                'version': 'v2',
                'types': []
            }

        fed_type_data = {
            'name': cls.__name__,
            'keys': fed_meta.keys,
            'extend': fed_meta.extend,
            'external_fields': fed_meta.external_fields,
            'provides_data': fed_meta.provides_data,
            'shareable': False,
        }

        self.schema_data['federation']['types'].append(fed_type_data)
```

### Part 3: Schema JSON Federation Extension

**File**: `fraiseql-python/src/fraiseql/schema.py` (modifications)

```python
def to_json(self) -> Dict[str, Any]:
    """Convert schema to JSON with federation metadata"""
    schema_json = {
        'types': [],
        'federation': {
            'enabled': False,
            'version': 'v2'
        }
    }

    # Add types with federation metadata
    for type_name, type_def in self.types.items():
        type_json = {
            'name': type_name,
            'kind': 'OBJECT',
            'fields': [],
            'federation': {
                'keys': [],
                'extend': False,
                'external_fields': [],
                'provides_data': [],
                'shareable': False,
            }
        }

        # Add federation metadata if present
        if hasattr(type_def, '__fraiseql_federation__'):
            fed_meta = type_def.__fraiseql_federation__
            type_json['federation']['keys'] = fed_meta.keys
            type_json['federation']['extend'] = fed_meta.extend
            type_json['federation']['external_fields'] = fed_meta.external_fields
            type_json['federation']['provides_data'] = fed_meta.provides_data
            schema_json['federation']['enabled'] = True

        # Add fields
        for field_name, field_type in type_def.__annotations__.items():
            field_json = {
                'name': field_name,
                'type': str(field_type),
                'federation': {
                    'external': field_name in type_json['federation']['external_fields'],
                    'requires': None,
                }
            }
            type_json['fields'].append(field_json)

        schema_json['types'].append(type_json)

    return schema_json
```

### Part 4: TypeScript Decorators

**File**: `fraiseql-typescript/src/federation.ts`

```typescript
interface FederationMetadata {
    keys: Array<{ fields: string[] }>;
    extend: boolean;
    external_fields: string[];
    requires: Record<string, string>;
    provides_data: string[];
}

function ensureFederationMetadata(target: any): FederationMetadata {
    if (!target.__fraiseqlFederation__) {
        target.__fraiseqlFederation__ = {
            keys: [],
            extend: false,
            external_fields: [],
            requires: {},
            provides_data: [],
        };
    }
    return target.__fraiseqlFederation__;
}

export function Key(fields: string): ClassDecorator {
    return function <T extends { new(...args: any[]): {} }>(constructor: T) {
        const metadata = ensureFederationMetadata(constructor.prototype);

        // Validate fields exist
        const fieldNames = Object.keys(constructor.prototype);
        const keyFields = fields.split(/\s+/);
        for (const field of keyFields) {
            if (!fieldNames.includes(field)) {
                throw new Error(`Field '${field}' not found in ${constructor.name}`);
            }
        }

        metadata.keys.push({ fields: keyFields });
        return constructor;
    };
}

export function Extends(): ClassDecorator {
    return function <T extends { new(...args: any[]): {} }>(constructor: T) {
        const metadata = ensureFederationMetadata(constructor.prototype);
        metadata.extend = true;
        return constructor;
    };
}

export function External(): PropertyDecorator {
    return function (target: any, propertyKey: string | symbol) {
        const metadata = ensureFederationMetadata(target);
        metadata.external_fields.push(String(propertyKey));
    };
}

export function Requires(fieldName: string): PropertyDecorator {
    return function (target: any, propertyKey: string | symbol) {
        const metadata = ensureFederationMetadata(target);

        // Validate field exists
        if (!target.hasOwnProperty(fieldName)) {
            throw new Error(`Field '${fieldName}' not found`);
        }

        metadata.requires[String(propertyKey)] = fieldName;
    };
}

export function Provides(reference: string): PropertyDecorator {
    return function (target: any, propertyKey: string | symbol) {
        const metadata = ensureFederationMetadata(target);
        metadata.provides_data.push(reference);
    };
}

export function Type(): ClassDecorator {
    return function <T extends { new(...args: any[]): {} }>(constructor: T) {
        ensureFederationMetadata(constructor.prototype);
        return constructor;
    };
}
```

### Part 5: Schema JSON Generation

**File**: `fraiseql-typescript/src/schema.ts` (modifications)

```typescript
export interface FederationTypeMetadata {
    name: string;
    keys: Array<{ fields: string[] }>;
    extend: boolean;
    external_fields: string[];
    shareable: boolean;
}

export function generateSchemaJson(types: any[]): any {
    const schemaJson = {
        types: [],
        federation: {
            enabled: false,
            version: 'v2'
        }
    };

    for (const type of types) {
        const metadata = type.prototype.__fraiseqlFederation__;
        const typeJson: any = {
            name: type.name,
            kind: 'OBJECT',
            fields: [],
            federation: {
                keys: metadata?.keys || [],
                extend: metadata?.extend || false,
                external_fields: metadata?.external_fields || [],
                provides_data: metadata?.provides_data || [],
                shareable: false,
            }
        };

        // Mark schema as federated if any type has federation metadata
        if (metadata && (metadata.keys.length > 0 || metadata.extend)) {
            schemaJson.federation.enabled = true;
        }

        // Add fields from type
        const descriptor = Object.getOwnPropertyDescriptors(type.prototype);
        for (const [key, desc] of Object.entries(descriptor)) {
            if (key !== 'constructor' && typeof desc.value !== 'function') {
                typeJson.fields.push({
                    name: key,
                    type: 'String', // TODO: Get actual type
                    federation: {
                        external: metadata?.external_fields.includes(key) || false,
                        requires: metadata?.requires[key] || null,
                    }
                });
            }
        }

        schemaJson.types.push(typeJson);
    }

    return schemaJson;
}
```

### Part 6: Compilation with Validation

**File**: `crates/fraiseql-cli/src/schema/federation_validator.rs` (new)

```rust
pub struct FederationValidator;

impl FederationValidator {
    /// Validate federation metadata at compile time
    pub fn validate(schema: &serde_json::Value) -> Result<(), String> {
        let federation = schema.get("federation")
            .ok_or_else(|| "No federation metadata".to_string())?;

        let types = schema.get("types")
            .and_then(|t| t.as_array())
            .ok_or_else(|| "No types in schema".to_string())?;

        // Validate each type
        for type_obj in types {
            let type_name = type_obj.get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| "Type missing name".to_string())?;

            let fed_meta = type_obj.get("federation")
                .ok_or_else(|| format!("Type {} missing federation metadata", type_name))?;

            // Validate keys reference existing fields
            if let Some(keys) = fed_meta.get("keys").and_then(|k| k.as_array()) {
                for key in keys {
                    let fields = key.get("fields")
                        .and_then(|f| f.as_array())
                        .ok_or_else(|| format!("Key in {} missing fields", type_name))?;

                    for field in fields {
                        let field_name = field.as_str()
                            .ok_or_else(|| "Key field not string".to_string())?;

                        // Check field exists in type
                        let field_exists = type_obj.get("fields")
                            .and_then(|tf| tf.as_array())
                            .map(|tf| tf.iter().any(|f| {
                                f.get("name").and_then(|n| n.as_str()) == Some(field_name)
                            }))
                            .unwrap_or(false);

                        if !field_exists {
                            return Err(format!(
                                "Key field '{}' not found in type {}",
                                field_name, type_name
                            ));
                        }
                    }
                }
            }

            // Validate external fields are in @extends types
            if let Some(external) = fed_meta.get("external_fields").and_then(|e| e.as_array()) {
                let is_extends = fed_meta.get("extend")
                    .and_then(|e| e.as_bool())
                    .unwrap_or(false);

                if !external.is_empty() && !is_extends {
                    return Err(format!(
                        "Type {} has external fields but is not @extends",
                        type_name
                    ));
                }
            }
        }

        Ok(())
    }
}
```

---

## Compilation & Testing

```bash
# Python tests
cd fraiseql-python
pytest tests/test_federation_decorators.py -v

# Expected output
test_key_single_field ... ok
test_key_multiple_fields ... ok
test_key_nonexistent_field ... ok
test_extends_marks_type ... ok
test_external_field_in_extends ... ok
test_external_on_non_extended_type ... ok
# ... etc (all passing)

# TypeScript tests
cd fraiseql-typescript
npm test

# Expected: All tests pass

# Verify schema JSON generation
python -c "
from fraiseql import Schema, type, key, extends, external

@type
@key('id')
class User:
    id: str
    email: str

schema = Schema(types=[User])
json_data = schema.to_json()
print(json.dumps(json_data, indent=2))
"
```

---

## Implementation Checklist

### Python Implementation
- [ ] `FederationMetadata` dataclass created
- [ ] `@key` decorator works with single and multiple keys
- [ ] `@extends` decorator marks extended types
- [ ] `@external()` marks external fields
- [ ] `@requires()` marks field dependencies
- [ ] `@provides()` marks provided data
- [ ] Decorators validate field existence
- [ ] Schema JSON includes federation metadata

### TypeScript Implementation
- [ ] `@Key` decorator works
- [ ] `@Extends` decorator works
- [ ] `@External` decorator works
- [ ] `@Requires` decorator works
- [ ] `@Provides` decorator works
- [ ] `@Type` decorator works
- [ ] TypeScript API mirrors Python
- [ ] Schema JSON generation identical

### Validation
- [ ] Key field validation works
- [ ] External field validation works
- [ ] Compile-time checks implemented
- [ ] Clear error messages
- [ ] All tests passing (30+ tests)

---

## Files Modified/Created

### Created
- `fraiseql-python/src/fraiseql/federation.py`
- `fraiseql-typescript/src/federation.ts`
- `crates/fraiseql-cli/src/schema/federation_validator.rs`

### Modified
- `fraiseql-python/src/fraiseql/registry.py`
- `fraiseql-python/src/fraiseql/schema.py`
- `fraiseql-typescript/src/schema.ts`

---

**Status**: [~] In Progress (Implementing decorators)
**Next**: REFACTOR Phase - Improve decorator API design
