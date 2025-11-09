# PostgreSQL Comment Format Convention

**Purpose**: Define the standard format for PostgreSQL comments that support both human-readable descriptions and machine-readable @fraiseql annotations
**Status**: Specification (Ready for Implementation)
**Date**: 2025-11-08

---

## 🎯 Overview

PostgreSQL comments serve **dual purposes** in the FraiseQL ecosystem:

1. **Human-readable documentation** → GraphQL schema descriptions
2. **Machine-readable metadata** → FraiseQL configuration

Both can coexist in the same comment using a **description-first, annotation-second** format.

---

## 📋 Standard Comment Format

### **General Structure**

```
[Human-readable description]

@fraiseql:[type]
[YAML metadata]
```

**Rules:**
1. **Description first** (optional, but recommended)
   - Plain text description for humans
   - Becomes GraphQL schema description
   - Can be multi-line

2. **Blank line** (recommended for readability)

3. **@fraiseql annotation** (optional)
   - Starts with `@fraiseql:[type]` marker
   - Followed by YAML-formatted metadata
   - Used for FraiseQL configuration

---

## 📚 Comment Types

### **1. Composite Type Comments**

```sql
COMMENT ON TYPE common.type_simple_address IS
'Simple postal address without relational integrity.
Use for prototyping and embedded addresses without validation.

@fraiseql:composite
name: SimpleAddress
tier: 2
storage: jsonb
use_when: Prototyping, embedded addresses';
```

**Result:**
- **GraphQL description**: "Simple postal address without relational integrity. Use for prototyping and embedded addresses without validation."
- **Metadata**: `name="SimpleAddress", tier=2, storage="jsonb"`

---

### **2. Composite Type Attribute Comments**

```sql
COMMENT ON COLUMN common.type_simple_address.street IS
'Street address line 1 (required).
Must not be empty.

@fraiseql:field
name: street
type: String!
required: true
validation: Must not be empty';
```

**Result:**
- **GraphQL field description**: "Street address line 1 (required). Must not be empty."
- **Metadata**: `name="street", type="String!", required=true`

---

### **3. View Comments**

```sql
COMMENT ON VIEW common.v_public_address IS
'Authoritative address with government data integration and referential integrity.
Includes nested relationships to Country, PostalCode, and AdministrativeUnit.

@fraiseql:type
trinity: true
expose_fields:
  - id
  - streetName
  - country';
```

**Result:**
- **GraphQL type description**: "Authoritative address with government data integration..."
- **Metadata**: `trinity=true, expose_fields=[...]`

---

### **4. Function Comments**

```sql
COMMENT ON FUNCTION app.create_organizational_unit IS
'Creates a new organizational unit within the hierarchy.
Validates parent relationships and organizational level constraints.

@fraiseql:mutation
name: createOrganizationalUnit
input_type: app.type_organizational_unit_input
success_type: CreateOrganizationalUnitSuccess
failure_type: CreateOrganizationalUnitError';
```

**Result:**
- **GraphQL mutation description**: "Creates a new organizational unit within the hierarchy..."
- **Metadata**: `name="createOrganizationalUnit", input_type=..., success_type=..., failure_type=...`

---

## 🔄 Comment Parsing Flow

### **Step 1: Split at @fraiseql Marker**

```python
comment = """Simple postal address

@fraiseql:composite
name: SimpleAddress"""

# Split at marker
if '@fraiseql:' in comment:
    description_part = comment.split('@fraiseql:', 1)[0].strip()
    annotation_part = comment.split('@fraiseql:', 1)[1]
else:
    description_part = comment
    annotation_part = None
```

### **Step 2: Extract Description**

```python
description = description_part  # "Simple postal address"
```

### **Step 3: Parse Annotation**

```python
import yaml

annotation_type = annotation_part.split('\n', 1)[0].strip()  # "composite"
yaml_content = annotation_part.split('\n', 1)[1]
metadata = yaml.safe_load(yaml_content)  # {name: "SimpleAddress"}
```

---

## 🎯 Priority Hierarchy

### **For GraphQL Descriptions:**

