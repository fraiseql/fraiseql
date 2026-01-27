# Cycle 16-3: REFACTOR Phase - Authoring API Design

**Cycle**: 3 of 8
**Phase**: REFACTOR (Improve design without changing behavior)
**Duration**: ~2-3 days
**Focus**: Clean up decorator API, improve error messages, extract validation

---

## Refactoring Tasks

### Task 1: Centralize Metadata Management

Extract metadata handling into a separate module:

**File**: `fraiseql-python/src/fraiseql/federation/_metadata.py`

```python
from dataclasses import dataclass, field as dataclass_field
from typing import Dict, List, Optional


@dataclass
class KeyDirective:
    fields: List[str]
    resolvable: bool = True


@dataclass
class TypeFederation:
    """Centralized federation metadata for a type"""
    name: str
    keys: List[KeyDirective] = dataclass_field(default_factory=list)
    extend: bool = False
    external_fields: List[str] = dataclass_field(default_factory=list)
    requires: Dict[str, str] = dataclass_field(default_factory=dict)
    provides_data: List[str] = dataclass_field(default_factory=list)
    shareable: bool = False

    def to_dict(self) -> Dict:
        """Convert to dictionary for schema.json"""
        return {
            'name': self.name,
            'keys': [{'fields': k.fields, 'resolvable': k.resolvable} for k in self.keys],
            'extend': self.extend,
            'external_fields': self.external_fields,
            'provides_data': self.provides_data,
            'shareable': self.shareable,
        }

    def validate(self, fields: Dict[str, type]) -> List[str]:
        """Validate metadata against type fields. Returns list of errors."""
        errors = []

        # Validate key fields exist
        for key in self.keys:
            for field_name in key.fields:
                if field_name not in fields:
                    errors.append(f"Key field '{field_name}' not found")

        # Validate external fields are in extends types
        if self.external_fields and not self.extend:
            errors.append("@external fields only valid in @extends types")

        # Validate requires fields exist
        for field_name in self.requires.keys():
            if field_name not in fields:
                errors.append(f"Field '{field_name}' in @requires not found")

        return errors
```

### Task 2: Improve Decorator Error Messages

**File**: `fraiseql-python/src/fraiseql/federation/_decorators.py`

```python
class FederationValidationError(Exception):
    """Federation-specific validation error"""
    pass


def key(fields: str) -> Callable:
    """Decorator: Mark type with federation key

    Args:
        fields: Space-separated field names, e.g., "tenant_id id"

    Example:
        @type
        @key("id")
        class User:
            id: ID
    """
    def decorator(cls):
        try:
            metadata = _ensure_federation_metadata(cls)

            # Parse fields
            key_fields = fields.strip().split()
            if not key_fields:
                raise FederationValidationError(
                    f"@key requires field names, got empty string"
                )

            # Validate each field exists in type annotations
            if not hasattr(cls, '__annotations__'):
                raise FederationValidationError(
                    f"Type {cls.__name__} has no fields to use as keys"
                )

            missing_fields = set(key_fields) - set(cls.__annotations__.keys())
            if missing_fields:
                available = ", ".join(cls.__annotations__.keys())
                raise FederationValidationError(
                    f"Key field(s) {missing_fields} not found in {cls.__name__}.\n"
                    f"Available fields: {available}"
                )

            # Add key to metadata
            metadata.keys.append(KeyDirective(fields=key_fields))
            return cls

        except FederationValidationError:
            raise
        except Exception as e:
            raise FederationValidationError(
                f"Error processing @key decorator on {cls.__name__}: {str(e)}"
            ) from e

    return decorator
```

### Task 3: Add Type Hints Throughout

Improve type safety:

```python
from typing import Optional, Type, TypeVar, overload

T = TypeVar('T')

@overload
def key(fields: str) -> Callable[[Type[T]], Type[T]]: ...

@overload
def external() -> Any: ...

# Implement with better typing
def external() -> Any:
    """Mark field as external (owned by another subgraph)"""
    return _ExternalFieldMarker()


class _ExternalFieldMarker:
    """Internal marker for external fields"""
    def __repr__(self) -> str:
        return "external()"
```

### Task 4: Extract Validation Module

**File**: `fraiseql-python/src/fraiseql/federation/_validator.py`

```python
class SchemaValidator:
    """Validate federation schema at compile time"""

    @staticmethod
    def validate_type(cls, metadata: TypeFederation) -> List[str]:
        """Validate single type federation metadata"""
        errors = []

        # Get type fields
        fields = getattr(cls, '__annotations__', {})
        if not fields:
            errors.append(f"Type {cls.__name__} has no fields")
            return errors

        # Delegate to metadata validator
        type_errors = metadata.validate(fields)
        errors.extend(type_errors)

        return errors

    @staticmethod
    def validate_schema(types: List[Type]) -> Dict[str, List[str]]:
        """Validate complete schema federation setup"""
        errors: Dict[str, List[str]] = {}

        # First pass: validate individual types
        for cls in types:
            metadata = getattr(cls, '__fraiseql_federation__', None)
            if metadata:
                type_errors = SchemaValidator.validate_type(cls, metadata)
                if type_errors:
                    errors[cls.__name__] = type_errors

        # Second pass: validate cross-type relationships
        # (e.g., @extends types must have a base type)
        for cls in types:
            metadata = getattr(cls, '__fraiseql_federation__', None)
            if metadata and metadata.extend:
                # Find if there's a base type with same name
                base_exists = any(
                    c.__name__ == cls.__name__ and
                    not getattr(c, '__fraiseql_federation__', TypeFederation(cls.__name__)).extend
                    for c in types
                )
                if not base_exists:
                    if cls.__name__ not in errors:
                        errors[cls.__name__] = []
                    errors[cls.__name__].append(
                        f"@extends type {cls.__name__} extends non-existent base type"
                    )

        return errors
```

