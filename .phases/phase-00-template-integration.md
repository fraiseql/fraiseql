# Phase 0: Template Integration

## Objective
Wire sql_templates.rs into WhereSqlGenerator so all template-based operators work across all 4 databases (Postgres, MySQL, SQLite, SQL Server).

## Success Criteria
- [ ] DatabaseType enum added to where_sql_generator.rs
- [ ] WhereSqlGenerator methods updated to accept db_type parameter
- [ ] Template lookup bridge implemented (apply_template function)
- [ ] All template-based operators route through sql_templates.rs
- [ ] 150+ operators work on all 4 databases
- [ ] Integration tests pass for template operators on all databases
- [ ] No performance regression

## TDD Cycles

### Cycle 1: Add Database Awareness to WhereSqlGenerator

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test that calls WhereSqlGenerator with DatabaseType parameter
  ```rust
  #[test]
  fn test_where_generator_accepts_database_type() {
      let clause = WhereClause::Field {
          path: vec!["email".to_string()],
          operator: WhereOperator::Extended(ExtendedOperator::EmailDomainEq("example.com".to_string())),
          value: json!("example.com"),
      };
      
      // Should accept DatabaseType parameter
      let sql_pg = WhereSqlGenerator::to_sql(&clause, DatabaseType::Postgres)?;
      let sql_mysql = WhereSqlGenerator::to_sql(&clause, DatabaseType::MySQL)?;
      
      // Different databases should produce different SQL
      assert_ne!(sql_pg, sql_mysql);
  }
  ```