```
1. Explicit description in @fraiseql annotation (if provided)
   ↓
2. Text before @fraiseql marker
   ↓
3. Full comment (if no @fraiseql annotation)
   ↓
4. Auto-generated fallback
```

**Examples:**

```sql
-- Case 1: Override via annotation
COMMENT ON VIEW v_user IS 'User profile
@fraiseql:type
description: Custom description override';
-- → Uses "Custom description override"

-- Case 2: Text before marker
COMMENT ON VIEW v_user IS 'User profile
@fraiseql:type
trinity: true';
-- → Uses "User profile"

-- Case 3: No annotation
COMMENT ON VIEW v_user IS 'User profile';
-- → Uses "User profile"

-- Case 4: No comment
-- (no COMMENT statement)
-- → Uses "Auto-generated from v_user"
```

---

## 📋 SpecQL Generation Templates

### **Template: Composite Type**

```sql
CREATE TYPE {schema}.type_{name}_input AS (
    {fields}
);

COMMENT ON TYPE {schema}.type_{name}_input IS
'{description}

@fraiseql:composite
name: {GraphQLName}
tier: {tier}
storage: {storage}
use_when: {use_cases}';
```

### **Template: Composite Type Attribute**

```sql
COMMENT ON COLUMN {schema}.type_{name}_input.{field} IS
'{field_description}

@fraiseql:field
name: {graphqlFieldName}
type: {graphqlType}
required: {true|false}
validation: {validation_rule}';
```

### **Template: Function**

```sql
COMMENT ON FUNCTION {schema}.{function_name} IS
'{function_description}

@fraiseql:mutation
name: {mutationName}
input_type: {schema}.type_{name}_input
success_type: {SuccessType}
failure_type: {FailureType}';
```

---

## ✅ Best Practices

### **1. Always Include Description**

```sql
-- ✅ GOOD: Clear description + metadata
COMMENT ON TYPE common.type_money_amount IS
'Monetary value with currency code.
Amount uses 2 decimal places for cents.

@fraiseql:composite
name: MoneyAmount
tier: 2';

-- ⚠️ ACCEPTABLE: Metadata only (no human description)
COMMENT ON TYPE common.type_money_amount IS
'@fraiseql:composite
name: MoneyAmount
tier: 2';
-- Works, but less helpful for humans reading schema
```

### **2. Multi-line Descriptions**

```sql
-- ✅ GOOD: Multi-paragraph description
COMMENT ON VIEW common.v_public_address IS
'Authoritative address with government data integration and referential integrity.

This view includes nested relationships to:
- Country (ISO 3166-1)
- PostalCode (validated)
- AdministrativeUnit (city/region hierarchy)

External integrations:
- BAN (Base Adresse Nationale, France)
- Google Maps API
- USPS address validation

@fraiseql:type
trinity: true';
```

### **3. Concise Field Descriptions**

```sql
-- ✅ GOOD: Concise but informative
COMMENT ON COLUMN common.type_simple_address.country_code IS
'ISO 3166-1 alpha-2 country code (US, FR, JP).

@fraiseql:field
name: countryCode
type: String!
required: true
validation: ^[A-Z]{2}$';

-- ❌ TOO VERBOSE: Redundant information
COMMENT ON COLUMN common.type_simple_address.country_code IS
'This field stores the country code for the address. It uses the ISO 3166-1 alpha-2 standard, which means it must be exactly 2 uppercase letters. For example, US for United States, FR for France, JP for Japan. This field is required and cannot be null...';
-- Too long, hard to maintain
```

### **4. Keep Metadata and Description Consistent**

```sql
-- ✅ GOOD: Description matches metadata
COMMENT ON TYPE common.type_date_range IS
'Date range with optional end date.
Use is_current flag for ongoing periods.

@fraiseql:composite
name: DateRange
tier: 2
use_when: Employment periods, subscriptions, active contracts';

-- ❌ INCONSISTENT: Description contradicts metadata
COMMENT ON TYPE common.type_date_range IS
'Simple date type with start and end.

@fraiseql:composite
name: DateRange
tier: 2
use_when: Historical data only';
-- Description says "simple", metadata says "Historical data only" - confusing
```

