# FraiseQL v2 Compiler Design: Rich Type Filters

## Architecture Overview

The FraiseQL v2 compiler is responsible for transforming authoring-time schema definitions into a complete, static, compiled schema that the runtime can execute without any code generation.

```
┌──────────────────────────────────────┐
│ Authoring Phase (Python)             │
├──────────────────────────────────────┤
│ @fraise_type                         │
│ class User:                          │
│   id: int                            │
│   email: EmailAddress    ← Rich type  │
│   vin: VIN              ← Rich type   │
│   created_at: DateTime               │
└────────────┬─────────────────────────┘
             │
             ├─ schema.json (generated from Python)
             │  - Type definitions
             │  - Field mappings
             │
             └─ fraiseql.toml (developer-written)
                - Validation rules
                - Security config
                - Database setup
                       ↓
┌──────────────────────────────────────────┐
│ Compilation Phase (Rust CLI)             │
├──────────────────────────────────────────┤
│ fraiseql-cli compile schema.json \      │
│              fraiseql.toml                │
│                                          │
│ For each rich type in schema:           │
│  1. Look up operators (from Rust)       │
│  2. Look up validation rules (from TOML)│
│  3. Load SQL handlers (from 4 databases)│
│  4. Generate GraphQL WhereInput type    │
│  5. Embed SQL templates                 │
│  6. Embed validation rules              │
│                                          │
│ Output: schema.compiled.json            │
└────────────┬─────────────────────────────┘
             │
             └─ schema.compiled.json
                - GraphQL types (static)
                - SQL templates (all 4 DBs)
                - Validation rules (embedded)
                       ↓
┌──────────────────────────────────────────┐
│ Runtime Phase (Rust Server)              │
├──────────────────────────────────────────┤
│ Server loads schema.compiled.json       │
│                                          │
│ For each GraphQL query:                 │
│  1. Extract parameters                  │
│  2. Validate params (rules from schema) │
│  3. Generate SQL (templates from schema)│
│  4. Execute on database                 │
│                                          │
│ No code generation                      │
│ No reflection                           │
│ Just static artifact loading            │
└──────────────────────────────────────────┘
```

## Single Source of Truth: The Rust Enum

Everything flows from `ExtendedOperator` in Rust:

```rust
// crates/fraiseql-core/src/filters/operators.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExtendedOperator {
    // Email operators
    EmailDomainEq(String),           ← Defines the operator
    EmailDomainIn(Vec<String>),      ← Name, parameter types
    EmailDomainEndswith(String),     ← All known at compile time
    EmailLocalPartStartswith(String),

    // VIN operators
    VinWmiEq(String),
    VinManufacturerEq(String),

    // IBAN operators
    IbanCountryEq(String),
    IbanCheckDigitEq(String),

    // ... 38 more operators
}
```

This enum is the **only place** operators are defined. Everything else is generated from it.

## Compiler Algorithm

### Input

1. **schema.json** (from Python authoring)

   ```json
   {
     "types": [
       {
         "name": "User",
         "fields": [
           {"name": "email", "type": "EmailAddress"},
           {"name": "vin", "type": "VIN"}
         ]
       }
     ]
   }
   ```

2. **fraiseql.toml** (developer configuration)

   ```toml
   [fraiseql.validation]
   email_domain_eq = { pattern = "^[a-z0-9]..." }
   vin_wmi_eq = { length = 3, pattern = "^[A-Z0-9]{3}$" }
   ```

### Processing

