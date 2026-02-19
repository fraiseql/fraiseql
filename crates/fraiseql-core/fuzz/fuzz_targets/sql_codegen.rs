#![no_main]

use libfuzzer_sys::fuzz_target;

/// Count unbalanced structural parentheses outside of single-quoted SQL strings.
fn structural_parens_balanced(sql: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_quote = false;
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quote {
            if c == '\'' {
                // Escaped quote '' stays in-quote
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_quote = false;
                }
            }
        } else {
            match c {
                '\'' => in_quote = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }

    depth == 0
}

fuzz_target!(|data: &str| {
    let Ok(clause) = serde_json::from_str::<fraiseql_core::db::WhereClause>(data) else {
        return;
    };

    let result = fraiseql_core::db::WhereSqlGenerator::to_sql(&clause);

    if let Ok(sql) = result {
        // SQL must not be empty for a valid clause
        assert!(!sql.is_empty(), "SQL generation produced empty string");

        // Structural parentheses must be balanced (ignoring those inside string literals)
        assert!(
            structural_parens_balanced(&sql),
            "Unbalanced structural parentheses in SQL: {sql}"
        );
    }
});
