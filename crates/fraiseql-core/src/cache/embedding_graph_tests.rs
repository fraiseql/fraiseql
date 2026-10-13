#![allow(clippy::unwrap_used)] // Reason: test code

use super::*;
use crate::schema::{MutationDefinition, QueryDefinition, TypeDefinition};

#[test]
fn every_name_a_cached_entry_or_a_sweep_can_carry_is_collected() {
    let mut schema = CompiledSchema::default();
    let mut query = QueryDefinition::new("posts", "Post");
    query.sql_source = Some("tv_post".to_string());
    query.additional_views = vec!["v_extra".to_string()];
    schema.queries.push(query);
    schema.types.push(TypeDefinition::new("User", "app.tv_user"));
    schema.types.push(TypeDefinition::new("Payload", ""));
    let mut mutation = MutationDefinition::new("touch", "User");
    mutation.invalidates_views = vec!["v_declared".to_string()];
    schema.mutations.push(mutation);

    let names: Vec<String> = swept_view_names(&schema).into_iter().collect();
    assert_eq!(names, ["app.tv_user", "tv_post", "v_declared", "v_extra"]);
}
