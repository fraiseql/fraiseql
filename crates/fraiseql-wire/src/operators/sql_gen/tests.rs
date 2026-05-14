#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_eq_operator_jsonb_string() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::Eq(
        Field::JsonbField("name".to_string()),
        Value::String("John".to_string()),
    );
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    // JSONB string fields get ::text cast for proper text comparison
    assert_eq!(sql, "(data->'name')::text = $1");
    assert_eq!(param_index, 1);
}

#[test]
fn test_eq_operator_direct_column() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::Eq(
        Field::DirectColumn("status".to_string()),
        Value::String("active".to_string()),
    );
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    // Direct columns don't need casting (use native types)
    assert_eq!(sql, "status = $1");
    assert_eq!(param_index, 1);
}

#[test]
fn test_len_eq_operator() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::LenEq(Field::JsonbField("tags".to_string()), 5);
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "array_length((data->'tags'), 1) = 5");
    assert_eq!(param_index, 0); // No parameters for length operators
}

#[test]
fn test_is_ipv4_operator() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsIPv4(Field::JsonbField("ip".to_string()));
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "family((data->'ip')::inet) = 4");
}

#[test]
fn test_l2_distance_operator() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::L2Distance {
        field: Field::JsonbField("embedding".to_string()),
        vector: vec![0.1, 0.2, 0.3],
        threshold: 0.5,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(
        sql,
        "l2_distance((data->'embedding')::vector, $1::vector) < 0.5"
    );
    assert_eq!(param_index, 1);
}

#[test]
fn test_in_operator() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::In(
        Field::JsonbField("status".to_string()),
        vec![
            Value::String("active".to_string()),
            Value::String("pending".to_string()),
        ],
    );
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "(data->'status') IN ($1, $2)");
    assert_eq!(param_index, 2);
}

#[test]
fn test_in_empty_list_returns_false() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::In(Field::DirectColumn("status".to_string()), vec![]);
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "FALSE");
    assert_eq!(param_index, 0, "no parameters consumed for empty IN");
}

#[test]
fn test_nin_empty_list_returns_true() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::Nin(Field::DirectColumn("status".to_string()), vec![]);
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "TRUE");
    assert_eq!(param_index, 0, "no parameters consumed for empty NOT IN");
}

// Helper: extract the inner string from Value::String via Debug, panics otherwise.
fn value_as_str(v: &Value) -> &str {
    match v {
        Value::String(s) => s.as_str(),
        other => panic!("expected Value::String, got {other:?}"),
    }
}

#[test]
fn test_contains_escapes_percent() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op =
        WhereOperator::Contains(Field::DirectColumn("note".to_string()), "50%".to_string());
    generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(value_as_str(&params[&1]), "50\\%");
}

#[test]
fn test_contains_escapes_underscore() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op =
        WhereOperator::Contains(Field::DirectColumn("code".to_string()), "A_B".to_string());
    generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(value_as_str(&params[&1]), "A\\_B");
}

#[test]
fn test_startswith_escapes_wildcard_in_prefix() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op =
        WhereOperator::Startswith(Field::DirectColumn("name".to_string()), "C%D".to_string());
    generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    // prefix escaped, trailing % appended for LIKE
    assert_eq!(value_as_str(&params[&1]), "C\\%D%");
}

#[test]
fn test_endswith_escapes_wildcard_in_suffix() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::Endswith(
        Field::DirectColumn("name".to_string()),
        "_suffix".to_string(),
    );
    generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    // suffix escaped, leading % prepended for LIKE
    assert_eq!(value_as_str(&params[&1]), "%\\_suffix");
}

#[test]
fn test_escape_like_literal_backslash() {
    assert_eq!(escape_like_literal("a\\b"), "a\\\\b");
    assert_eq!(escape_like_literal("a%b"), "a\\%b");
    assert_eq!(escape_like_literal("a_b"), "a\\_b");
    // Combined: order matters -- backslash must be escaped first
    assert_eq!(escape_like_literal("100%_\\n"), "100\\%\\_\\\\n");
}

// ============ LTree Operator Tests ============

#[test]
fn test_ltree_ancestor_of() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::AncestorOf {
        field: Field::DirectColumn("path".to_string()),
        path: "Top.Sciences.Astronomy".to_string(),
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree @> $1::ltree");
    assert_eq!(param_index, 1);
}

#[test]
fn test_ltree_descendant_of() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::DescendantOf {
        field: Field::DirectColumn("path".to_string()),
        path: "Top.Sciences".to_string(),
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree <@ $1::ltree");
    assert_eq!(param_index, 1);
}

#[test]
fn test_ltree_matches_lquery() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::MatchesLquery {
        field: Field::DirectColumn("path".to_string()),
        pattern: "Top.*.Ast*".to_string(),
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree ~ $1::lquery");
    assert_eq!(param_index, 1);
}

#[test]
fn test_ltree_matches_ltxtquery() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::MatchesLtxtquery {
        field: Field::DirectColumn("path".to_string()),
        query: "Science & !Deprecated".to_string(),
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree @ $1::ltxtquery");
    assert_eq!(param_index, 1);
}

