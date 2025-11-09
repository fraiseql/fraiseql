# SpecQL ↔ FraiseQL: Responsibilities & Boundaries

**Purpose**: Define clear separation of concerns between SpecQL (database code generator) and FraiseQL (GraphQL framework)
**Status**: Living Document
**Last Updated**: 2025-11-08

---

## 🎯 Executive Summary

**SpecQL** generates PostgreSQL schema (tables, types, functions)
**FraiseQL** introspects PostgreSQL and generates GraphQL API

The boundary is simple: **SpecQL writes to PostgreSQL, FraiseQL reads from PostgreSQL.**

---

## 🏗️ Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                        DEVELOPER                                 │
│                            ↓                                     │
│                    SpecQL YAML Schemas                           │
└──────────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────────┐
│                         SpecQL                                   │
│                   (Code Generator)                               │
│                                                                  │
│  Generates:                                                      │
│  • Tables (tb_*)                                                 │
│  • Composite Types (type_*_input)                               │
│  • Functions (app.*, core.*)                                    │
│  • Views (v_*)                                                   │
│  • Comments (@fraiseql:* annotations)                           │
└──────────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────────┐
│                      PostgreSQL                                  │
│                   (Source of Truth)                              │
│                                                                  │
│  Contains:                                                       │
│  • Schema (tables, types, functions, views)                     │
│  • Comments (metadata for FraiseQL)                             │
│  • Constraints (validation, referential integrity)              │
└──────────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────────┐
│                      FraiseQL                                    │
│                  (GraphQL Framework)                             │
│                                                                  │
│  Introspects PostgreSQL:                                         │
│  • Reads tables/views → GraphQL types                           │
│  • Reads functions → GraphQL mutations                          │
│  • Reads composite types → GraphQL inputs                       │
│  • Reads comments → GraphQL descriptions                        │
│                                                                  │
│  Generates:                                                      │
│  • GraphQL schema                                                │
│  • Python classes (@fraiseql.type, @fraiseql.mutation)          │
│  • TypeScript types (optional)                                  │
└──────────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────────┐
│                     GraphQL API                                  │
│                  (Runtime Execution)                             │
└──────────────────────────────────────────────────────────────────┘
```

---

## 📋 Responsibility Matrix

### SpecQL Responsibilities (Database Code Generation)

| Task | SpecQL Generates | Example |
|------|------------------|---------|
| **Tables** | `CREATE TABLE` DDL | `tb_organizational_unit`, `tb_machine` |
| **Composite Types** | `CREATE TYPE` DDL | `app.type_organizational_unit_input` |
| **Functions** | `CREATE FUNCTION` DDL | `app.create_organizational_unit(...)` |
| **Views** | `CREATE VIEW` DDL | `v_organizational_unit`, `tv_machine` |
| **Constraints** | `CHECK`, `UNIQUE`, `FK` | Email validation, unique identifiers |
| **Indexes** | `CREATE INDEX` | Performance optimization |
| **Comments** | `COMMENT ON ...` | `@fraiseql:mutation`, `@fraiseql:field` |
| **Domains** | `CREATE DOMAIN` (optional) | Rich scalar types (email, phone, money) |
| **Schemas** | `CREATE SCHEMA` | `app`, `core`, `common`, `catalog` |
| **Migrations** | Schema versioning | ALTER TABLE, data migrations |

**Key Principle**: SpecQL owns **all DDL operations** (CREATE, ALTER, DROP)

---

### FraiseQL Responsibilities (GraphQL API Generation)

| Task | FraiseQL Does | Example |
|------|---------------|---------|
| **Introspection** | Read PostgreSQL metadata | Query `pg_type`, `pg_class`, `pg_proc` |
| **Type Generation** | Python classes from views | `@fraiseql.type` from `v_*` views |
| **Input Generation** | Python classes from composite types | GraphQL input from `type_*_input` |
| **Mutation Generation** | Python decorators from functions | `@fraiseql.mutation` from `app.*` functions |
| **Query Generation** | GraphQL queries from views | `users()`, `machines()` queries |
| **Schema Export** | GraphQL SDL generation | `.graphql` schema files |
| **Context Injection** | Runtime parameter mapping | `context["tenant_id"]` → `input_pk_organization` |
| **Validation** | Runtime input validation | GraphQL type checking |
| **Execution** | Query/mutation execution | Call PostgreSQL functions, return JSONB |

**Key Principle**: FraiseQL owns **all introspection and runtime execution** (never writes to PostgreSQL schema)

---

## 🔄 The Three-Tier Type System

### Tier 1: Rich Scalar Types

**Definition**: Validated primitives with business semantics (email, phoneNumber, money, coordinates)

| Responsibility | SpecQL | FraiseQL |
|----------------|--------|----------|
| **PostgreSQL Storage** | ✅ CREATE DOMAIN (optional) or CHECK constraints | ❌ |
| **Validation Logic** | ✅ In database (CHECK constraints, triggers) | ⚠️ Can add GraphQL-level validation |
| **GraphQL Type Mapping** | ❌ | ✅ Map to GraphQL scalars |
| **Type Safety** | ✅ At database level | ✅ At GraphQL level |

**Example Flow:**

```yaml
# SpecQL YAML (hypothetical)
scalar_types:
  email:
    base_type: TEXT
    validation: "~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Z|a-z]{2,}$'"
