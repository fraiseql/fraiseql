# Phase 5: Extended/Rich Scalar Type Operators

## Objective
Implement operators for all 44 rich scalar types using existing SQL templates from sql_templates.rs.

## Success Criteria
- [ ] All 44 rich scalar types supported with their operators
- [ ] ~228 SQL templates applied across 4 databases
- [ ] 44 types × 4 databases = 176+ test cases passing
- [ ] Integration tests on actual data (email addresses, VINs, IBANs, coordinates, etc.)
- [ ] `cargo clippy -p fraiseql-core` clean
- [ ] `cargo test -p fraiseql-core` passes

## Rich Scalar Types to Implement
```
Email, PhoneNumber, URL, Domain, Hostname,
Country, CountryCode, Currency, Timezone, Locale, Language,
VIN, IBAN, CUSIP, ISIN, LEI, SEDOL,
Coordinates, Location, PostalCode,
Slug, Version, SemVer, UUID,
IPv4Address, IPv6Address, MacAddress,
CreditCard, EAN, ISBN, SSN,
DateRange, LTree, MacAddress
```

## TDD Cycles

### Cycle 1: Route All Extended Operators to Templates

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test expecting `WhereOperator::Extended(op)` to produce SQL from templates
- **GREEN**: Add routing in `operator_to_sql()`:
  ```rust
  WhereOperator::Extended(extended_op) => {
      let operator_name = extended_op.template_name();
      Self::apply_template(db_type, operator_name, field_sql, value)
  }
  ```
- **REFACTOR**: Implement `template_name()` for all ExtendedOperator variants
- **CLEANUP**: Verify all operators map correctly, commit

### Cycle 2: Handle Lookup Data Support

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test for operators using lookup data (e.g., continent lists, currency info)
- **GREEN**: Implement parameter array support in `apply_template()`
- **REFACTOR**: Ensure $params placeholder is correctly expanded
- **CLEANUP**: Test with various lookup data, commit

### Cycle 3: Comprehensive Testing

**File**: `crates/fraiseql-core/tests/operators_extended.rs` (new file)

- **RED**: Write matrix of 176+ test cases (44 types × 4 databases)
- **GREEN**: Verify each produces valid database-specific SQL
- **REFACTOR**: Add integration tests with actual data for each type
- **CLEANUP**: All tests pass, commit

## Notable Type Groups

### Email Operators
```
domainEq, domainIn, domainEndswith, localPartStartswith
```

### Coordinate Operators
```
distanceWithin, withinBoundingBox, withinPolygon
```

### Financial Operators
```
Country-specific validation, currency formatting
```

### Identifier Operators
```
VIN component extraction, IBAN validation, checksum verification
```

## Dependencies
- Requires Phase 0 (Template Integration) ✓
- Requires templates in sql_templates.rs (already exist)
- Independent of Phases 2-4, 6-9

## Status
[ ] Not Started
