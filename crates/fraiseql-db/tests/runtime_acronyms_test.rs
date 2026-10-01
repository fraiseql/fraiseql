//! #1372 through the path the issue reported: an acronym registered by the project
//! (`[fraiseql.naming] acronyms = ["co2"]`), after the first word of a camelCase name.
//!
//! Its own test binary, holding a single test, because `set_runtime_acronyms` installs a
//! process-global set and only the first call wins: in the crate's unit tests another
//! test could install a different set first.

use fraiseql_db::utils::{set_runtime_acronyms, to_snake_case};

#[test]
fn a_project_registered_acronym_is_kept_whole_after_the_first_word() {
    // Before registration the built-in defaults apply, and `co2` is not one of them.
    assert_eq!(to_snake_case("emissionCo2Kg"), "emission_co_2_kg");

    set_runtime_acronyms(&["co2".to_string()]);

    assert_eq!(to_snake_case("emissionCo2Kg"), "emission_co2_kg");
    assert_eq!(to_snake_case("EmissionCo2Kg"), "emission_co2_kg");
    assert_eq!(to_snake_case("co2Emission"), "co2_emission");
    // The built-in defaults stay registered alongside the project's.
    assert_eq!(to_snake_case("hostIpv4"), "host_ipv4");
}
