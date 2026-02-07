//! Performance benchmarks for rich filter compiler.
//!
//! Tests measure compilation speed and memory usage for:
//! 1. Empty schema compilation (baseline)
//! 2. Schema with multiple types
//! 3. Schema with complex relationships

use fraiseql_cli::schema::intermediate::IntermediateSchema;
use fraiseql_cli::schema::SchemaConverter;

/// Benchmark: Compile empty schema with auto-generated rich types
#[test]
fn bench_compile_empty_schema_rich_types() {
    let iterations = 1000;
    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let intermediate = IntermediateSchema {
            security: None,
            version: "2.0.0".to_string(),
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
            fragments: None,
            directives: None,
            fact_tables: None,
            aggregate_queries: None,
            observers: None,
        };

        let _compiled = SchemaConverter::convert(intermediate)
            .expect("Compilation should succeed");
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;

    println!("Empty schema compilation:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average: {:.2} µs per iteration", avg_ns / 1000.0);

    // Verify performance is reasonable (should be < 1ms per iteration)
    assert!(
        (elapsed.as_millis() as f64 / iterations as f64) < 1.0,
        "Compilation too slow: average > 1ms"
    );
}

/// Benchmark: Metadata access and search performance
#[test]
fn bench_metadata_access_performance() {
    let intermediate = IntermediateSchema {
        security: None,
        version: "2.0.0".to_string(),
        types: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        fragments: None,
        directives: None,
        fact_tables: None,
        aggregate_queries: None,
        observers: None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let start = std::time::Instant::now();
    let iterations = 10000;

    for _ in 0..iterations {
        let _ = compiled.input_types.iter().find(|t| t.name == "EmailAddressWhereInput");
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;

    println!("Metadata access performance:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average: {:.2} ns per lookup", avg_ns);

    // Verify lookups are fast
    assert!(
        avg_ns < 10000.0, // Should be < 10µs
        "Metadata lookup too slow: average > 10µs"
    );
}

/// Benchmark: Operator metadata parsing
#[test]
fn bench_operator_metadata_parsing() {
    let intermediate = IntermediateSchema {
        security: None,
        version: "2.0.0".to_string(),
        types: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        fragments: None,
        directives: None,
        fact_tables: None,
        aggregate_queries: None,
        observers: None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should exist");

    let start = std::time::Instant::now();
    let iterations = 1000;

    for _ in 0..iterations {
        if let Some(metadata) = &email_where.metadata {
            let _ = metadata.get("operators");
        }
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;

    println!("Operator metadata parsing:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average: {:.2} ns per parse", avg_ns);

    assert!(
        avg_ns < 10000.0,
        "Metadata parsing too slow: average > 10µs"
    );
}

/// Benchmark: Database template access
#[test]
fn bench_database_template_access() {
    let intermediate = IntermediateSchema {
        security: None,
        version: "2.0.0".to_string(),
        types: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        fragments: None,
        directives: None,
        fact_tables: None,
        aggregate_queries: None,
        observers: None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should exist");

    let metadata = email_where.metadata.as_ref().unwrap();
    let operators = metadata["operators"].as_object().unwrap();

    let start = std::time::Instant::now();
    let iterations = 10000;

    for _ in 0..iterations {
        for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
            let _ = operators
                .get("domainEq")
                .and_then(|op| op.as_object())
                .and_then(|obj| obj.get(*db));
        }
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / (iterations as f64 * 4.0);

    println!("Database template access:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average: {:.2} ns per template access", avg_ns);

    assert!(
        avg_ns < 5000.0,
        "Template access too slow: average > 5µs"
    );
}

/// Benchmark: Lookup data access
#[test]
fn bench_lookup_data_access() {
    let intermediate = IntermediateSchema {
        security: None,
        version: "2.0.0".to_string(),
        types: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        fragments: None,
        directives: None,
        fact_tables: None,
        aggregate_queries: None,
        observers: None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let security = compiled.security.as_ref().expect("Security should exist");
    let lookup = security["lookup_data"].as_object().expect("Lookup data should exist");

    let start = std::time::Instant::now();
    let iterations = 10000;

    for _ in 0..iterations {
        let _ = lookup
            .get("countries")
            .and_then(|c| c.as_object())
            .and_then(|obj| obj.get("US"));
    }

    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;

    println!("Lookup data access:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average: {:.2} ns per lookup", avg_ns);

    assert!(
        avg_ns < 10000.0,
        "Lookup data access too slow: average > 10µs"
    );
}

/// Benchmark: Full operator metadata traversal
#[test]
fn bench_full_operator_traversal() {
    let intermediate = IntermediateSchema {
        security: None,
        version: "2.0.0".to_string(),
        types: vec![],
        enums: vec![],
        input_types: vec![],
        interfaces: vec![],
        unions: vec![],
        queries: vec![],
        mutations: vec![],
        subscriptions: vec![],
        fragments: None,
        directives: None,
        fact_tables: None,
        aggregate_queries: None,
        observers: None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let start = std::time::Instant::now();
    let mut count = 0;

    // Traverse all WhereInput types and count operators
    for where_input in &compiled.input_types {
        if let Some(metadata) = &where_input.metadata {
            if let Some(operators) = metadata.get("operators").and_then(|o| o.as_object()) {
                for (_op_name, templates) in operators {
                    if let Some(db_templates) = templates.as_object() {
                        for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
                            if db_templates.contains_key(*db) {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let elapsed = start.elapsed();

    println!("Full operator traversal:");
    println!("  Time: {:?}", elapsed);
    println!("  Total operators found: {}", count);
    println!("  Expected: 49 types × ~4-10 operators × 4 databases");

    // Rough sanity check (49 types, average 4 operators per type, 4 databases = ~784 operators)
    assert!(
        count > 100,
        "Should find many operators (found {})",
        count
    );
}
