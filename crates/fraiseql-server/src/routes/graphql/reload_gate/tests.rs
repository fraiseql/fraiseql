//! Tests for the reload gate: what a reload reapplies and what it refuses.

use fraiseql_core::schema::{CompiledSchema, ObserverDefinition, ObserversConfig};

use super::{RELOAD_REAPPLIED, boot_frozen_drift};

#[test]
fn a_changed_observer_declaration_is_reapplied_not_refused() {
    let next = CompiledSchema {
        observers: vec![ObserverDefinition::new("order_created", "Order", "INSERT")],
        ..CompiledSchema::default()
    };

    assert!(boot_frozen_drift(&CompiledSchema::default(), &next).is_empty());
    assert!(RELOAD_REAPPLIED.contains(&"observers"));
}

/// The runtime's own configuration is read at boot: only the declarations are re-applied.
#[test]
fn a_changed_observer_runtime_configuration_still_needs_a_restart() {
    let next = CompiledSchema {
        observers_config: Some(ObserversConfig::default()),
        ..CompiledSchema::default()
    };

    assert_eq!(boot_frozen_drift(&CompiledSchema::default(), &next), ["observers_config"]);
}
