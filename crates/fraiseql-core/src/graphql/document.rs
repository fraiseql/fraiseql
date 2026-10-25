//! The executable document a request sends, read once into fraiseql's own model.
//!
//! [`parse`](crate::graphql::document::parse) is the only code that reads a parser's tree.
//! Everything downstream (the operation selection of § 6.1, the depth, complexity and cost
//! estimates, the introspection policy, the subscription root, the matcher) reads a
//! [`Document`](crate::graphql::document::Document) of fraiseql's types, so the parser is one
//! module's concern.
//!
//! The parser is `apollo-parser`, chosen for keeping a document as written: an object
//! literal's keys stay in the order written, so `orderBy: {ratio: ASC, current: DESC}` sorts by
//! `ratio` first (#1567), and a key written twice is seen, and refused (§ 5.6.3). It is also
//! error-resilient by design: a fuzz campaign seeded with every input that panicked the
//! parsers measured against it (graphql-parser 0.4 on a block string indented with U+00A0,
//! #976; cynic-parser 0.11 on the same, on an integer literal beyond `i64` and on a NUL in a
//! block string) ran 8.3 million inputs without a panic.

use apollo_parser::cst::{self, CstNode as _};

use crate::graphql::{
    types::{Directive, FieldSelection, FragmentDefinition, GraphQLArgument, GraphQLType},
    value_json::{self, LiteralError},
};

/// The deepest nesting the parser follows before refusing a document, rather than recursing
/// further. Far above any real selection set: the depth limit of the request validator is
/// what refuses a deep query, with its own message.
const RECURSION_LIMIT: usize = 500;

/// An executable document: its operations and fragments, in document order.
#[derive(Debug, Clone)]
pub struct Document {
    /// Every operation the document defines.
    pub operations:    Vec<Operation>,
    /// Every fragment the document defines.
    pub fragments:     Vec<FragmentDefinition>,
    /// The request validator's score, once a request computed one that no variable
    /// decided: a document the executor's cache shares is then scored once.
    pub(super) scored: std::sync::OnceLock<super::complexity::Scored>,
}

/// The kind of an operation (§ 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    /// `query`, or a bare selection set.
    Query,
    /// `mutation`.
    Mutation,
    /// `subscription`.
    Subscription,
}

impl OperationKind {
    /// The keyword that introduces the operation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

/// One operation of a document.
#[derive(Debug, Clone)]
pub struct Operation {
    /// Its kind.
    pub kind:       OperationKind,
    /// Its name; `None` for an anonymous operation.
    pub name:       Option<String>,
    /// Its variable definitions, in order.
    pub variables:  Vec<crate::graphql::types::VariableDefinition>,
    /// The directives on the operation itself.
    pub directives: Vec<Directive>,
    /// Its selection set.
    pub selections: Vec<FieldSelection>,
}

/// Why a document was refused, with where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentError(String);

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Read `source` as an executable document.
///
/// # Errors
///
/// [`DocumentError`] naming the line and column of the first syntax error, of a type-system
/// definition (a request may hold only operations and fragments), of a literal that cannot be
/// read (an integer beyond `i64`, a number beyond `f64`, an input object naming a key twice, a
/// value nesting deeper than [`value_json::MAX_DEPTH`]), or of a node the parser left
/// incomplete.
pub fn parse(source: &str) -> Result<Document, DocumentError> {
    let tree = apollo_parser::Parser::new(source).recursion_limit(RECURSION_LIMIT).parse();
    let at = Locator::new(source);
    if let Some(error) = tree.errors().next() {
        return Err(at.error(error.index(), &readable(error.message())));
    }
    let mut document = Document {
        operations: Vec::new(),
        fragments:  Vec::new(),
        scored:     std::sync::OnceLock::new(),
    };
    let reader = Reader { at: &at };
    for definition in tree.document().definitions() {
        match definition {
            cst::Definition::OperationDefinition(op) => {
                document.operations.push(reader.operation(&op)?);
            },
            cst::Definition::FragmentDefinition(fragment) => {
                document.fragments.push(reader.fragment(&fragment)?);
            },
            other => {
                let text = other.syntax().text().to_string();
                let head = text.split(['{', '(', '\n']).next().unwrap_or("").trim().to_string();
                return Err(at.error(
                    offset(other.syntax()),
                    &format!(
                        "`{head}` is a type-system definition; a request document holds \
                         operations and fragments only"
                    ),
                ));
            },
        }
    }
    Ok(document)
}

/// A parser message in the characters an author writes: the parser names a missing token by
/// its kind (`expected R_CURLY, got EOF`).
fn readable(message: &str) -> String {
    const TOKENS: [(&str, &str); 14] = [
        ("BANG", "!"),
        ("L_PAREN", "("),
        ("R_PAREN", ")"),
        ("L_CURLY", "{"),
        ("R_CURLY", "}"),
        ("L_BRACK", "["),
        ("R_BRACK", "]"),
        ("AT", "@"),
        ("DOLLAR", "$"),
        ("AMP", "&"),
        ("PIPE", "|"),
        ("SPREAD", "..."),
        ("EQ", "="),
        ("COLON", ":"),
    ];
    let Some((expected, got)) =
        message.strip_prefix("expected ").and_then(|rest| rest.split_once(", got "))
    else {
        return message.to_string();
    };
    let expected = TOKENS.iter().find(|(kind, _)| *kind == expected).map_or_else(
        || expected.to_lowercase().replace('_', " "),
        |(_, token)| format!("`{token}`"),
    );
    let got = if got == "EOF" {
        "the end of the document".to_string()
    } else {
        format!("`{got}`")
    };
    format!("expected {expected}, found {got}")
}

/// Byte offset → `line:column`, both 1-based, the column counted in characters.
struct Locator<'s> {
    source: &'s str,
}