```rust
// Pseudocode for the compiler

fn compile(schema: Schema, config: Config) -> CompiledSchema {
    let mut compiled = CompiledSchema::new();

    // Build type mapping: TypeName → ExtendedOperator
    let type_operators = build_type_operator_mapping();
    // Result: EmailAddress → [EmailDomainEq, EmailDomainIn, ...]
    //         VIN → [VinWmiEq, VinManufacturerEq, ...]

    for rich_type in schema.get_rich_types() {
        // Rich type = EmailAddress, VIN, IBAN, etc.
        // Find all operators for this type
        let operators = type_operators[rich_type];

        // Build GraphQL WhereInput type
        let where_input = build_where_input(rich_type, operators);
        compiled.graphql[&format!("{}WhereInput", rich_type)] = where_input;

        // For each operator, embed SQL templates
        for op in operators {
            let validation_rule = config.validation[&op_name(op)];

            for database in [Postgres, MySQL, SQLite, SQLServer] {
                let sql_template = get_sql_template(database, op);
                compiled.operators[&op][database] = sql_template;
                compiled.validation[&op] = validation_rule;
            }
        }
    }

    compiled
}

fn build_where_input(rich_type: &str, operators: Vec<ExtendedOperator>) -> GraphQLType {
    // Input EmailAddressWhereInput {
    //   eq: String
    //   domainEq: String         ← from EmailDomainEq operator
    //   domainIn: [String!]!     ← from EmailDomainIn operator
    //   domainEndswith: String   ← from EmailDomainEndswith operator
    //   localPartStartswith: String ← from EmailLocalPartStartswith operator
    // }

    let mut fields = base_fields();  // eq, neq, in, nin, contains, isnull

    for op in operators {
        // Convert ExtendedOperator to GraphQL field
        // EmailDomainEq → domainEq: String
        // VinWmiEq → wmiEq: String
        fields.push(operator_to_graphql_field(op));
    }

    GraphQLInputType {
        name: format!("{}WhereInput", rich_type),
        fields,
    }
}

fn get_sql_template(database: Database, op: ExtendedOperator) -> String {
    // This calls the database-specific handler
    // which returns a template with $param and $field placeholders
    match database {
        Postgres => postgres::get_sql_template(op),
        MySQL => mysql::get_sql_template(op),
        SQLite => sqlite::get_sql_template(op),
        SQLServer => sqlserver::get_sql_template(op),
    }
}
```

### Output: schema.compiled.json

```json
{
  "types": [...],  // From original schema.json
  "graphql": {
    "UserWhereInput": {
      "fields": {
        "email": {
          "type": "EmailAddressWhereInput"
        }
      }
    },
    "EmailAddressWhereInput": {
      "fields": {
        "eq": {"type": "String"},
        "neq": {"type": "String"},
        "in": {"type": "[String!]!"},
        "nin": {"type": "[String!]!"},
        "contains": {"type": "String"},
        "domainEq": {
          "type": "String",
          "description": "Email domain (e.g., 'example.com')"
        },
        "domainIn": {
          "type": "[String!]!",
          "description": "Email domain in list"
        },
        "domainEndswith": {
          "type": "String",
          "description": "Email domain ends with suffix"
        },
        "localPartStartswith": {
          "type": "String",
          "description": "Local part starts with prefix"
        }
      }
    },
    "VINWhereInput": {
      "fields": {
        "eq": {"type": "String"},
        "wmiEq": {
          "type": "String",
          "description": "VIN World Manufacturer Identifier"
        }
      }
    }
  },
  "operators": {
    "emailDomainEq": {
      "postgres": "SPLIT_PART($field, '@', 2) = $param",
      "mysql": "SUBSTRING_INDEX($field, '@', -1) = ?",
      "sqlite": "SUBSTR($field, INSTR($field, '@') + 1) = ?",
      "sqlserver": "SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) = @param"
    },
    "emailDomainIn": {
      "postgres": "SPLIT_PART($field, '@', 2) IN ($params)",
      "mysql": "SUBSTRING_INDEX($field, '@', -1) IN ($params)",
      "sqlite": "SUBSTR($field, INSTR($field, '@') + 1) IN ($params)",
      "sqlserver": "SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) IN ($params)"
    },
    "vinWmiEq": {
      "postgres": "SUBSTRING($field FROM 1 FOR 3) = $param",
      "mysql": "SUBSTRING($field, 1, 3) = ?",
      "sqlite": "SUBSTR($field, 1, 3) = ?",
      "sqlserver": "SUBSTRING($field, 1, 3) = @param"
    }
  },
  "validation": {
    "emailDomainEq": {
      "pattern": "^[a-z0-9]([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$"
    },
    "emailDomainIn": {
      "pattern": "^[a-z0-9]([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$"
    },
    "emailDomainEndswith": {
      "pattern": "^\\.([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$"
    },
    "emailLocalPartStartswith": {
      "min_length": 1,
      "max_length": 64
    },
    "vinWmiEq": {
      "length": 3,
      "pattern": "^[A-Z0-9]{3}$"
    }
  }
}
```