#[test]
fn test_ltree_matches_any_lquery() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::MatchesAnyLquery {
        field: Field::DirectColumn("path".to_string()),
        patterns: vec!["Top.*".to_string(), "Other.*".to_string()],
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree ? ARRAY[$1::lquery, $2::lquery]");
    assert_eq!(param_index, 2);
}

#[test]
fn test_ltree_depth_eq() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::DepthEq {
        field: Field::DirectColumn("path".to_string()),
        depth: 3,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "nlevel(path::ltree) = 3");
    assert_eq!(param_index, 0); // Depth is inlined, not parameterized
}

#[test]
fn test_ltree_depth_gt() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::DepthGt {
        field: Field::DirectColumn("path".to_string()),
        depth: 2,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "nlevel(path::ltree) > 2");
    assert_eq!(param_index, 0);
}

#[test]
fn test_ltree_depth_lte() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::DepthLte {
        field: Field::DirectColumn("path".to_string()),
        depth: 5,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "nlevel(path::ltree) <= 5");
    assert_eq!(param_index, 0);
}

// ============ CIDR Containment Check Helper Tests ============

#[test]
fn test_cidr_containment_check_single_range() {
    let sql = cidr_containment_check("ip_addr", &["100.64.0.0/10"], false);
    assert_eq!(sql, "(ip_addr::inet << '100.64.0.0/10'::inet)");
}

#[test]
fn test_cidr_containment_check_multiple_ranges() {
    let sql = cidr_containment_check("ip_addr", &["224.0.0.0/4", "ff00::/8"], false);
    assert_eq!(
        sql,
        "(ip_addr::inet << '224.0.0.0/4'::inet OR ip_addr::inet << 'ff00::/8'::inet)"
    );
}

#[test]
fn test_cidr_containment_check_negated() {
    let sql = cidr_containment_check("ip_addr", &["224.0.0.0/4", "ff00::/8"], true);
    assert_eq!(
        sql,
        "NOT (ip_addr::inet << '224.0.0.0/4'::inet OR ip_addr::inet << 'ff00::/8'::inet)"
    );
}

#[test]
fn test_cidr_containment_check_single_range_negated() {
    let sql = cidr_containment_check("ip_addr", &["100.64.0.0/10"], true);
    assert_eq!(sql, "NOT (ip_addr::inet << '100.64.0.0/10'::inet)");
}

// ============ Network Operator Tests (boolean value pattern) ============

#[test]
fn test_is_private_true() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsPrivate {
        field: Field::DirectColumn("src_ip".to_string()),
        value: true,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.contains("10.0.0.0/8"), "must check 10/8");
    assert!(sql.contains("172.16.0.0/12"), "must check 172.16/12");
    assert!(sql.contains("192.168.0.0/16"), "must check 192.168/16");
    assert!(!sql.starts_with("NOT "), "value=true must not negate");
}

#[test]
fn test_is_private_false_replaces_is_public() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsPrivate {
        field: Field::DirectColumn("src_ip".to_string()),
        value: false,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.starts_with("NOT ("), "value=false must negate with NOT (");
    assert!(sql.contains("10.0.0.0/8"));
}

#[test]
fn test_is_loopback_true() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsLoopback {
        field: Field::DirectColumn("ip".to_string()),
        value: true,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.contains("127.0.0.0/8"));
    assert!(sql.contains("::1/128"));
    assert!(!sql.starts_with("NOT "));
}

#[test]
fn test_is_loopback_false() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsLoopback {
        field: Field::DirectColumn("ip".to_string()),
        value: false,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.starts_with("NOT ("), "value=false must negate");
    assert!(sql.contains("127.0.0.0/8"));
}

// ============ isMulticast / isLinkLocal Operator Tests ============

#[test]
fn test_is_multicast_true() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsMulticast {
        field: Field::DirectColumn("ip".to_string()),
        value: true,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.contains("224.0.0.0/4"), "must check IPv4 multicast");
    assert!(sql.contains("ff00::/8"), "must check IPv6 multicast");
    assert!(!sql.starts_with("NOT "));
}

#[test]
fn test_is_multicast_false() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsMulticast {
        field: Field::DirectColumn("ip".to_string()),
        value: false,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.starts_with("NOT ("), "value=false must negate");
    assert!(sql.contains("224.0.0.0/4"));
}

#[test]
fn test_is_link_local_true() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsLinkLocal {
        field: Field::DirectColumn("ip".to_string()),
        value: true,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.contains("169.254.0.0/16"), "must check IPv4 link-local");
    assert!(sql.contains("fe80::/10"), "must check IPv6 link-local");
    assert!(!sql.starts_with("NOT "));
}

#[test]
fn test_is_link_local_false() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::IsLinkLocal {
        field: Field::DirectColumn("ip".to_string()),
        value: false,
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert!(sql.starts_with("NOT ("), "value=false must negate");
    assert!(sql.contains("169.254.0.0/16"));
}

#[test]
fn test_ltree_lca() {
    let mut param_index = 0;
    let mut params = HashMap::new();
    let op = WhereOperator::Lca {
        field: Field::DirectColumn("path".to_string()),
        paths: vec![
            "Org.Engineering.Backend".to_string(),
            "Org.Engineering.Frontend".to_string(),
        ],
    };
    let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
    assert_eq!(sql, "path::ltree = lca(ARRAY[$1::ltree, $2::ltree])");
    assert_eq!(param_index, 2);
}