---

## 🔧 Implementation Notes

### **FraiseQL Changes Required**

**1. Add description extraction utility:**
```python
# src/fraiseql/utils/comment_parser.py

def split_comment(comment: str | None) -> tuple[str | None, str | None]:
    """Split comment into description and annotation parts.

    Returns:
        (description, annotation_yaml) tuple
    """
    if not comment:
        return None, None

    if '@fraiseql:' in comment:
        parts = comment.split('@fraiseql:', 1)
        description = parts[0].strip() or None
        annotation = '@fraiseql:' + parts[1] if parts[1] else None
        return description, annotation

    return comment.strip(), None
```

**2. Update generators to use split comments:**
```python
# In TypeGenerator, InputGenerator, MutationGenerator

# Extract description and annotation separately
description, annotation_yaml = split_comment(metadata.comment)

# Use description for GraphQL
type_cls.__doc__ = description or fallback_description

# Parse annotation for configuration
annotation = metadata_parser.parse_annotation(annotation_yaml)
```

---

## 📊 Examples in Practice

### **Example 1: Reusable Composite Type**

```sql
-- SpecQL generates:
CREATE TYPE common.type_simple_address AS (
    street TEXT,
    city TEXT,
    postal_code TEXT,
    country_code TEXT
);

COMMENT ON TYPE common.type_simple_address IS
'Simple postal address without relational integrity.
Use for prototyping and embedded addresses without validation.

@fraiseql:composite
name: SimpleAddress
tier: 2
storage: jsonb
use_when: Prototyping, embedded addresses
examples:
  - Event venue addresses
  - Shipping labels
  - Non-validated user input';

COMMENT ON COLUMN common.type_simple_address.street IS
'Street address line 1 (required).

@fraiseql:field
name: street
type: String!
required: true';

COMMENT ON COLUMN common.type_simple_address.city IS
'City name (required).

@fraiseql:field
name: city
type: String!
required: true';

COMMENT ON COLUMN common.type_simple_address.country_code IS
'ISO 3166-1 alpha-2 country code (e.g., US, FR, JP).

@fraiseql:field
name: countryCode
type: String!
required: true
validation: ^[A-Z]{2}$';
```

**FraiseQL generates:**
```graphql
"""
Simple postal address without relational integrity.
Use for prototyping and embedded addresses without validation.
"""
input SimpleAddressInput {
  """Street address line 1 (required)."""
  street: String!

  """City name (required)."""
  city: String!

  """ISO 3166-1 alpha-2 country code (e.g., US, FR, JP)."""
  countryCode: String!
}
```

---

### **Example 2: Function with Metadata**

```sql
-- SpecQL generates:
CREATE FUNCTION app.create_organizational_unit(
    input_tenant_id UUID,
    input_user_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result;

COMMENT ON FUNCTION app.create_organizational_unit IS
'Creates a new organizational unit within the hierarchy.
Validates parent relationships and organizational level constraints.

@fraiseql:mutation
name: createOrganizationalUnit
input_type: app.type_organizational_unit_input
success_type: CreateOrganizationalUnitSuccess
failure_type: CreateOrganizationalUnitError';
```

**FraiseQL generates:**
```graphql
type Mutation {
  """
  Creates a new organizational unit within the hierarchy.
  Validates parent relationships and organizational level constraints.
  """
  createOrganizationalUnit(
    input: CreateOrganizationalUnitInput!
  ): CreateOrganizationalUnitPayload!
}
```

---

## 🎯 Summary

**The comment format is designed for coexistence:**

1. ✅ **Human-readable descriptions** come first
2. ✅ **@fraiseql annotations** follow (optional)
3. ✅ **FraiseQL splits comments** and uses both parts
4. ✅ **No conflict** - Each serves different purpose

**SpecQL should generate both** in every comment for maximum clarity and functionality.

---

**Status**: Specification complete
**Next Step**: Implement comment splitting in FraiseQL (small enhancement)
**Estimated effort**: 1-2 hours (add utility function + update generators)