## Runtime: Query Execution

```rust
async fn execute_query(
    query: GraphQLQuery,
    schema: CompiledSchema,
    db: DatabaseConnection,
) -> Result<QueryResult> {
    // 1. Extract where clause parameters from query
    let params = extract_where_params(&query)?;

    // 2. For each parameter, validate against compiled rules
    for (operator_name, value) in params {
        if let Some(rule) = schema.validation.get(&operator_name) {
            // Apply validation (patterns, lengths, checksums, etc.)
            rule.validate(&value)?;  // ← Validation happens HERE in Rust
        }
    }

    // 3. All parameters are now guaranteed valid
    // Generate SQL from templates
    let sql = generate_sql_from_templates(&query, &schema)?;

    // 4. Execute on database
    let result = db.execute_query(&sql).await?;

    Ok(result)
}
```

**Key points**:

- ✅ Validation happens **before** SQL generation
- ✅ Validation happens in **Rust layer**, not database
- ✅ All 4 databases validate the **same way**
- ✅ Invalid parameters never reach the database
- ✅ Clear, application-controlled error messages

## Operator Mapping: Rich Type → Operators

The compiler needs to know which operators apply to which types. This is defined in Rust:

```rust
// In filters module, we need a mapping function

fn get_operators_for_type(rich_type: &str) -> Vec<&'static ExtendedOperator> {
    match rich_type {
        "EmailAddress" => vec![
            &ExtendedOperator::EmailDomainEq,
            &ExtendedOperator::EmailDomainIn,
            &ExtendedOperator::EmailDomainEndswith,
            &ExtendedOperator::EmailLocalPartStartswith,
        ],
        "VIN" => vec![
            &ExtendedOperator::VinWmiEq,
            &ExtendedOperator::VinManufacturerEq,
        ],
        "IBAN" => vec![
            &ExtendedOperator::IbanCountryEq,
            &ExtendedOperator::IbanCheckDigitEq,
        ],
        // ... etc for all 44 types
    }
}
```

This is the **bridge** between:

- Type name in schema.json (string: "EmailAddress")
- Operator enums in Rust (value: ExtendedOperator::EmailDomainEq)

## SQL Template Generation

Each database implements a function that returns SQL templates:

```rust
// PostgreSQL example
impl ExtendedOperatorHandler for PostgresWhereGenerator {
    fn get_sql_template(operator: &ExtendedOperator) -> String {
        match operator {
            EmailDomainEq(_) => {
                "SPLIT_PART($field, '@', 2) = $param".to_string()
            }
            EmailDomainIn(_) => {
                "SPLIT_PART($field, '@', 2) IN ($params)".to_string()
            }
            VinWmiEq(_) => {
                "SUBSTRING($field FROM 1 FOR 3) = $param".to_string()
            }
            // ... etc
        }
    }
}
```

At compile time, these templates are extracted and embedded in schema.compiled.json with placeholders:

- `$field` - the database field reference
- `$param` - parameter placeholder (?, $1, @p1, etc.)
- `$params` - multiple parameter placeholders (?, ?, ?), etc.

