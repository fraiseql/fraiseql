//! Unit tests for the capture-trigger install-script assembly (#366).

#![allow(clippy::unwrap_used)] // Reason: test module

use fraiseql_core::schema::{CompiledSchema, SubscribableEntity};

use super::build_ddl;

fn schema_with(entities: Vec<SubscribableEntity>) -> CompiledSchema {
    CompiledSchema {
        subscribable: entities,
        ..Default::default()
    }
}

#[test]
fn no_subscribable_types_yields_empty_script() {
    assert!(build_ddl(&CompiledSchema::default(), true).is_empty());
}

#[test]
fn includes_function_then_triggers_when_requested() {
    let schema = schema_with(vec![SubscribableEntity {
        entity_type: "Post".to_string(),
        tables:      vec!["tb_post".to_string()],
        pre_image:   false,
    }]);
    let ddl = build_ddl(&schema, true);
    // Self-contained: the function definition precedes the generated triggers.
    // Match the generated trigger name prefix (not bare "CREATE TRIGGER", which
    // also appears in the function migration's header comment).
    let fn_pos = ddl
        .find("CREATE OR REPLACE FUNCTION core.fn_entity_change_log_capture")
        .unwrap();
    let trig_pos = ddl.find("CREATE TRIGGER \"tr_cdc_capture").unwrap();
    assert!(fn_pos < trig_pos, "the function is defined before the triggers reference it");
    assert_eq!(
        ddl.matches("CREATE TRIGGER \"tr_cdc_capture").count(),
        3,
        "ins/upd/del for the one table"
    );
}

#[test]
fn omits_function_when_not_requested() {
    let schema = schema_with(vec![SubscribableEntity {
        entity_type: "Post".to_string(),
        tables:      vec!["tb_post".to_string()],
        pre_image:   false,
    }]);
    let ddl = build_ddl(&schema, false);
    assert!(
        !ddl.contains("CREATE OR REPLACE FUNCTION core.fn_entity_change_log_capture"),
        "function omitted"
    );
    assert!(ddl.contains("CREATE TRIGGER"), "triggers still present");
}
