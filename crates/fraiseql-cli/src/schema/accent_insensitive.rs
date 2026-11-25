//! The database objects `[accent_insensitive]` needs: the `unaccent` extension and the
//! `IMMUTABLE` wrapper its filters fold through.
//!
//! The server never creates them. `fraiseql compile --emit-ddl` writes them as one file,
//! and `fraiseql compile --database` refuses a database that lacks them, naming the
//! statements that create them. Both read the statements from here.

use fraiseql_db::where_clause::UnaccentFunction;

/// The file `--emit-ddl` writes the statements to. No type's file can have this name: a
/// type's is its `snake_case` name, which never holds a hyphen.
pub const DDL_FILE: &str = "accent-insensitive.sql";

/// The schema `unaccent` is created in when the statements create it.
pub const DEFAULT_EXTENSION_SCHEMA: &str = "public";

/// `CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA "<extension_schema>";`
#[must_use]
pub fn extension_ddl(extension_schema: &str) -> String {
    format!(
        "CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA {};",
        quote(extension_schema)
    )
}

/// `CREATE SCHEMA IF NOT EXISTS "<schema>";`, the wrapper's schema.
#[must_use]
pub fn schema_ddl(function: &UnaccentFunction) -> String {
    format!("CREATE SCHEMA IF NOT EXISTS {};", quote(function.schema()))
}

/// The wrapper, folding through the `unaccent` dictionary of the extension in
/// `extension_schema`.
///
/// `unaccent()` is `STABLE`, because it looks its dictionary up when called; an index
/// expression can only use an `IMMUTABLE` function. Naming the dictionary by schema pins it,
/// which is what makes declaring the wrapper `IMMUTABLE` sound.
#[must_use]
pub fn function_ddl(function: &UnaccentFunction, extension_schema: &str) -> String {
    let dictionary = format!("{}.unaccent", quote(extension_schema));
    format!(
        "CREATE OR REPLACE FUNCTION {schema}.{name}(text) RETURNS text\n    \
         LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT\n    \
         RETURN {dictionary}('{dictionary_literal}'::regdictionary, $1);",
        schema = quote(function.schema()),
        name = quote(function.function()),
        dictionary_literal = dictionary.replace('\'', "''"),
    )
}

/// The wrapper's schema, then the wrapper.
#[must_use]
pub fn wrapper_ddl(function: &UnaccentFunction, extension_schema: &str) -> String {
    format!("{}\n{}", schema_ddl(function), function_ddl(function, extension_schema))
}

/// Every statement, in the order they apply: the wrapper's schema, the extension, the
/// wrapper.
#[must_use]
pub fn ddl(function: &UnaccentFunction) -> String {
    format!(
        "{}\n{}\n{}\n",
        schema_ddl(function),
        extension_ddl(DEFAULT_EXTENSION_SCHEMA),
        function_ddl(function, DEFAULT_EXTENSION_SCHEMA)
    )
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// What `--database` found of the wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapperState {
    /// A function `<schema>.<name>(text)` exists, `IMMUTABLE`, returning `text`.
    Ready,
    /// No function `<schema>.<name>(text)`.
    Missing,
    /// It exists with another volatility: `s` (`STABLE`) or `v` (`VOLATILE`).
    NotImmutable(char),
    /// It exists returning another type, named here.
    NotText(String),
}

/// The drift errors `--database` reports for `function`, given whether the extension
/// exists (and in which schema) and what it found of the wrapper.
#[must_use]
pub fn drift(
    function: &UnaccentFunction,
    extension_schema: Option<&str>,
    wrapper: &WrapperState,
) -> Vec<String> {
    let mut errors = Vec::new();
    let in_schema = extension_schema.unwrap_or(DEFAULT_EXTENSION_SCHEMA);
    if extension_schema.is_none() {
        errors.push(format!(
            "[accent_insensitive]: the database has no `unaccent` extension; create it:\n    {}",
            extension_ddl(in_schema)
        ));
    }
    let problem = match wrapper {
        WrapperState::Ready => return errors,
        WrapperState::Missing => format!("the database has no function {function}(text)"),
        WrapperState::NotImmutable(volatility) => format!(
            "{function}(text) is {}, not IMMUTABLE, so no index can use it",
            if *volatility == 's' {
                "STABLE"
            } else {
                "VOLATILE"
            }
        ),
        WrapperState::NotText(returns) => {
            format!("{function}(text) returns {returns}, not text")
        },
    };
    errors.push(format!(
        "[accent_insensitive]: {problem}; create it (`fraiseql compile --emit-ddl` writes it \
         to {DDL_FILE}):\n    {}",
        wrapper_ddl(function, in_schema).replace('\n', "\n    ")
    ));
    errors
}

#[cfg(test)]
#[path = "accent_insensitive_tests.rs"]
mod accent_insensitive_tests;
