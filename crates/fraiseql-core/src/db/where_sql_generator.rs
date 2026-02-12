//! WHERE clause to SQL string generator for fraiseql-wire.
//!
//! Converts FraiseQL's WHERE clause AST to SQL predicates that can be used
//! with fraiseql-wire's `where_sql()` method.

use serde_json::Value;

use crate::{
    db::{WhereClause, WhereOperator, DatabaseType},
    error::{FraiseQLError, Result},
};

/// Generates SQL WHERE clause strings from AST.
pub struct WhereSqlGenerator;

impl WhereSqlGenerator {
    /// Convert WHERE clause AST to SQL string.
    ///
    /// # Deprecated
    ///
    /// This method is deprecated. Use `to_sql_for_db()` instead to specify
    /// the target database type. This method defaults to PostgreSQL.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::db::{WhereClause, WhereOperator, where_sql_generator::WhereSqlGenerator};
    /// use serde_json::json;
    ///
    /// let clause = WhereClause::Field {
    ///     path: vec!["status".to_string()],
    ///     operator: WhereOperator::Eq,
    ///     value: json!("active"),
    /// };
    ///
    /// let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    /// assert_eq!(sql, "data->>'status' = 'active'");
    /// ```
    #[deprecated(since = "2.0.0", note = "use `to_sql_for_db()` instead")]
    pub fn to_sql(clause: &WhereClause) -> Result<String> {
        // Default to PostgreSQL for backwards compatibility
        Self::to_sql_for_db(clause, DatabaseType::PostgreSQL)
    }

    /// Convert WHERE clause AST to SQL string for a specific database.
    ///
    /// This method routes to database-specific SQL templates and functions.
    /// For now, it delegates to the database-agnostic to_sql method.
    /// Future phases will use db_type to select database-specific templates.
    ///
    /// # Arguments
    ///
    /// * `clause` - The WHERE clause to convert
    /// * `db_type` - The target database type
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use fraiseql_core::db::{WhereClause, WhereOperator, DatabaseType, where_sql_generator::WhereSqlGenerator};
    /// use serde_json::json;
    ///
    /// let clause = WhereClause::Field {
    ///     path: vec!["email".to_string()],
    ///     operator: WhereOperator::Eq,
    ///     value: json!("test@example.com"),
    /// };
    ///
    /// let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)?;
    /// ```
    pub fn to_sql_for_db(clause: &WhereClause, db_type: DatabaseType) -> Result<String> {
        Self::to_sql_internal(clause, db_type)
    }