- **GREEN**: Add DatabaseType enum and update signatures
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum DatabaseType {
      Postgres,
      MySQL,
      SQLite,
      SQLServer,
  }
  
  impl WhereSqlGenerator {
      pub fn to_sql(clause: &WhereClause, db_type: DatabaseType) -> Result<String> {
          match clause {
              WhereClause::Field { path, operator, value } => {
                  Self::generate_field_predicate(path, operator, value, db_type)
              }
              // ... other cases pass db_type down
          }
      }
      
      fn generate_field_predicate(
          path: &[String],
          operator: &WhereOperator,
          value: &Value,
          db_type: DatabaseType,
      ) -> Result<String> {
          let json_path = Self::build_json_path(path);
          // ... rest of implementation
      }
  }
  ```

- **REFACTOR**: Ensure all recursive calls pass db_type
- **CLEANUP**: `cargo clippy -p fraiseql-core`, commit

---

### Cycle 2: Create Template Lookup Bridge

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Test that templates are looked up and applied
  ```rust
  #[test]
  fn test_template_lookup_and_substitution() {
      let sql = WhereSqlGenerator::apply_template(
          DatabaseType::Postgres,
          "domainEq",
          "data->>'email'",
          &json!("example.com"),
      )?;
      
      // Should contain SPLIT_PART (PostgreSQL function)
      assert!(sql.contains("SPLIT_PART"));
      
      // MySQL version should use SUBSTRING_INDEX
      let sql_mysql = WhereSqlGenerator::apply_template(
          DatabaseType::MySQL,
          "domainEq",
          "data->>'email'",
          &json!("example.com"),
      )?;
      assert!(sql_mysql.contains("SUBSTRING_INDEX"));
  }
  ```

- **GREEN**: Implement apply_template function
  ```rust
  fn apply_template(
      db_type: DatabaseType,
      operator_name: &str,
      field_path: &str,
      value: &Value,
  ) -> Result<String> {
      use crate::schema::sql_templates::extract_template_for_operator;
      
      let db_name = match db_type {
          DatabaseType::Postgres => "postgres",
          DatabaseType::MySQL => "mysql",
          DatabaseType::SQLite => "sqlite",
          DatabaseType::SQLServer => "sqlserver",
      };
      
      let template = extract_template_for_operator(db_name, operator_name)
          .ok_or_else(|| FraiseQLError::Validation {
              message: format!(
                  "No template for operator '{}' on {}",
                  operator_name, db_name
              ),
              path: None,
          })?;
      
      // Replace placeholders
      let mut sql = template.replace("$field", field_path);
      
      // Handle parameter placeholders per database
      sql = match db_type {
          DatabaseType::Postgres => sql,  // Keep $1, $2 style
          DatabaseType::MySQL | DatabaseType::SQLite | DatabaseType::SQLServer => {
              // Replace $1, $2, etc. with ?
              let mut result = sql.clone();
              for i in 1..=10 {
                  result = result.replace(&format!("${}", i), "?");
              }
              result
          }
      };
      
      Ok(sql)
  }
  ```

- **REFACTOR**: Handle edge cases (missing templates, null values)
- **CLEANUP**: Clippy, commit

---

### Cycle 3: Route Template Operators to Lookup

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Test that all template-based operators route correctly
  ```rust
  #[test]
  fn test_email_operators_use_templates() {
      let operators = vec![
          (ExtendedOperator::EmailDomainEq("example.com".to_string()), "domainEq"),
          (ExtendedOperator::EmailDomainIn(vec!["example.com".to_string()]), "domainIn"),
          (ExtendedOperator::EmailDomainEndswith(".edu".to_string()), "domainEndswith"),
      ];
      
      for (op, expected_template_name) in operators {
          let sql = WhereSqlGenerator::operator_to_sql(
              &WhereOperator::Extended(op),
              DatabaseType::Postgres,
          )?;
          
          // Should contain something from the template
          assert!(!sql.is_empty());
      }
  }
  ```

- **GREEN**: Add routing for extended operators
  ```rust
  fn operator_to_sql(operator: &WhereOperator, db_type: DatabaseType) -> Result<String> {
      match operator {
          // Existing operators (unchanged)
          WhereOperator::Eq => Ok("=".to_string()),
          // ... etc
          
          // NEW: Extended operators route to templates
          WhereOperator::Extended(op) => {
              match op {
                  ExtendedOperator::EmailDomainEq(_) => {
                      Self::apply_template(db_type, "domainEq", &json!(""), &json!(null))
                  }
                  ExtendedOperator::EmailDomainIn(_) => {
                      Self::apply_template(db_type, "domainIn", &json!(""), &json!(null))
                  }
                  // ... all 44 extended operators
                  _ => Err(FraiseQLError::Validation {
                      message: format!("Extended operator not implemented: {:?}", op),
                      path: None,
                  })
              }
          }
      }
  }
  ```

- **REFACTOR**: Ensure all 150+ template operators are routed
- **CLEANUP**: Clippy, commit

---

### Cycle 4: Test Template Integration

**File**: `crates/fraiseql-core/tests/where_template_integration.rs` (new file)

- **RED**: Write comprehensive template tests
  ```rust
  #[test]
  fn test_templates_work_on_all_databases() {
      let test_cases = vec![
          ("domainEq", vec!["postgres", "mysql", "sqlite", "sqlserver"]),
          ("isIPv4", vec!["postgres", "mysql", "sqlite", "sqlserver"]),
          ("ancestorOf", vec!["postgres"]),  // PostgreSQL only
      ];
      
      for (op_name, supported_dbs) in test_cases {
          for db in supported_dbs {
              let sql = extract_template_for_operator(db, op_name);
              assert!(sql.is_some(), "Template should exist for {} on {}", op_name, db);
          }
      }
  }
  
  #[test]
  fn test_parameter_substitution_per_database() {
      let template = "field = $1";
      
      let pg_sql = apply_template(Postgres, "test", "data", &json!("value"));
      assert_eq!(pg_sql, "data = $1");  // PostgreSQL keeps $1
      
      let mysql_sql = apply_template(MySQL, "test", "data", &json!("value"));
      assert_eq!(mysql_sql, "data = ?");  // MySQL uses ?
  }
  ```

- **GREEN**: Verify templates exist and are applied correctly
- **CLEANUP**: Run tests, clippy, commit

---

## Dependencies
- Requires understanding of sql_templates.rs structure
- Depends on WhereOperator and ExtendedOperator enums
- Must maintain backward compatibility with existing operators

## Blockers
- [ ] Verify sql_templates.rs has complete coverage (228 templates)
- [ ] Confirm extract_template_for_operator function is public
- [ ] Ensure no breaking changes to existing operator_to_sql API

## Status
[ ] Not Started | [ ] In Progress | [ ] Complete

## Notes
- This phase unlocks ~150 operators at once
- After this, all template-based operators work across all 4 databases
- Phases 1-5 can begin in parallel after this completes
- Highest ROI of any phase