```

↓ **SpecQL generates:**

```sql
-- Option A: Domain type
CREATE DOMAIN email AS TEXT
CHECK (VALUE ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$');

-- Option B: CHECK constraint (current approach)
CREATE TABLE tb_user (
    email TEXT CHECK (email ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$')
);
```

↓ **FraiseQL introspects:**

```python
# Auto-generated
@fraiseql.type(sql_source="v_user")
class User:
    id: UUID
    email: str  # FraiseQL could map to EmailAddress scalar type
```

**Decision: SpecQL creates PostgreSQL domains (Tier 1), FraiseQL maps them to GraphQL scalars**

---

### Tier 2: Composite Types (Reusable Structures)

**Definition**: Structured, reusable business concepts (SimpleAddress, MoneyAmount, DateRange, Contact)

| Responsibility | SpecQL | FraiseQL |
|----------------|--------|----------|
| **Composite Type Definition** | ✅ CREATE TYPE | ❌ |
| **Reusable Type Library** | ✅ Generate standard types | ❌ |
| **Usage in Functions** | ✅ Function parameters | ❌ |
| **GraphQL Input Generation** | ❌ | ✅ Introspect → GraphQL input |
| **GraphQL Type Generation** | ❌ | ✅ Generate classes |

**Example Flow:**

```yaml
# SpecQL YAML (hypothetical - reusable type library)
composite_types:
  SimpleAddress:
    fields:
      street: text!
      city: text!
      postal_code: text
      country_code: text!
    storage: jsonb  # Store as JSONB column, not dedicated table
```

↓ **SpecQL generates:**

```sql
-- Reusable composite type
CREATE TYPE common.type_simple_address AS (
    street TEXT,
    city TEXT,
    postal_code TEXT,
    country_code TEXT
);

COMMENT ON TYPE common.type_simple_address IS
'@fraiseql:composite name=SimpleAddress';

-- Used in multiple entities
CREATE TABLE tb_organization (
    id UUID PRIMARY KEY,
    headquarters JSONB  -- Stores SimpleAddress structure
);

-- Function accepts this type
CREATE FUNCTION app.update_headquarters(
    input_pk_organization UUID,
    input_address common.type_simple_address
) RETURNS app.mutation_result;
```

↓ **FraiseQL introspects:**

```python
# Auto-generated from composite type
@fraiseql.input
class SimpleAddressInput:
    street: str
    city: str
    postal_code: str | None
    country_code: str

# Used in mutations
@fraiseql.mutation(function="update_headquarters", schema="app")
class UpdateHeadquarters:
    input: SimpleAddressInput  # FraiseQL auto-detects composite type
    success: Organization
    failure: OrganizationError
```

**Current Status:**
- ✅ **SpecQL creates entity-specific composite types** (`type_organizational_unit_input`)
- ⚠️ **SpecQL should create reusable composite types** (`type_simple_address`, `type_money_amount`)
- ✅ **FraiseQL Phase 5 will introspect composite types**

**Decision: SpecQL should add a reusable composite type library (SimpleAddress, MoneyAmount, DateRange, Contact)**

---

### Tier 3: Entity Types (Full Entities with Relationships)

**Definition**: Full entities with dedicated tables, foreign keys, and relationships (PublicAddress, User, Product)

| Responsibility | SpecQL | FraiseQL |
|----------------|--------|----------|
| **Table Schema** | ✅ CREATE TABLE with FKs | ❌ |
| **Relationships** | ✅ Foreign keys | ❌ |
| **Views** | ✅ JSONB views with nested data | ❌ |
| **GraphQL Type** | ❌ | ✅ From views |
| **GraphQL Relationships** | ❌ | ✅ Nested resolvers |
| **External API Integration** | ⚠️ PL/pgSQL functions calling APIs? | ❌ |

**Example Flow:**

```yaml
# SpecQL YAML (entity definition)
entities:
  PublicAddress:
    table: common.tb_public_address
    fields:
      id: uuid
      country: ref(Country)  # FK relationship
      postal_code: ref(PostalCode)  # FK relationship
      street_name: text
      latitude: numeric
      longitude: numeric
    external_validation:
      ban_api: true  # French BAN integration
```

↓ **SpecQL generates:**

```sql
-- Table with FK relationships
CREATE TABLE common.tb_public_address (
    id INTEGER PRIMARY KEY,
    pk_public_address UUID DEFAULT gen_random_uuid(),
    fk_country INTEGER REFERENCES catalog.tb_country(pk_country),
    fk_postal_code UUID REFERENCES common.tb_postal_code(pk_postal_code),
    street_name TEXT,
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    ban_identifier TEXT  -- External API reference
);

-- View with nested relationships
CREATE VIEW common.v_public_address AS
SELECT
    id,
    jsonb_build_object(
        'id', pk_public_address,
        'streetName', street_name,
        'country', (SELECT jsonb_build_object('code', code, 'name', name)
                    FROM catalog.tb_country WHERE pk_country = fk_country),
        'postalCode', (SELECT code FROM common.tb_postal_code WHERE pk_postal_code = fk_postal_code),
        'latitude', latitude,
        'longitude', longitude
    ) AS data
FROM common.tb_public_address;

-- External API validation function (optional)
CREATE FUNCTION app.validate_address_with_ban(
    input_street TEXT,
    input_postal_code TEXT,
    input_city TEXT
) RETURNS JSONB AS $$
    -- Call external BAN API
    -- Return validation result
$$ LANGUAGE plpgsql;
```

↓ **FraiseQL introspects:**

```python
# Auto-generated from view
@fraiseql.type(sql_source="v_public_address")
class PublicAddress:
    id: UUID
    street_name: str
    country: Country  # Nested type from JSONB
    postal_code: str
    latitude: float | None
    longitude: float | None

# Mutation using entity
@fraiseql.mutation(function="create_public_address", schema="app")
class CreatePublicAddress:
    input: CreatePublicAddressInput
    success: PublicAddress
    failure: AddressError
```

**Decision: SpecQL handles all Tier 3 entity schema generation, FraiseQL introspects and generates GraphQL types**

---

## 📊 Feature Comparison

### Rich Type Features

| Feature | SpecQL Generates | FraiseQL Introspects |
|---------|------------------|----------------------|
| **Email validation** | ✅ CREATE DOMAIN or CHECK | ✅ Map to GraphQL scalar |
| **Phone number validation** | ✅ CREATE DOMAIN or CHECK | ✅ Map to GraphQL scalar |
| **Money type (amount + currency)** | ✅ Composite type | ✅ GraphQL input type |
| **Date ranges** | ✅ Composite type or RANGE type | ✅ GraphQL input type |
| **SimpleAddress** | ⚠️ **Should add** | ✅ Will introspect when added |
| **Contact** | ⚠️ **Should add** | ✅ Will introspect when added |
| **GeoLocation (lat/lng)** | ✅ Can use PostGIS or composite | ✅ GraphQL type |
| **Dimension (L×W×H)** | ⚠️ **Should add** | ✅ Will introspect when added |
| **RecurrenceRule** | ⚠️ **Should add** (iCal format) | ✅ Will introspect when added |

---

### Enterprise Patterns

| Feature | SpecQL Generates | FraiseQL Introspects |
|---------|------------------|----------------------|
| **PublicAddress with FK hierarchy** | ✅ Tables + FKs + views | ✅ GraphQL type with nested data |
| **Multi-level geo hierarchy** | ✅ Tables (Country, Region, City) | ✅ GraphQL nested types |
| **External API integration (BAN)** | ⚠️ PL/pgSQL functions? | ❌ Runtime only |
| **Address autocomplete** | ⚠️ PL/pgSQL API calls? | ⚠️ GraphQL resolver? |
| **Geocoding** | ⚠️ PL/pgSQL API calls? | ⚠️ GraphQL resolver? |

**Decision Points:**
- **External API calls**: Should SpecQL generate PL/pgSQL functions that call external APIs? Or should FraiseQL handle this at runtime?
- **Recommendation**: SpecQL provides functions, FraiseQL exposes via GraphQL

---

## 🚀 Reusable Composite Type Library (Tier 2)

### Proposed Standard Types (SpecQL Should Generate)

#### 1. SimpleAddress
```sql
CREATE TYPE common.type_simple_address AS (
    street TEXT,
    street2 TEXT,
    city TEXT,
    state TEXT,
    postal_code TEXT,
    country_code TEXT,
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION
);

COMMENT ON TYPE common.type_simple_address IS
'@fraiseql:composite
name: SimpleAddress
description: Simple postal address without relational integrity
tier: 2
use_when: Prototyping, embedded addresses without validation';
```

**Usage Pattern:**
```sql
-- Store as JSONB column
ALTER TABLE tb_organization ADD COLUMN billing_address JSONB;

-- Function accepts structured input
CREATE FUNCTION app.update_billing_address(
    input_pk_organization UUID,
    input_address common.type_simple_address
) ...;
```

---

#### 2. MoneyAmount
```sql
CREATE TYPE common.type_money_amount AS (
    amount NUMERIC(15, 2),
    currency TEXT  -- ISO 4217 (USD, EUR, JPY)
);

COMMENT ON TYPE common.type_money_amount IS
'@fraiseql:composite
name: MoneyAmount
description: Monetary value with currency
tier: 2
validation: currency ~ ''^[A-Z]{3}$''';
```

---

#### 3. DateRange
```sql
CREATE TYPE common.type_date_range AS (
    start_date DATE,
    end_date DATE,
    is_current BOOLEAN
);

COMMENT ON TYPE common.type_date_range IS
'@fraiseql:composite
name: DateRange
description: Date range with optional current flag
tier: 2
validation: end_date IS NULL OR end_date >= start_date';
```

---

#### 4. Contact
```sql
CREATE TYPE common.type_contact AS (
    name TEXT,
    email TEXT,
    phone TEXT,
    company TEXT,
    job_title TEXT,
    preferred_contact_method TEXT  -- 'email', 'phone', 'text'
);

COMMENT ON TYPE common.type_contact IS
'@fraiseql:composite
name: Contact
description: Contact information (embedded, no FK relationships)
tier: 2
use_when: Emergency contact, non-user contact info';
```

---

#### 5. GeoLocation
```sql
CREATE TYPE common.type_geo_location AS (
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    accuracy_meters DOUBLE PRECISION
);

COMMENT ON TYPE common.type_geo_location IS
'@fraiseql:composite
name: GeoLocation
description: Geographic coordinates with accuracy
tier: 2
validation: latitude BETWEEN -90 AND 90, longitude BETWEEN -180 AND 180';
```

---

#### 6. Dimension
```sql
CREATE TYPE common.type_dimension AS (
    length NUMERIC,
    width NUMERIC,
    height NUMERIC,
    unit TEXT  -- 'mm', 'cm', 'm', 'in', 'ft'
);

COMMENT ON TYPE common.type_dimension IS
'@fraiseql:composite
name: Dimension
description: Physical dimensions with unit
tier: 2
validation: unit IN (''mm'', ''cm'', ''m'', ''in'', ''ft'')';
```

---

#### 7. RecurrenceRule
```sql
CREATE TYPE common.type_recurrence_rule AS (
    frequency TEXT,  -- 'DAILY', 'WEEKLY', 'MONTHLY', 'YEARLY'
    interval INTEGER,
    count INTEGER,
    until_date DATE,
    by_day TEXT[],  -- ['MO', 'WE', 'FR']
    rrule TEXT  -- Full iCalendar RRULE format
);

COMMENT ON TYPE common.type_recurrence_rule IS
'@fraiseql:composite
name: RecurrenceRule
description: Recurring event pattern (iCalendar RRULE)
tier: 2
validation: frequency IN (''DAILY'', ''WEEKLY'', ''MONTHLY'', ''YEARLY'')';
```

---

#### 8. FileAttachment
```sql
CREATE TYPE common.type_file_attachment AS (
    filename TEXT,
    mime_type TEXT,
    size_bytes BIGINT,
    storage_key TEXT,  -- S3/storage reference
    uploaded_at TIMESTAMPTZ
);

COMMENT ON TYPE common.type_file_attachment IS
'@fraiseql:composite
name: FileAttachment
description: File metadata without dedicated storage table
tier: 2
use_when: Embedded file references';
```

---

#### 9. SocialProfile
```sql
CREATE TYPE common.type_social_profile AS (
    platform TEXT,  -- 'linkedin', 'twitter', 'github', 'facebook'
    profile_url TEXT,
    handle TEXT  -- @username
);

COMMENT ON TYPE common.type_social_profile IS
'@fraiseql:composite
name: SocialProfile
description: Social media profile link
tier: 2';
```

---

#### 10. AuditInfo
```sql
CREATE TYPE common.type_audit_info AS (
    created_at TIMESTAMPTZ,
    created_by UUID,
    updated_at TIMESTAMPTZ,
    updated_by UUID,
    deleted_at TIMESTAMPTZ,
    deleted_by UUID
);

COMMENT ON TYPE common.type_audit_info IS
'@fraiseql:composite
name: AuditInfo
description: Audit trail metadata (alternative to Trinity pattern)
tier: 2
use_when: Simple entities without Trinity pattern';
```

---

### Metadata Convention for Composite Types

SpecQL should add these metadata fields to composite type comments:

```sql
COMMENT ON TYPE common.type_* IS
'@fraiseql:composite
name: GraphQLTypeName
description: Human-readable description
tier: 1|2|3
storage: jsonb|composite|table
use_when: When to use this type vs alternatives
validation: Additional validation rules
examples: Usage examples';
```

This metadata allows FraiseQL to:
1. Generate accurate GraphQL input types
2. Add descriptions to GraphQL schema
3. Document best practices
4. Provide migration guidance

---

## 🔄 Smart Type Promotion (Future)

**Definition**: Automatic migration from Tier 2 (composite types) → Tier 3 (entity tables)

**Example**: Start with `SimpleAddress` (JSONB), promote to `PublicAddress` (dedicated table with FKs)

| Responsibility | SpecQL | FraiseQL |
|----------------|--------|----------|
| **Detection heuristics** | ❌ | ✅ Analyze usage patterns |
| **Suggest promotion** | ❌ | ✅ CLI warning |
| **Migration generation** | ✅ Generate ALTER TABLE, data migration | ❌ |
| **Migration execution** | ✅ Apply migration | ❌ |

**Not required for now** - Focus on Phase 5 first, validate Tier 2 composite types work well.

---

## ✅ Current Status & Next Steps

### ✅ What Already Works

1. **SpecQL generates:**
   - ✅ Entity-specific composite types (`type_organizational_unit_input`)
   - ✅ Functions with JSONB parameters
   - ✅ Standard mutation return type (`app.mutation_result`)
   - ✅ Context parameter pattern (`input_pk_organization`, `input_created_by`)

2. **FraiseQL introspects:**
   - ✅ Views → GraphQL types
   - ✅ Functions → GraphQL mutations (parameter-based)
   - ✅ Comments → GraphQL descriptions

### ⚠️ What Needs Implementation

1. **SpecQL should add** (Tier 2 library):
   - ⚠️ Reusable composite types (`type_simple_address`, `type_money_amount`, etc.)
   - ⚠️ Metadata comments on composite types (`@fraiseql:composite`)
   - ⚠️ Field-level comments on composite type attributes (`@fraiseql:field`)

2. **FraiseQL needs** (Phase 5):
   - ⚠️ Composite type introspection (`discover_composite_type()`)
   - ⚠️ Input generation from composite types (not function parameters)
   - ⚠️ Context parameter auto-detection (`input_pk_*` → `context_params`)
   - ⚠️ Field metadata parsing (`@fraiseql:field`)

3. **Optional future enhancements**:
   - ⏸️ Rich scalar types (Tier 1 - PostgreSQL domains)
   - ⏸️ Smart type promotion detection (Tier 2 → 3)
   - ⏸️ External API integration patterns

---

## 📝 Action Items

### For SpecQL Team

1. **Priority 1: Add Reusable Composite Type Library**
   - [ ] Create `type_simple_address`
   - [ ] Create `type_money_amount`
   - [ ] Create `type_date_range`
   - [ ] Create `type_contact`
   - [ ] Create `type_geo_location`
   - [ ] Create `type_dimension`
   - [ ] Create `type_recurrence_rule`
   - [ ] Create `type_file_attachment`
   - [ ] Create `type_social_profile`
   - [ ] Create `type_audit_info`

2. **Priority 2: Add Composite Type Metadata**
   - [ ] Add `@fraiseql:composite` comments on types
   - [ ] Add `@fraiseql:field` comments on attributes
   - [ ] Document tier classification (1, 2, 3)
   - [ ] Document usage guidelines

3. **Priority 3: Rich Scalar Types (Optional)**
   - [ ] Consider PostgreSQL DOMAIN types for email, phone, money
   - [ ] Decide on validation strategy (CHECK vs DOMAIN)
   - [ ] Document scalar type conventions

### For FraiseQL Team

1. **Priority 1: Complete Phase 5** (Composite Type Introspection)
   - [ ] Implement `discover_composite_type()`
   - [ ] Implement composite type-based input generation
   - [ ] Implement context parameter auto-detection
   - [ ] Write unit tests
   - [ ] Write integration tests with PrintOptim schema
   - [ ] Document usage

2. **Priority 2: Validate with Real Schema**
   - [ ] Test AutoFraiseQL against PrintOptim database
   - [ ] Verify all mutations auto-generate correctly
   - [ ] Document any edge cases

3. **Priority 3: Document Patterns**
   - [ ] Update AutoFraiseQL documentation
   - [ ] Add examples with composite types
   - [ ] Document Tier 2 vs Tier 3 guidelines

---

## 🎯 Success Criteria

### Phase 5 Complete

- [ ] FraiseQL can introspect `app.type_organizational_unit_input`
- [ ] FraiseQL generates `CreateOrganizationalUnitInput` GraphQL input
- [ ] FraiseQL auto-detects `context_params` from function signature
- [ ] All PrintOptim mutations auto-generate from SpecQL schema
- [ ] Zero manual code required for mutations

### Tier 2 Library Complete

- [ ] SpecQL generates 10 standard reusable composite types
- [ ] FraiseQL introspects and generates GraphQL inputs for all
- [ ] Documentation explains when to use Tier 2 vs Tier 3
- [ ] Example applications use reusable types

---

## 📚 References

- **FraiseQL Phase 5 Implementation Plan**: `/home/lionel/code/fraiseql/docs/implementation-plans/PHASE_5_COMPOSITE_TYPE_INPUT_GENERATION.md`
- **Rich Type System Design**: `/tmp/fraiseql_rich_type_system_design_extended.md`
- **PrintOptim Composite Types**: `/home/lionel/code/printoptim_backend/db/0_schema/00_common/004_input_types/`
- **FraiseQL Composite Type Issue**: `/home/lionel/code/fraiseql/docs/issues/SPECQL_COMPOSITE_TYPE_REQUIREMENT.md`

---

## 🤝 Collaboration Pattern

**Golden Rule**: PostgreSQL is the contract between SpecQL and FraiseQL.

```
SpecQL writes schema → PostgreSQL stores schema → FraiseQL reads schema
```

**When in doubt:**
- If it's a **database concern** (tables, types, functions, validation) → SpecQL
- If it's a **GraphQL concern** (types, inputs, queries, mutations) → FraiseQL
- If it's **runtime logic** (authentication, authorization, external APIs) → Application layer

---

**Status**: Ready for implementation
**Next Step**: FraiseQL Phase 5 (Composite Type Introspection)
