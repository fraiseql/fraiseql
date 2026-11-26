//! How many name allocations the selection-driven projector makes per projected row.
//!
//! [`project_entity`] resolves each selected field's stored key as it reads the row. The
//! measure is differential, so it counts only that resolution: a row is projected, and the
//! same response is built by hand into the same map type. Their difference has a constant
//! part (the projector's per-level work) and a part that grows with the number of selected
//! fields. Measured at two widths, the growth per field is the name allocations per field.

use fraiseql_core::runtime::project_entity;
use serde_json::{Map, Value, json};

use super::wide_order::{declared, row, schema, selections};

/// The response `project_entity` answers for `row(width)`, built directly.
fn by_hand(width: usize) -> Value {
    let ints = || {
        let mut level = Map::new();
        for i in 0..width {
            level.insert(declared(i), json!(i));
        }
        level
    };
    let mut order = ints();
    order.insert("shippingAddress".into(), Value::Object(ints()));
    Value::Object(order)
}

/// Blocks allocated by `f`, and what it returned.
fn blocks<T>(f: impl FnOnce() -> T) -> (u64, T) {
    let before = dhat::HeapStats::get().total_blocks;
    let out = f();
    (dhat::HeapStats::get().total_blocks - before, out)
}

/// Blocks `project_entity` allocates for one row of `width`, beyond building its response.
fn overhead_per_row(width: usize) -> u64 {
    let schema = schema(width);
    let selections = selections(width);
    let row = row(width);
    let (projected_blocks, projected) =
        blocks(|| project_entity(&row, "Order", &selections, &schema));
    let (built_blocks, built) = blocks(|| by_hand(width));
    assert_eq!(projected, built, "the hand-built response is the projected one");
    projected_blocks - built_blocks
}

/// Name allocations per selected field per projected row, at every depth.
///
/// `stored_key_candidates` builds both spellings of the declared name on every lookup: the
/// `snake_case` key, grown a character at a time, and the `camelCase` fallback. A key
/// resolved once, at schema load, makes the count zero.
pub fn name_allocations_per_field() -> u64 {
    // The projector collects each level's selections into a `Vec`, which grows at 4, 8 and
    // 16 entries. Both widths keep every level between 5 and 8 entries, so that growth is
    // the same and cancels, and the difference holds only what each field costs.
    let narrow = 5;
    let wide = 7;
    let growth = overhead_per_row(wide) - overhead_per_row(narrow);
    let fields_added = 2 * (wide - narrow) as u64; // the root's and `shippingAddress`'s
    assert_eq!(growth % fields_added, 0, "the overhead grows by a whole number per field");
    growth / fields_added
}