impl<'s> Locator<'s> {
    const fn new(source: &'s str) -> Self {
        Self { source }
    }

    fn position(&self, offset: usize) -> (usize, usize) {
        let before = self.source.get(..offset).unwrap_or(self.source);
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        (line, column)
    }

    fn error(&self, offset: usize, message: &str) -> DocumentError {
        let (line, column) = self.position(offset);
        DocumentError(format!("{message} at {line}:{column}"))
    }
}

fn offset(node: &apollo_parser::SyntaxNode) -> usize {
    usize::from(node.text_range().start())
}

/// Reads the nodes of a tree the parser reported no error for. Every child the grammar
/// requires is then present; one that is not is refused rather than unwrapped.
struct Reader<'a, 's> {
    at: &'a Locator<'s>,
}

impl Reader<'_, '_> {
    fn incomplete(&self, node: &apollo_parser::SyntaxNode, what: &str) -> DocumentError {
        self.at.error(offset(node), &format!("incomplete {what}"))
    }

    fn name(
        &self,
        name: Option<cst::Name>,
        parent: &apollo_parser::SyntaxNode,
        what: &str,
    ) -> Result<String, DocumentError> {
        name.map(|n| n.text().to_string()).ok_or_else(|| self.incomplete(parent, what))
    }

    fn operation(&self, op: &cst::OperationDefinition) -> Result<Operation, DocumentError> {
        // A bare selection set has no operation type, and is a query.
        let kind = match op.operation_type() {
            Some(t) if t.mutation_token().is_some() => OperationKind::Mutation,
            Some(t) if t.subscription_token().is_some() => OperationKind::Subscription,
            _ => OperationKind::Query,
        };
        let variables = op
            .variable_definitions()
            .into_iter()
            .flat_map(|defs| defs.variable_definitions())
            .map(|def| self.variable(&def))
            .collect::<Result<_, _>>()?;
        Ok(Operation {
            kind,
            name: op.name().map(|n| n.text().to_string()),
            variables,
            directives: self.directives(op.directives())?,
            selections: self.selection_set(op.selection_set())?,
        })
    }

    fn fragment(&self, f: &cst::FragmentDefinition) -> Result<FragmentDefinition, DocumentError> {
        let name = self.name(f.fragment_name().and_then(|n| n.name()), f.syntax(), "fragment")?;
        let type_condition = self.name(
            f.type_condition().and_then(|t| t.named_type()).and_then(|t| t.name()),
            f.syntax(),
            "fragment type condition",
        )?;
        let selections = self.selection_set(f.selection_set())?;
        let fragment_spreads = spreads(&selections);
        Ok(FragmentDefinition {
            name,
            type_condition,
            selections,
            fragment_spreads,
        })
    }

    fn variable(
        &self,
        def: &cst::VariableDefinition,
    ) -> Result<crate::graphql::types::VariableDefinition, DocumentError> {
        let name =
            self.name(def.variable().and_then(|v| v.name()), def.syntax(), "variable definition")?;
        let ty = def.ty().ok_or_else(|| self.incomplete(def.syntax(), "variable type"))?;
        let default_value = match def.default_value() {
            None => None,
            Some(default) => {
                let value = default
                    .value()
                    .ok_or_else(|| self.incomplete(default.syntax(), "default value"))?;
                Some(self.literal(&value)?)
            },
        };
        Ok(crate::graphql::types::VariableDefinition {
            name,
            var_type: self.graphql_type(&ty)?,
            default_value,
        })
    }