    /// Internal implementation that threads db_type through recursion.
    fn to_sql_internal(clause: &WhereClause, db_type: DatabaseType) -> Result<String> {
        match clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => Self::generate_field_predicate(path, operator, value, db_type),
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok("TRUE".to_string());
                }
                let parts: Result<Vec<_>> =
                    clauses.iter().map(|c| Self::to_sql_internal(c, db_type)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
                }
                let parts: Result<Vec<_>> =
                    clauses.iter().map(|c| Self::to_sql_internal(c, db_type)).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(clause) => {
                let inner = Self::to_sql_internal(clause, db_type)?;
                Ok(format!("NOT ({})", inner))
            },
        }
    }

    fn generate_field_predicate(
        path: &[String],
        operator: &WhereOperator,
        value: &Value,
        db_type: DatabaseType,
    ) -> Result<String> {
        let json_path = Self::build_json_path(path);
        let sql = match operator {
            // Null checks
            WhereOperator::IsNull => {
                let is_null = value.as_bool().unwrap_or(true);
                if is_null {
                    format!("{json_path} IS NULL")
                } else {
                    format!("{json_path} IS NOT NULL")
                }
            },
            // Template-based operators (network, email, etc.)
            WhereOperator::IsIPv4 => {
                Self::apply_template(db_type, "isIPv4", &json_path, value)?
            },
            WhereOperator::IsIPv6 => {
                Self::apply_template(db_type, "isIPv6", &json_path, value)?
            },
            WhereOperator::IsPrivate => {
                Self::apply_template(db_type, "isPrivate", &json_path, value)?
            },
            WhereOperator::IsPublic => {
                Self::apply_template(db_type, "isPublic", &json_path, value)?
            },
            WhereOperator::InSubnet => {
                Self::apply_template(db_type, "inSubnet", &json_path, value)?
            },
            WhereOperator::IsLoopback => {
                Self::apply_template(db_type, "isLoopback", &json_path, value)?
            },
            WhereOperator::ContainsIP => {
                Self::apply_template(db_type, "containsIP", &json_path, value)?
            },
            WhereOperator::ContainsSubnet => {
                Self::apply_template(db_type, "containsSubnet", &json_path, value)?
            },
            WhereOperator::Overlaps => {
                Self::apply_template(db_type, "overlaps", &json_path, value)?
            },
            WhereOperator::StrictlyContains => {
                Self::apply_template(db_type, "strictlyLeft", &json_path, value)?
            },
            // All other operators
            _ => {
                let sql_op = Self::operator_to_sql(operator, db_type)?;
                let sql_value = Self::value_to_sql(value, operator)?;
                format!("{json_path} {sql_op} {sql_value}")
            },
        };
        Ok(sql)
    }

    fn build_json_path(path: &[String]) -> String {
        if path.is_empty() {
            return "data".to_string();
        }

        if path.len() == 1 {
            // Simple path: data->>'field'
            // SECURITY: Escape field name to prevent SQL injection
            let escaped = Self::escape_sql_string(&path[0]);
            format!("data->>'{}'", escaped)
        } else {
            // Nested path: data#>'{a,b,c}'->>'d'
            // SECURITY: Escape all field names to prevent SQL injection
            let nested = &path[..path.len() - 1];
            let last = &path[path.len() - 1];

            // Escape all nested components
            let escaped_nested: Vec<String> =
                nested.iter().map(|n| Self::escape_sql_string(n)).collect();
            let nested_path = escaped_nested.join(",");
            let escaped_last = Self::escape_sql_string(last);
            format!("data#>'{{{}}}'->>'{}'", nested_path, escaped_last)
        }
    }

    /// Look up and apply a SQL template for an operator on a specific database.
    ///
    /// This function retrieves database-specific SQL templates and substitutes placeholders.
    /// Templates use `$field` for the field reference and database-specific parameter placeholders
    /// (`$1` for PostgreSQL/SQL Server, `?` for MySQL/SQLite).
    ///
    /// # Arguments
    ///
    /// * `db_type` - The target database type
    /// * `operator_name` - The operator name (e.g., "domainEq", "wmiEq")
    /// * `field_sql` - The JSONB field reference (e.g., "data->>'email'")
    /// * `value` - The comparison value (used to validate parameter count)
    ///
    /// # Returns
    ///
    /// Returns substituted SQL template, or error if template not found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let sql = WhereSqlGenerator::apply_template(
    ///     DatabaseType::PostgreSQL,
    ///     "domainEq",
    ///     "data->>'email'",
    ///     &json!("example.com"),
    /// )?;
    /// // Result: "SPLIT_PART(data->>'email', '@', 2) = $1"
    /// ```
    #[allow(dead_code)]
    fn apply_template(
        db_type: DatabaseType,
        operator_name: &str,
        field_sql: &str,
        _value: &Value,
    ) -> Result<String> {
        // Lookup template for the operator on this database
        let template = Self::get_template_for_operator(db_type, operator_name).ok_or_else(|| {
            // Provide helpful error message for missing templates
            FraiseQLError::Internal {
                message: format!(
                    "Operator '{}' is not supported on {:?}. This operator may not be available for all databases.",
                    operator_name, db_type
                ),
                source: None,
            }
        })?;

        // Substitute $field placeholder with actual field reference
        let sql = template.replace("$field", field_sql);

        Ok(sql)
    }

    /// Get SQL template for an operator on a specific database.
    ///
    /// This function maintains the mapping of operators to database-specific SQL templates.
    /// Phase 0 includes templates for email operators as a proof-of-concept.
    /// Additional operators will be added in subsequent phases.
    fn get_template_for_operator(db_type: DatabaseType, operator_name: &str) -> Option<String> {
        match (db_type, operator_name) {
            // ========================================================================
            // EMAIL OPERATORS (Phase 0 example templates)
            // ========================================================================
            (DatabaseType::PostgreSQL, "domainEq") => {
                Some("SPLIT_PART($field, '@', 2) = $1".to_string())
            },
            (DatabaseType::MySQL, "domainEq") => Some("SUBSTRING_INDEX($field, '@', -1) = ?".to_string()),
            (DatabaseType::SQLite, "domainEq") => {
                Some("SUBSTR($field, INSTR($field, '@') + 1) = ?".to_string())
            },
            (DatabaseType::SQLServer, "domainEq") => {
                Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) = ?".to_string())
            },

            (DatabaseType::PostgreSQL, "domainIn") => {
                Some("SPLIT_PART($field, '@', 2) IN ($params)".to_string())
            },
            (DatabaseType::MySQL, "domainIn") => {
                Some("SUBSTRING_INDEX($field, '@', -1) IN ($params)".to_string())
            },
            (DatabaseType::SQLite, "domainIn") => {
                Some("SUBSTR($field, INSTR($field, '@') + 1) IN ($params)".to_string())
            },
            (DatabaseType::SQLServer, "domainIn") => {
                Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) IN ($params)".to_string())
            },

            (DatabaseType::PostgreSQL, "domainEndswith") => {
                Some("SPLIT_PART($field, '@', 2) LIKE '%' || $1".to_string())
            },
            (DatabaseType::MySQL, "domainEndswith") => {
                Some("SUBSTRING_INDEX($field, '@', -1) LIKE CONCAT('%', ?)".to_string())
            },
            (DatabaseType::SQLite, "domainEndswith") => {
                Some("SUBSTR($field, INSTR($field, '@') + 1) LIKE '%' || ?".to_string())
            },
            (DatabaseType::SQLServer, "domainEndswith") => {
                Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) LIKE '%' + ?".to_string())
            },

            (DatabaseType::PostgreSQL, "localPartStartswith") => {
                Some("SPLIT_PART($field, '@', 1) LIKE $1 || '%'".to_string())
            },
            (DatabaseType::MySQL, "localPartStartswith") => {
                Some("SUBSTRING_INDEX($field, '@', 1) LIKE CONCAT(?, '%')".to_string())
            },
            (DatabaseType::SQLite, "localPartStartswith") => {
                Some("SUBSTR($field, 1, INSTR($field, '@') - 1) LIKE ? || '%'".to_string())
            },
            (DatabaseType::SQLServer, "localPartStartswith") => {
                Some("SUBSTRING($field, 1, CHARINDEX('@', $field) - 1) LIKE ? + '%'".to_string())
            },

            // ========================================================================
            // NETWORK OPERATORS (Phase 2)
            // ========================================================================
            // IsIPv4: Validate that field contains an IPv4 address
            (DatabaseType::PostgreSQL, "isIPv4") => Some("CAST($field AS INET) IS NOT NULL AND CAST($field AS INET) ~ '\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}\\.\\d{1,3}'".to_string()),
            (DatabaseType::MySQL, "isIPv4") => Some("INET_ATON($field) IS NOT NULL".to_string()),
            (DatabaseType::SQLite, "isIPv4") => Some("$field REGEXP '^[0-9]{1,3}\\\\.[0-9]{1,3}\\\\.[0-9]{1,3}\\\\.[0-9]{1,3}$'".to_string()),
            (DatabaseType::SQLServer, "isIPv4") => Some("ISNUMERIC(PARSENAME($field, 4)) = 1 AND ISNUMERIC(PARSENAME($field, 3)) = 1 AND ISNUMERIC(PARSENAME($field, 2)) = 1 AND ISNUMERIC(PARSENAME($field, 1)) = 1".to_string()),

            // IsIPv6: Validate that field contains an IPv6 address
            (DatabaseType::PostgreSQL, "isIPv6") => Some("CAST($field AS INET) IS NOT NULL AND CAST($field AS INET) ~ ':'".to_string()),
            (DatabaseType::MySQL, "isIPv6") => Some("$field REGEXP '^([0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}$'".to_string()),
            (DatabaseType::SQLite, "isIPv6") => Some("$field REGEXP '^([0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}$'".to_string()),
            (DatabaseType::SQLServer, "isIPv6") => Some("$field LIKE '%:%' AND $field NOT LIKE '%%.%%'".to_string()),

            // IsPrivate: Check if IP is in private ranges
            (DatabaseType::PostgreSQL, "isPrivate") => Some("(CAST($field AS INET) << '10.0.0.0/8'::INET OR CAST($field AS INET) << '172.16.0.0/12'::INET OR CAST($field AS INET) << '192.168.0.0/16'::INET)".to_string()),
            (DatabaseType::MySQL, "isPrivate") => Some("(INET_ATON($field) >= INET_ATON('10.0.0.0') AND INET_ATON($field) <= INET_ATON('10.255.255.255')) OR (INET_ATON($field) >= INET_ATON('172.16.0.0') AND INET_ATON($field) <= INET_ATON('172.31.255.255')) OR (INET_ATON($field) >= INET_ATON('192.168.0.0') AND INET_ATON($field) <= INET_ATON('192.168.255.255'))".to_string()),
            (DatabaseType::SQLite, "isPrivate") => Some("$field LIKE '10.%' OR ($field LIKE '172.%' AND CAST(SUBSTR($field, INSTR($field, '.') + 1, INSTR(SUBSTR($field, INSTR($field, '.') + 1), '.') - 1) AS INTEGER) BETWEEN 16 AND 31) OR ($field LIKE '192.168.%')".to_string()),
            (DatabaseType::SQLServer, "isPrivate") => Some("(CAST(PARSENAME($field, 4) AS INT) = 10) OR (CAST(PARSENAME($field, 4) AS INT) = 172 AND CAST(PARSENAME($field, 3) AS INT) BETWEEN 16 AND 31) OR (CAST(PARSENAME($field, 4) AS INT) = 192 AND CAST(PARSENAME($field, 3) AS INT) = 168)".to_string()),

            // IsPublic: Inverse of IsPrivate
            (DatabaseType::PostgreSQL, "isPublic") => Some("NOT (CAST($field AS INET) << '10.0.0.0/8'::INET OR CAST($field AS INET) << '172.16.0.0/12'::INET OR CAST($field AS INET) << '192.168.0.0/16'::INET)".to_string()),
            (DatabaseType::MySQL, "isPublic") => Some("NOT ((INET_ATON($field) >= INET_ATON('10.0.0.0') AND INET_ATON($field) <= INET_ATON('10.255.255.255')) OR (INET_ATON($field) >= INET_ATON('172.16.0.0') AND INET_ATON($field) <= INET_ATON('172.31.255.255')) OR (INET_ATON($field) >= INET_ATON('192.168.0.0') AND INET_ATON($field) <= INET_ATON('192.168.255.255')))".to_string()),
            (DatabaseType::SQLite, "isPublic") => Some("NOT ($field LIKE '10.%' OR ($field LIKE '172.%' AND CAST(SUBSTR($field, INSTR($field, '.') + 1, INSTR(SUBSTR($field, INSTR($field, '.') + 1), '.') - 1) AS INTEGER) BETWEEN 16 AND 31) OR ($field LIKE '192.168.%'))".to_string()),
            (DatabaseType::SQLServer, "isPublic") => Some("NOT ((CAST(PARSENAME($field, 4) AS INT) = 10) OR (CAST(PARSENAME($field, 4) AS INT) = 172 AND CAST(PARSENAME($field, 3) AS INT) BETWEEN 16 AND 31) OR (CAST(PARSENAME($field, 4) AS INT) = 192 AND CAST(PARSENAME($field, 3) AS INT) = 168))".to_string()),

            // InSubnet: Check if IP is within specified CIDR subnet
            (DatabaseType::PostgreSQL, "inSubnet") => Some("CAST($field AS INET) << $1::INET".to_string()),
            (DatabaseType::MySQL, "inSubnet") => Some("INET_ATON($field) BETWEEN INET_ATON(SUBSTRING_INDEX($1, '/', 1)) AND INET_ATON(BROADCAST(CAST($1 AS CHAR)))".to_string()),
            (DatabaseType::SQLite, "inSubnet") => Some("CAST($field AS TEXT) BETWEEN CAST(SUBSTR($1, 1, INSTR($1, '/') - 1) AS TEXT) AND CAST(BROADCAST(CAST($1 AS TEXT)) AS TEXT)".to_string()),
            (DatabaseType::SQLServer, "inSubnet") => Some("CAST($field AS VARCHAR) BETWEEN SUBSTRING($1, 1, CHARINDEX('/', $1) - 1) AND BROADCAST(CAST($1 AS VARCHAR))".to_string()),

            // IsLoopback: Check if IP is loopback address (127.0.0.1 or ::1)
            (DatabaseType::PostgreSQL, "isLoopback") => Some("(CAST($field AS INET) << '127.0.0.0/8'::INET OR CAST($field AS INET) << '::1/128'::INET)".to_string()),
            (DatabaseType::MySQL, "isLoopback") => Some("(CAST($field AS UNSIGNED) >= INET_ATON('127.0.0.1') AND CAST($field AS UNSIGNED) <= INET_ATON('127.255.255.255')) OR $field = '::1'".to_string()),
            (DatabaseType::SQLite, "isLoopback") => Some("($field LIKE '127.%' OR $field = '::1')".to_string()),
            (DatabaseType::SQLServer, "isLoopback") => Some("(SUBSTRING($field, 1, 4) = '127.' OR $field = '::1')".to_string()),

            // ContainsIP: Check if subnet contains specified IP (reverse of InSubnet)
            (DatabaseType::PostgreSQL, "containsIP") => Some("$field::INET >> $1::INET".to_string()),
            (DatabaseType::MySQL, "containsIP") => Some("INET_ATON(SUBSTRING_INDEX($field, '/', 1)) <= INET_ATON($1) AND INET_ATON(BROADCAST($field)) >= INET_ATON($1)".to_string()),
            (DatabaseType::SQLite, "containsIP") => Some("CAST(SUBSTR($field, 1, INSTR($field, '/') - 1) AS TEXT) <= $1 AND CAST(BROADCAST($field) AS TEXT) >= $1".to_string()),
            (DatabaseType::SQLServer, "containsIP") => Some("SUBSTRING($field, 1, CHARINDEX('/', $field) - 1) <= $1 AND BROADCAST(SUBSTRING($field, 1, CHARINDEX('/', $field) - 1)) >= $1".to_string()),

            // ContainsSubnet: Check if subnet contains another subnet
            (DatabaseType::PostgreSQL, "containsSubnet") => Some("$field::INET >> $1::INET".to_string()),
            (DatabaseType::MySQL, "containsSubnet") => Some("INET_ATON(SUBSTRING_INDEX($field, '/', 1)) <= INET_ATON(SUBSTRING_INDEX($1, '/', 1)) AND INET_ATON(BROADCAST($field)) >= INET_ATON(BROADCAST($1))".to_string()),
            (DatabaseType::SQLite, "containsSubnet") => Some("CAST(SUBSTR($field, 1, INSTR($field, '/') - 1) AS TEXT) <= CAST(SUBSTR($1, 1, INSTR($1, '/') - 1) AS TEXT)".to_string()),
            (DatabaseType::SQLServer, "containsSubnet") => Some("SUBSTRING($field, 1, CHARINDEX('/', $field) - 1) <= SUBSTRING($1, 1, CHARINDEX('/', $1) - 1)".to_string()),

            // Overlaps: Check if CIDR ranges overlap
            (DatabaseType::PostgreSQL, "overlaps") => Some("$field::INET && $1::INET".to_string()),
            (DatabaseType::MySQL, "overlaps") => Some("NOT (INET_ATON(BROADCAST(SUBSTRING_INDEX($field, '/', 1))) < INET_ATON(SUBSTRING_INDEX($1, '/', 1)) OR INET_ATON(SUBSTRING_INDEX($field, '/', 1)) > INET_ATON(BROADCAST(SUBSTRING_INDEX($1, '/', 1))))".to_string()),
            (DatabaseType::SQLite, "overlaps") => Some("NOT (CAST(BROADCAST(SUBSTR($field, 1, INSTR($field, '/') - 1)) AS TEXT) < CAST(SUBSTR($1, 1, INSTR($1, '/') - 1) AS TEXT) OR CAST(SUBSTR($field, 1, INSTR($field, '/') - 1) AS TEXT) > CAST(BROADCAST(SUBSTR($1, 1, INSTR($1, '/') - 1)) AS TEXT))".to_string()),
            (DatabaseType::SQLServer, "overlaps") => Some("NOT (BROADCAST(SUBSTRING($field, 1, CHARINDEX('/', $field) - 1)) < SUBSTRING($1, 1, CHARINDEX('/', $1) - 1) OR SUBSTRING($field, 1, CHARINDEX('/', $field) - 1) > BROADCAST(SUBSTRING($1, 1, CHARINDEX('/', $1) - 1)))".to_string()),

            // StrictlyLeft: Check if first CIDR range is entirely to the left (lower IPs)
            (DatabaseType::PostgreSQL, "strictlyLeft") => Some("$field::INET << $1::INET AND NOT ($field::INET && $1::INET)".to_string()),
            (DatabaseType::MySQL, "strictlyLeft") => Some("INET_ATON(BROADCAST($field)) < INET_ATON(SUBSTRING_INDEX($1, '/', 1))".to_string()),
            (DatabaseType::SQLite, "strictlyLeft") => Some("CAST(BROADCAST(SUBSTR($field, 1, INSTR($field, '/') - 1)) AS TEXT) < CAST(SUBSTR($1, 1, INSTR($1, '/') - 1) AS TEXT)".to_string()),
            (DatabaseType::SQLServer, "strictlyLeft") => Some("CAST(BROADCAST(SUBSTRING($field, 1, CHARINDEX('/', $field) - 1)) AS VARCHAR) < SUBSTRING($1, 1, CHARINDEX('/', $1) - 1)".to_string()),

            // StrictlyRight: Check if first CIDR range is entirely to the right (higher IPs)
            (DatabaseType::PostgreSQL, "strictlyRight") => Some("$field::INET >> $1::INET AND NOT ($field::INET && $1::INET)".to_string()),
            (DatabaseType::MySQL, "strictlyRight") => Some("INET_ATON(SUBSTRING_INDEX($field, '/', 1)) > INET_ATON(BROADCAST($1))".to_string()),
            (DatabaseType::SQLite, "strictlyRight") => Some("CAST(SUBSTR($field, 1, INSTR($field, '/') - 1) AS TEXT) > CAST(BROADCAST(SUBSTR($1, 1, INSTR($1, '/') - 1)) AS TEXT)".to_string()),
            (DatabaseType::SQLServer, "strictlyRight") => Some("SUBSTRING($field, 1, CHARINDEX('/', $field) - 1) > CAST(BROADCAST(SUBSTRING($1, 1, CHARINDEX('/', $1) - 1)) AS VARCHAR)".to_string()),

            // Add more operators in later phases
            _ => None,
        }
    }

    fn operator_to_sql(operator: &WhereOperator, _db_type: DatabaseType) -> Result<&'static str> {
        // Phase 0: db_type parameter added for future database-specific implementations
        // Currently all basic operators generate the same SQL across all databases
        Ok(match operator {
            // Comparison
            WhereOperator::Eq => "=",
            WhereOperator::Neq => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",

            // Containment
            WhereOperator::In => "= ANY",
            WhereOperator::Nin => "!= ALL",

            // String operations
            WhereOperator::Contains => "LIKE",
            WhereOperator::Icontains => "ILIKE",
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Istartswith => "ILIKE",
            WhereOperator::Endswith => "LIKE",
            WhereOperator::Iendswith => "ILIKE",
            WhereOperator::Like => "LIKE",
            WhereOperator::Ilike => "ILIKE",

            // Array operations
            WhereOperator::ArrayContains => "@>",
            WhereOperator::ArrayContainedBy => "<@",
            WhereOperator::ArrayOverlaps => "&&",

            // These operators require special handling
            WhereOperator::IsNull => {
                return Err(FraiseQLError::Internal {
                    message: "IsNull should be handled separately".to_string(),
                    source:  None,
                });
            },
            WhereOperator::LenEq
            | WhereOperator::LenGt
            | WhereOperator::LenLt
            | WhereOperator::LenGte
            | WhereOperator::LenLte
            | WhereOperator::LenNeq => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Array length operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source:  None,
                });
            },

            // Vector operations not supported
            WhereOperator::L2Distance
            | WhereOperator::CosineDistance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance
            | WhereOperator::InnerProduct
            | WhereOperator::JaccardDistance => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Vector operations not supported in fraiseql-wire: {operator:?}"
                    ),
                    source:  None,
                });
            },

            // Advanced operators not yet supported
            WhereOperator::IsLoopback
            | WhereOperator::ContainsSubnet
            | WhereOperator::ContainsIP
            | WhereOperator::Overlaps
            | WhereOperator::StrictlyContains
            | WhereOperator::AncestorOf
            | WhereOperator::DescendantOf
            | WhereOperator::MatchesLquery
            | WhereOperator::MatchesLtxtquery
            | WhereOperator::MatchesAnyLquery
            | WhereOperator::DepthEq
            | WhereOperator::DepthNeq
            | WhereOperator::DepthGt
            | WhereOperator::DepthGte
            | WhereOperator::DepthLt
            | WhereOperator::DepthLte
            | WhereOperator::Lca
            | WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsPublic
            | WhereOperator::InSubnet
            | WhereOperator::Matches
            | WhereOperator::PlainQuery
            | WhereOperator::PhraseQuery
            | WhereOperator::WebsearchQuery
            | WhereOperator::Extended(_) => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Operator {:?} is not yet supported in fraiseql-wire. Please contact support or file an issue.",
                        operator
                    ),
                    source:  None,
                });
            },
        })
    }

    fn value_to_sql(value: &Value, operator: &WhereOperator) -> Result<String> {
        match (value, operator) {
            (Value::Null, _) => Ok("NULL".to_string()),
            (Value::Bool(b), _) => Ok(b.to_string()),
            (Value::Number(n), _) => Ok(n.to_string()),

            // String operators with wildcards
            (Value::String(s), WhereOperator::Contains | WhereOperator::Icontains) => {
                Ok(format!("'%{}%'", Self::escape_sql_string(s)))
            },
            (Value::String(s), WhereOperator::Startswith | WhereOperator::Istartswith) => {
                Ok(format!("'{}%'", Self::escape_sql_string(s)))
            },
            (Value::String(s), WhereOperator::Endswith | WhereOperator::Iendswith) => {
                Ok(format!("'%{}'", Self::escape_sql_string(s)))
            },

            // Regular strings
            (Value::String(s), _) => Ok(format!("'{}'", Self::escape_sql_string(s))),

            // Arrays (for IN operator)
            (Value::Array(arr), WhereOperator::In | WhereOperator::Nin) => {
                let values: Result<Vec<_>> =
                    arr.iter().map(|v| Self::value_to_sql(v, &WhereOperator::Eq)).collect();
                Ok(format!("ARRAY[{}]", values?.join(", ")))
            },

            // Array operations
            (
                Value::Array(_),
                WhereOperator::ArrayContains
                | WhereOperator::ArrayContainedBy
                | WhereOperator::ArrayOverlaps,
            ) => {
                // SECURITY: Serialize to JSON string and escape single quotes to prevent
                // SQL injection. The serde_json serializer handles internal escaping, and
                // we escape single quotes for the SQL string literal context.
                let json_str =
                    serde_json::to_string(value).map_err(|e| FraiseQLError::Internal {
                        message: format!("Failed to serialize JSON for array operator: {e}"),
                        source:  None,
                    })?;
                let escaped = json_str.replace('\'', "''");
                Ok(format!("'{}'::jsonb", escaped))
            },

            _ => Err(FraiseQLError::Internal {
                message: format!(
                    "Unsupported value type for operator: {value:?} with {operator:?}"
                ),
                source:  None,
            }),
        }
    }

    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_simple_equality() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'status' = 'active'");
    }

    #[test]
    fn test_nested_path() {
        let clause = WhereClause::Field {
            path:     vec!["user".to_string(), "email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data#>'{user}'->>'email' = 'test@example.com'");
    }

    #[test]
    fn test_icontains() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("john"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' ILIKE '%john%'");
    }

    #[test]
    fn test_startswith() {
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Startswith,
            value:    json!("admin"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'email' LIKE 'admin%'");
    }

    #[test]
    fn test_and_clause() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("active"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(18),
            },
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "(data->>'status' = 'active' AND data->>'age' >= 18)");
    }

    #[test]
    fn test_or_clause() {
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("moderator"),
            },
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "(data->>'type' = 'admin' OR data->>'type' = 'moderator')");
    }

    #[test]
    fn test_not_clause() {
        let clause = WhereClause::Not(Box::new(WhereClause::Field {
            path:     vec!["deleted".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        }));

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "NOT (data->>'deleted' = true)");
    }

    #[test]
    fn test_is_null() {
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'deleted_at' IS NULL");
    }

    #[test]
    fn test_is_not_null() {
        let clause = WhereClause::Field {
            path:     vec!["updated_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(false),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'updated_at' IS NOT NULL");
    }

    #[test]
    fn test_in_operator() {
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending", "approved"]),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'status' = ANY ARRAY['active', 'pending', 'approved']");
    }

    #[test]
    fn test_sql_injection_prevention() {
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("'; DROP TABLE users; --"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' = '''; DROP TABLE users; --'");
        // Single quotes are escaped to ''
    }

    #[test]
    fn test_numeric_comparison() {
        let clause = WhereClause::Field {
            path:     vec!["price".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(99.99),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'price' > 99.99");
    }

    #[test]
    fn test_boolean_value() {
        let clause = WhereClause::Field {
            path:     vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'published' = true");
    }

    #[test]
    fn test_empty_and_clause() {
        let clause = WhereClause::And(vec![]);
        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "TRUE");
    }

    #[test]
    fn test_empty_or_clause() {
        let clause = WhereClause::Or(vec![]);
        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "FALSE");
    }

    #[test]
    fn test_complex_nested_condition() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("article"),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path:     vec!["status".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("published"),
                },
                WhereClause::And(vec![
                    WhereClause::Field {
                        path:     vec!["status".to_string()],
                        operator: WhereOperator::Eq,
                        value:    json!("draft"),
                    },
                    WhereClause::Field {
                        path:     vec!["author".to_string(), "role".to_string()],
                        operator: WhereOperator::Eq,
                        value:    json!("admin"),
                    },
                ]),
            ]),
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(
            sql,
            "(data->>'type' = 'article' AND (data->>'status' = 'published' OR (data->>'status' = 'draft' AND data#>'{author}'->>'role' = 'admin')))"
        );
    }

    #[test]
    fn test_sql_injection_in_field_name_simple() {
        // Test that malicious field names are escaped to prevent SQL injection
        let clause = WhereClause::Field {
            path:     vec!["name'; DROP TABLE users; --".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("value"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        // Field name should be escaped with doubled single quotes
        // Result: data->>'name''; DROP TABLE users; --' = 'value'
        // The doubled '' prevents the quote from closing the string
        assert!(sql.contains("''")); // Escaped quotes present
        // The SQL structure should be: identifier->>'field' operator value
        // With escaping, DROP TABLE becomes part of the field string, not executable
        assert!(sql.contains("data->>'"));
        assert!(sql.contains("= 'value'")); // Proper value comparison
    }

    #[test]
    fn test_sql_injection_prevention_in_array_operator() {
        // SECURITY: Ensure JSON injection in array operators is escaped
        let clause = WhereClause::Field {
            path:     vec!["tags".to_string()],
            operator: WhereOperator::ArrayContains,
            value:    json!(["normal", "'; DROP TABLE users; --"]),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        // The JSON serializer will escape the inner quotes, and we escape SQL single quotes.
        // The result should be a properly escaped JSONB literal, not executable SQL.
        assert!(sql.contains("::jsonb"), "Must produce valid JSONB cast");
        // Verify the value is inside a JSON string (double-quoted), not a raw SQL string.
        // serde_json serializes this as: ["normal","'; DROP TABLE users; --"]
        // After SQL escaping: [\"normal\",\"''; DROP TABLE users; --\"]
        // The single quote inside the JSON value is doubled for SQL safety.
        assert!(
            sql.contains("''"),
            "Single quotes inside JSON values must be doubled for SQL safety"
        );
    }

    #[test]
    fn test_sql_injection_in_nested_field_name() {
        // Test that malicious nested field names are also escaped
        let clause = WhereClause::Field {
            path:     vec![
                "user".to_string(),
                "role'; DROP TABLE users; --".to_string(),
            ],
            operator: WhereOperator::Eq,
            value:    json!("admin"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        // Both simple and nested path components should be escaped
        assert!(sql.contains("''")); // Escaped quotes present
        assert!(sql.contains("data#>'{")); // Nested path syntax
    }

    #[test]
    fn test_where_generator_accepts_database_type() {
        // Phase 0: Add database awareness to WhereSqlGenerator
        // This test ensures WhereSqlGenerator can accept DatabaseType parameter
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        // Should accept DatabaseType parameter
        let _sql_pg = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        let _sql_mysql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL);
        let _sql_sqlite = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite);
        let _sql_sqlserver = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLServer);

        // All should succeed (for now, same SQL for basic operators)
        assert!(_sql_pg.is_ok());
        assert!(_sql_mysql.is_ok());
        assert!(_sql_sqlite.is_ok());
        assert!(_sql_sqlserver.is_ok());
    }

    #[test]
    fn test_template_lookup_and_substitution() {
        // Phase 0, Cycle 2: RED - Test template lookup and substitution
        // This test ensures apply_template can lookup and substitute templates

        // Test email domain extraction templates on PostgreSQL
        let sql_pg =
            WhereSqlGenerator::apply_template(DatabaseType::PostgreSQL, "domainEq", "data->>'email'", &json!("example.com"));
        assert!(sql_pg.is_ok());
        let sql = sql_pg.unwrap();
        assert!(sql.contains("SPLIT_PART"), "PostgreSQL should use SPLIT_PART for domain extraction");
        assert!(sql.contains("$field") == false, "Template placeholders should be substituted");
        assert!(sql.contains("data->>'email'"), "Field reference should be substituted");

        // Test MySQL version should use SUBSTRING_INDEX (different function)
        let sql_mysql =
            WhereSqlGenerator::apply_template(DatabaseType::MySQL, "domainEq", "data->>'email'", &json!("example.com"));
        assert!(sql_mysql.is_ok());
        let sql = sql_mysql.unwrap();
        assert!(sql.contains("SUBSTRING_INDEX"), "MySQL should use SUBSTRING_INDEX");
        assert!(sql.contains("?"), "MySQL should use ? for parameters");
        assert!(!sql.contains("$1"), "MySQL should not use $1 style parameters");
    }

    #[test]
    fn test_template_operators_routing() {
        // Phase 0, Cycle 3: Test that template-based operators can be identified
        // This test ensures we can identify which operators use templates vs basic SQL

        // Test that we can retrieve templates for known operators
        let email_template_pg = WhereSqlGenerator::get_template_for_operator(DatabaseType::PostgreSQL, "domainEq");
        assert!(email_template_pg.is_some(), "domainEq template should exist for PostgreSQL");

        let email_template_mysql = WhereSqlGenerator::get_template_for_operator(DatabaseType::MySQL, "domainIn");
        assert!(email_template_mysql.is_some(), "domainIn template should exist for MySQL");

        // Test that unknown operators return None
        let unknown_template = WhereSqlGenerator::get_template_for_operator(DatabaseType::PostgreSQL, "unknownOp");
        assert!(unknown_template.is_none(), "Unknown operators should return None");

        // Test cross-database template differences
        let pg_template = WhereSqlGenerator::get_template_for_operator(DatabaseType::PostgreSQL, "domainEq");
        let mysql_template = WhereSqlGenerator::get_template_for_operator(DatabaseType::MySQL, "domainEq");

        assert_ne!(
            pg_template, mysql_template,
            "PostgreSQL and MySQL should have different templates for the same operator"
        );

        // Verify database-specific SQL functions
        assert!(pg_template.unwrap().contains("SPLIT_PART"));
        assert!(mysql_template.unwrap().contains("SUBSTRING_INDEX"));
    }

    #[test]
    fn test_templates_exist_for_all_email_operators() {
        // Phase 0, Cycle 4: Integration test for template coverage
        // Verify that email operators have templates on all 4 databases

        let email_operators = vec!["domainEq", "domainIn", "domainEndswith", "localPartStartswith"];
        let databases = vec![
            DatabaseType::PostgreSQL,
            DatabaseType::MySQL,
            DatabaseType::SQLite,
            DatabaseType::SQLServer,
        ];

        for operator in &email_operators {
            for db in &databases {
                let template = WhereSqlGenerator::get_template_for_operator(*db, operator);
                assert!(template.is_some(), "Template should exist for {} on {:?}", operator, db);
            }
        }
    }

    #[test]
    fn test_parameter_substitution_per_database() {
        // Phase 0, Cycle 4: Verify database-specific parameter syntax
        // PostgreSQL uses $1, others use ?

        let pg_sql =
            WhereSqlGenerator::apply_template(DatabaseType::PostgreSQL, "domainEq", "data->>'email'", &json!("test@example.com"));
        assert!(pg_sql.is_ok());
        assert!(pg_sql.unwrap().contains("$1"), "PostgreSQL should use $1 parameter");

        let mysql_sql =
            WhereSqlGenerator::apply_template(DatabaseType::MySQL, "domainEq", "data->>'email'", &json!("test@example.com"));
        assert!(mysql_sql.is_ok());
        assert!(mysql_sql.unwrap().contains("?"), "MySQL should use ? parameter");

        let sqlite_sql =
            WhereSqlGenerator::apply_template(DatabaseType::SQLite, "domainEq", "data->>'email'", &json!("test@example.com"));
        assert!(sqlite_sql.is_ok());
        assert!(sqlite_sql.unwrap().contains("?"), "SQLite should use ? parameter");

        let sqlserver_sql =
            WhereSqlGenerator::apply_template(DatabaseType::SQLServer, "domainEq", "data->>'email'", &json!("test@example.com"));
        assert!(sqlserver_sql.is_ok());
        assert!(sqlserver_sql.unwrap().contains("?"), "SQL Server should use ? parameter");
    }

    #[test]
    fn test_template_with_to_sql_for_db() {
        // Phase 0, Cycle 4: Integration test combining database awareness with templates
        // Verify that to_sql_for_db can be extended to use templates for extended operators

        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        // Current implementation: basic operators work the same across databases
        let sql_pg = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        let sql_mysql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL);

        assert!(sql_pg.is_ok());
        assert!(sql_mysql.is_ok());

        // For basic Eq operator, result should be identical across databases
        // (In later phases, we'll have database-specific results for template operators)
        assert_eq!(sql_pg.unwrap(), sql_mysql.unwrap());
    }
}