At runtime, placeholders are substituted when building the actual SQL.

## Validation Rules: From Default to Custom

1. **Default rules** (`filters/default_rules.rs`)
   - 70+ rules shipped with FraiseQL
   - Patterns, lengths, checksums, ranges
   - Cover all 44+ operators

2. **TOML override** (`fraiseql.toml`)

   ```toml
   [fraiseql.validation]
   # Override a default rule
   email_domain_eq = { pattern = "^custom\\.pattern\\..*" }

   # Or keep the default
   # (not specified means use default_rules.rs)
   ```

3. **Compiler merges** them

   ```
   default_rules[op] + toml_overrides[op] → compiled_rules[op]
   ```

4. **Embedded in schema.compiled.json**

   ```json
   "validation": {
     "emailDomainEq": { "pattern": "..." }
   }
   ```

5. **Applied at runtime**

   ```rust
   rule.validate(value)?;
   ```

## Benefits of This Architecture

✅ **No Duplication**: Operators defined once in Rust, everything flows from there
✅ **Single Source of Truth**: ExtendedOperator enum is the authoritative list
✅ **Deterministic**: Same inputs → same compiled schema every time
✅ **Static Artifact**: schema.compiled.json is complete and self-contained
✅ **No Runtime Generation**: Server just loads JSON, no code generation
✅ **Fast Validation**: Happens in Rust before touching database
✅ **Database Agnostic**: All 4 databases validate the same way
✅ **Configuration Driven**: Developer controls behavior via TOML
✅ **v2 Philosophy**: Clear separation of Authoring → Compilation → Runtime

## Implementation Phases

### Phase 1: Current

- ✅ ExtendedOperator enum defined
- ✅ Validation framework built (ValidationRule, ChecksumType)
- ✅ Default rules created (70+ for all operators)
- ✅ SQL generation patterns established (6 operators, all 4 DBs)

### Phase 2: Next (Compiler)

- [ ] Build fraiseql-cli compile command
- [ ] Parse schema.json
- [ ] Parse fraiseql.toml
- [ ] Build type→operators mapping
- [ ] Generate GraphQL WhereInput types
- [ ] Extract SQL templates from database handlers
- [ ] Embed validation rules from config
- [ ] Output schema.compiled.json

### Phase 3: Runtime Integration

- [ ] Load schema.compiled.json at server startup
- [ ] Wire validation into query execution path
- [ ] Use embedded SQL templates
- [ ] Apply validation rules before SQL generation

### Phase 4: Complete SQL Generation

- [ ] Implement remaining 38 operators across all 4 databases
- [ ] Test all operators end-to-end
- [ ] Document operator semantics

## Files Involved

**Current (exist)**:

- `crates/fraiseql-core/src/filters/operators.rs` - ExtendedOperator enum (single source of truth)
- `crates/fraiseql-core/src/filters/validators.rs` - ValidationRule framework
- `crates/fraiseql-core/src/filters/default_rules.rs` - 70+ default validation rules
- `crates/fraiseql-core/src/db/{postgres,mysql,sqlite,sqlserver}/where_generator.rs` - SQL templates

**Future (to build)**:

- `crates/fraiseql-cli/src/compile.rs` - Main compiler logic
- `crates/fraiseql-cli/src/rich_filters.rs` - Rich filter compilation
- `crates/fraiseql-cli/src/graphql_gen.rs` - GraphQL type generation
- Tests for compiler pipeline

## No Python Filter Classes

In v1, we manually wrote GraphQL filter classes in Python. In v2:

- ❌ No Python filter classes
- ❌ No manual GraphQL type definitions
- ✅ Compiler generates everything from Rust operators
- ✅ Python is pure authoring (just type annotations)

The compiler ensures:

1. One authoritative definition (Rust ExtendedOperator)
2. All artifacts generated consistently
3. No duplication or drift
4. Clean separation of concerns