### Task 5: TypeScript API Consistency

Ensure TypeScript API mirrors Python exactly:

**File**: `fraiseql-typescript/src/federation/decorators.ts`

```typescript
/**
 * Decorator: Mark type with federation key
 * @param fields Space-separated field names
 *
 * @example
 * @Key("id")
 * @Type()
 * class User {
 *   id: string;
 *   email: string;
 * }
 */
export function Key(fields: string): ClassDecorator {
    return function <T extends { new(...args: any[]): {} }>(constructor: T) {
        try {
            const metadata = ensureFederationMetadata(constructor.prototype);

            // Parse fields
            const keyFields = fields.trim().split(/\s+/).filter(f => f.length > 0);
            if (keyFields.length === 0) {
                throw new Error(
                    '@Key requires field names, got empty string'
                );
            }

            // Validate fields
            const propertyDescriptors = Object.getOwnPropertyDescriptors(constructor.prototype);
            const invalidFields = keyFields.filter(f => !propertyDescriptors.hasOwnProperty(f));

            if (invalidFields.length > 0) {
                const available = Object.keys(propertyDescriptors).join(', ');
                throw new Error(
                    `Key field(s) ${invalidFields.join(', ')} not found in ${constructor.name}.\n` +
                    `Available fields: ${available}`
                );
            }

            metadata.keys.push({ fields: keyFields });
            return constructor;

        } catch (error) {
            throw new Error(
                `Error processing @Key decorator on ${constructor.name}: ${error.message}`
            );
        }
    };
}
```

### Task 6: Documentation Improvements

Add comprehensive docstrings:

```python
def extends(cls: Type[T]) -> Type[T]:
    """Decorator: Extend type defined in another subgraph

    Use @extends when a type is defined in multiple subgraphs.
    The primary definition (no @extends) owns the type.
    Extensions add new fields or query capabilities.

    Args:
        cls: The type class to mark as extended

    Returns:
        The modified type class

    Example:
        # In subgraph A (primary definition)
        @type
        @key("id")
        class User:
            id: ID
            email: str

        # In subgraph B (extension)
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: List[Order]  # New field

    Raises:
        TypeError: If @extends is used on a non-class

    See Also:
        external(): Mark individual fields as external
        key(): Define federation key
    """
    if not isinstance(cls, type):
        raise TypeError(f"@extends can only decorate classes, not {type(cls)}")

    metadata = _ensure_federation_metadata(cls)
    metadata.extend = True
    return cls
```

### Task 7: Schema JSON Validator

**File**: `crates/fraiseql-cli/src/schema/federation_validator.rs`

Improve error messages in Rust:

```rust
pub fn validate_federation_schema(schema: &serde_json::Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    let types = schema.get("types")
        .and_then(|t| t.as_array())
        .unwrap_or(&vec![]);

    for type_obj in types {
        let type_name = type_obj.get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Unknown");

        if let Some(fed) = type_obj.get("federation").and_then(|f| f.as_object()) {
            // Validate keys
            if let Some(keys) = fed.get("keys").and_then(|k| k.as_array()) {
                for (idx, key_obj) in keys.iter().enumerate() {
                    if let Some(fields) = key_obj.get("fields").and_then(|f| f.as_array()) {
                        for field in fields {
                            let field_name = field.as_str().unwrap_or("unknown");
                            let has_field = type_obj.get("fields")
                                .and_then(|f| f.as_array())
                                .map(|f| f.iter().any(|tf| {
                                    tf.get("name").and_then(|n| n.as_str()) == Some(field_name)
                                }))
                                .unwrap_or(false);

                            if !has_field {
                                errors.push(format!(
                                    "Type '{}' key #{}: field '{}' does not exist\n  \
                                    Available fields: {}",
                                    type_name,
                                    idx + 1,
                                    field_name,
                                    format_available_fields(type_obj)
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
```

---

## Refactoring Checklist

- [ ] Metadata management centralized
- [ ] Decorator error messages improved (with context)
- [ ] Type hints added throughout
- [ ] Validation extracted to separate module
- [ ] TypeScript API consistent with Python
- [ ] Documentation comprehensive (docstrings + examples)
- [ ] Rust validator improved with better errors
- [ ] All tests still passing (30+ tests)
- [ ] No clippy warnings

---

**Status**: [~] In Progress (Refactoring)
**Next**: CLEANUP Phase - Finalization
