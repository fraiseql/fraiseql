//! Internal helper functions for connection operations

/// Extract entity name from query for metrics
/// Query format: SELECT data FROM v_{entity} ...
pub(super) fn extract_entity_from_query(query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    if let Some(from_pos) = query_lower.find("from") {
        let after_from = &query_lower[from_pos + 4..].trim_start();
        if let Some(entity_start) = after_from.find('v').or_else(|| after_from.find('t')) {
            let potential_table = &after_from[entity_start..];
            // Extract table name: "v_entity" or "tv_entity"
            let end_pos = potential_table
                .find(' ')
                .or_else(|| potential_table.find(';'))
                .unwrap_or(potential_table.len());
            let table_name = &potential_table[..end_pos];
            // Extract entity from table name
            if let Some(entity_pos) = table_name.rfind('_') {
                return Some(table_name[entity_pos + 1..].to_string());
            }
        }
    }
    None
}
