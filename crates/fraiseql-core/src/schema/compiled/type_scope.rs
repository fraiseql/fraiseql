//! The scope a level of a type is read under, wherever it is read.
//!
//! A type is scoped by `inject_params` declared on one of two places: the query that reads
//! it, or the type itself (`TypeDefinition::inject_params`). A level of the type is read in
//! more places than its queries: embedded in another type's document, returned by a write,
//! filtered through, delivered by a subscription. Every one of them asks
//! [`type_scope`](CompiledSchema::type_scope), so the scope a type declares on itself holds
//! there too, whether or not a query returns the type.
//!
//! The type's own declaration is never dropped because a query exists. A query that returns
//! the type under another scope, or none, is refused when the schema loads
//! ([`type_inject_violations`](CompiledSchema::type_inject_violations)), so the two only
//! ever agree in a schema that loads.

use std::borrow::Cow;

use indexmap::IndexMap;

use super::{CompiledSchema, QueryDefinition};
use crate::schema::InjectedParamSource;

impl CompiledSchema {
    /// The `inject_params` `type_name` declares on itself, when it declares any.
    #[must_use]
    pub fn own_scope(&self, type_name: &str) -> Option<&IndexMap<String, InjectedParamSource>> {
        self.find_type(type_name)
            .map(|t| &t.inject_params)
            .filter(|scope| !scope.is_empty())
    }

    /// The read a level of `type_name` is scoped as.
    ///
    /// The type's [`scoping_read`](Self::scoping_read), carrying the type's own
    /// `inject_params` when it declares them. A type no query reads, scoped by its own
    /// declaration, is read as itself: a read named after the type, over its `sql_source`.
    /// `None` for a type neither a query nor the type itself scopes, and no query reads.
    #[must_use]
    pub fn type_scope(&self, type_name: &str) -> Option<Cow<'_, QueryDefinition>> {
        let read = self.scoping_read(type_name);
        let Some(own) = self.own_scope(type_name) else {
            return read.map(Cow::Borrowed);
        };
        let mut scoped = read.cloned().unwrap_or_else(|| self.read_of_type(type_name));
        scoped.inject_params.clone_from(own);
        Some(Cow::Owned(scoped))
    }

    /// The read named `name`, as a level of `type_name` is scoped by it: a query of that
    /// name under the type's own scope, or the type's own read when `name` is the type's.
    #[must_use]
    pub fn scoped_read_named(
        &self,
        name: &str,
        type_name: &str,
    ) -> Option<Cow<'_, QueryDefinition>> {
        match self.queries.iter().find(|q| q.name == name) {
            Some(query) => Some(match self.own_scope(type_name) {
                Some(own) => {
                    let mut scoped = query.clone();
                    scoped.inject_params.clone_from(own);
                    Cow::Owned(scoped)
                },
                None => Cow::Borrowed(query),
            }),
            None if name == type_name => self.type_scope(type_name),
            None => None,
        }
    }

    /// A list read of `type_name` over the type's own `sql_source`, for a type no query
    /// reads.
    fn read_of_type(&self, type_name: &str) -> QueryDefinition {
        let source = self.find_type(type_name).map(|t| t.sql_source.as_str().to_string());
        let read = QueryDefinition::new(type_name, type_name).returning_list();
        match source {
            Some(source) => read.with_sql_source(source),
            None => read,
        }
    }
}