    fn graphql_type(&self, ty: &cst::Type) -> Result<GraphQLType, DocumentError> {
        match ty {
            cst::Type::NamedType(named) => Ok(GraphQLType {
                name:          self.name(named.name(), named.syntax(), "type")?,
                nullable:      true,
                list:          false,
                list_nullable: false,
            }),
            cst::Type::ListType(list) => {
                let inner = list.ty().ok_or_else(|| self.incomplete(list.syntax(), "list type"))?;
                Ok(GraphQLType {
                    name:          format!("[{}]", self.graphql_type(&inner)?.name),
                    nullable:      true,
                    list:          true,
                    list_nullable: true,
                })
            },
            cst::Type::NonNullType(non_null) => {
                let mut parsed = match (non_null.named_type(), non_null.list_type()) {
                    (Some(named), _) => self.graphql_type(&cst::Type::NamedType(named))?,
                    (None, Some(list)) => self.graphql_type(&cst::Type::ListType(list))?,
                    (None, None) => return Err(self.incomplete(non_null.syntax(), "type")),
                };
                parsed.nullable = false;
                if parsed.list {
                    parsed.list_nullable = false;
                }
                Ok(parsed)
            },
        }
    }

    fn selection_set(
        &self,
        set: Option<cst::SelectionSet>,
    ) -> Result<Vec<FieldSelection>, DocumentError> {
        set.into_iter()
            .flat_map(|set| set.selections())
            .map(|selection| self.selection(&selection))
            .collect()
    }

    fn selection(&self, selection: &cst::Selection) -> Result<FieldSelection, DocumentError> {
        Ok(match selection {
            cst::Selection::Field(field) => FieldSelection {
                name:          self.name(field.name(), field.syntax(), "field")?,
                alias:         field.alias().and_then(|a| a.name()).map(|n| n.text().to_string()),
                arguments:     self.arguments(field.arguments())?,
                nested_fields: self.selection_set(field.selection_set())?,
                directives:    self.directives(field.directives())?,
            },
            // A spread is a field named `...<fragment>`, resolved by the fragment resolver.
            cst::Selection::FragmentSpread(spread) => FieldSelection {
                name:          format!(
                    "...{}",
                    self.name(
                        spread.fragment_name().and_then(|n| n.name()),
                        spread.syntax(),
                        "fragment spread"
                    )?
                ),
                alias:         None,
                arguments:     Vec::new(),
                nested_fields: Vec::new(),
                directives:    self.directives(spread.directives())?,
            },
            // An inline fragment is a field named `...on <type>` (`...on ` with no type).
            cst::Selection::InlineFragment(inline) => FieldSelection {
                name:          format!(
                    "...on {}",
                    inline
                        .type_condition()
                        .and_then(|t| t.named_type())
                        .and_then(|t| t.name())
                        .map(|n| n.text().to_string())
                        .unwrap_or_default()
                ),
                alias:         None,
                arguments:     Vec::new(),
                nested_fields: self.selection_set(inline.selection_set())?,
                directives:    self.directives(inline.directives())?,
            },
        })
    }

    fn arguments(
        &self,
        arguments: Option<cst::Arguments>,
    ) -> Result<Vec<GraphQLArgument>, DocumentError> {
        arguments
            .into_iter()
            .flat_map(|args| args.arguments())
            .map(|argument| {
                let name = self.name(argument.name(), argument.syntax(), "argument")?;
                let value = argument
                    .value()
                    .ok_or_else(|| self.incomplete(argument.syntax(), "argument value"))?;
                Ok(GraphQLArgument {
                    name,
                    value_type: value_type(&value).to_string(),
                    value_json: self.literal(&value)?,
                })
            })
            .collect()
    }

    fn directives(
        &self,
        directives: Option<cst::Directives>,
    ) -> Result<Vec<Directive>, DocumentError> {
        directives
            .into_iter()
            .flat_map(|ds| ds.directives())
            .map(|directive| {
                Ok(Directive {
                    name:      self.name(directive.name(), directive.syntax(), "directive")?,
                    arguments: self.arguments(directive.arguments())?,
                })
            })
            .collect()
    }

    fn literal(&self, value: &cst::Value) -> Result<String, DocumentError> {
        value_json::encode_literal(value)
            .map_err(|LiteralError { at, message }| self.at.error(at, &message))
    }
}

/// The kind of value an argument was written as.
const fn value_type(value: &cst::Value) -> &'static str {
    match value {
        cst::Value::StringValue(_) => "string",
        cst::Value::IntValue(_) => "int",
        cst::Value::FloatValue(_) => "float",
        cst::Value::BooleanValue(_) => "boolean",
        cst::Value::NullValue(_) => "null",
        cst::Value::EnumValue(_) => "enum",
        cst::Value::ListValue(_) => "list",
        cst::Value::ObjectValue(_) => "object",
        cst::Value::Variable(_) => "variable",
    }
}

/// The fragment names spread anywhere in `selections`, in order.
fn spreads(selections: &[FieldSelection]) -> Vec<String> {
    let mut names = Vec::new();
    for selection in selections {
        if let Some(name) = selection.name.strip_prefix("...") {
            if !name.starts_with("on ") {
                names.push(name.to_string());
            }
        }
        names.extend(spreads(&selection.nested_fields));
    }
    names
}
