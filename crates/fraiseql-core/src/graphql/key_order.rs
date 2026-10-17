//! The key order an operation's object literals were written in (#1567).
//!
//! `graphql-parser` stores an object literal as a `BTreeMap`, so `{ratio: ASC, current:
//! DESC}` reaches [`value_json`](super::value_json) as `{current, ratio}`. Where the order
//! carries meaning, a multi-key `orderBy`, the literal was applied alphabetically while the
//! same object passed as a variable (JSON, which keeps its order) was applied as written.
//!
//! The parser keeps the order of everything else (definitions, selections, arguments,
//! directives), and the positions it records count columns by its own rules (a tab is eight),
//! so this reads the order from the source text instead: a second, structural pass over the
//! executable document that mirrors the parser's tree, recording each argument value's
//! shape. The two trees are walked in lockstep, so a value's shape is found by where it sits,
//! never by matching keys.
//!
//! The pass runs on a document the parser has already accepted, so it only has to follow the
//! grammar, not adjudicate it. A document it cannot follow is reported, never served with the
//! alphabetical order: a silently reordered sort answers with other rows.

/// How a value was written: the order of an object's keys, recursively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// A scalar, an enum, `null` or a variable reference.
    Leaf,
    /// A list, item by item.
    List(Vec<Shape>),
    /// An object, its keys in the order written.
    Object(Vec<(String, Shape)>),
}

/// The values written in one selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionShapes {
    /// Its arguments' values, in order (a field's; none for a fragment).
    pub arguments:  Vec<Shape>,
    /// Each directive's argument values, in order.
    pub directives: Vec<Vec<Shape>>,
    /// Its sub-selections (a field's or an inline fragment's).
    pub selection:  Vec<SelectionShapes>,
}

/// The values written in one definition (an operation or a fragment).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefinitionShapes {
    /// Each variable's default value, in order.
    pub variable_defaults: Vec<Option<Shape>>,
    /// Its selection set.
    pub selection:         Vec<SelectionShapes>,
}

/// Read the shapes of every definition of `source`, in document order.
///
/// # Errors
///
/// A description of where the source departs from the grammar.
pub fn document_shapes(source: &str) -> Result<Vec<DefinitionShapes>, String> {
    let tokens = lex(source)?;
    let mut reader = Reader { tokens, at: 0 };
    let mut definitions = Vec::new();
    while reader.peek().is_some() {
        definitions.push(reader.definition()?);
    }
    Ok(definitions)
}

/// One token of an executable document. Literals other than names are opaque: only their
/// extent matters here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'s> {
    /// `{ } ( ) [ ] : = ! $ @ |` and `...`.
    Punct(&'s str),
    Name(&'s str),
    /// A number or a string.
    Literal,
}

fn lex(source: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | b',' => i += 1,
            // The UTF-8 byte order mark, which the grammar ignores.
            0xEF if source[i..].starts_with('\u{feff}') => i += '\u{feff}'.len_utf8(),
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
            },
            b'{' | b'}' | b'(' | b')' | b'[' | b']' | b':' | b'=' | b'!' | b'$' | b'@' | b'|'
            | b'&' => {
                tokens.push(Token::Punct(&source[i..=i]));
                i += 1;
            },
            b'.' if source[i..].starts_with("...") => {
                tokens.push(Token::Punct("..."));
                i += 3;
            },
            b'_' | b'a'..=b'z' | b'A'..=b'Z' => {
                let start = i;
                while i < bytes.len() && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                tokens.push(Token::Name(&source[start..i]));
            },
            b'-' | b'+' | b'.' | b'0'..=b'9' => {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'.' | b'-' | b'+'))
                {
                    i += 1;
                }
                tokens.push(Token::Literal);
            },
            b'"' if source[i..].starts_with("\"\"\"") => {
                // A block string ends at the first `"""` not escaped as `\"""`.
                let mut j = i + 3;
                loop {
                    let Some(k) = source[j..].find("\"\"\"") else {
                        return Err("an unterminated block string".to_string());
                    };
                    let end = j + k;
                    if source[..end].ends_with('\\') {
                        j = end + 3;
                    } else {
                        i = end + 3;
                        break;
                    }
                }
                tokens.push(Token::Literal);
            },
            b'"' => {
                i += 1;
                loop {
                    match bytes.get(i) {
                        None | Some(b'\n') => return Err("an unterminated string".to_string()),
                        Some(b'\\') => i += 2,
                        Some(b'"') => {
                            i += 1;
                            break;
                        },
                        Some(_) => i += 1,
                    }
                }
                tokens.push(Token::Literal);
            },
            _ => {
                // Any other character is inside no token the grammar allows; the parser has
                // already refused such a document.
                return Err(format!("an unexpected character at byte {i}"));
            },
        }
    }
    Ok(tokens)
}

struct Reader<'s> {
    tokens: Vec<Token<'s>>,
    at:     usize,
}

impl<'s> Reader<'s> {
    fn peek(&self) -> Option<Token<'s>> {
        self.tokens.get(self.at).copied()
    }

    fn peek_second(&self) -> Option<Token<'s>> {
        self.tokens.get(self.at + 1).copied()
    }

    fn next(&mut self) -> Result<Token<'s>, String> {
        let token = self.peek().ok_or_else(|| "an unexpected end of the document".to_string())?;
        self.at += 1;
        Ok(token)
    }

    /// Consume `punct` if it is next.
    fn eat(&mut self, punct: &str) -> bool {
        if self.peek() == Some(Token::Punct(punct)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, punct: &str) -> Result<(), String> {
        if self.eat(punct) {
            Ok(())
        } else {
            Err(format!("`{punct}` expected, found {:?}", self.peek()))
        }
    }

    fn name(&mut self) -> Result<&'s str, String> {
        match self.next()? {
            Token::Name(name) => Ok(name),
            other => Err(format!("a name expected, found {other:?}")),
        }
    }

    fn definition(&mut self) -> Result<DefinitionShapes, String> {
        if self.peek() == Some(Token::Punct("{")) {
            return Ok(DefinitionShapes {
                variable_defaults: Vec::new(),
                selection:         self.selection_set()?,
            });
        }
        match self.name()? {
            "fragment" => {
                self.name()?;
                if self.name()? != "on" {
                    return Err("`on` expected after a fragment's name".to_string());
                }
                self.name()?;
                self.directives()?;
                Ok(DefinitionShapes {
                    variable_defaults: Vec::new(),
                    selection:         self.selection_set()?,
                })
            },
            "query" | "mutation" | "subscription" => {
                if matches!(self.peek(), Some(Token::Name(_))) {
                    self.name()?;
                }
                let variable_defaults = self.variable_definitions()?;
                self.directives()?;
                Ok(DefinitionShapes {
                    variable_defaults,
                    selection: self.selection_set()?,
                })
            },
            other => Err(format!("a definition expected, found `{other}`")),
        }
    }

    fn variable_definitions(&mut self) -> Result<Vec<Option<Shape>>, String> {
        let mut defaults = Vec::new();
        if !self.eat("(") {
            return Ok(defaults);
        }
        while !self.eat(")") {
            self.expect("$")?;
            self.name()?;
            self.expect(":")?;
            self.skip_type()?;
            defaults.push(if self.eat("=") {
                Some(self.value()?)
            } else {
                None
            });
            self.directives()?;
        }
        Ok(defaults)
    }

    fn skip_type(&mut self) -> Result<(), String> {
        if self.eat("[") {
            self.skip_type()?;
            self.expect("]")?;
        } else {
            self.name()?;
        }
        self.eat("!");
        Ok(())
    }

    fn directives(&mut self) -> Result<Vec<Vec<Shape>>, String> {
        let mut directives = Vec::new();
        while self.eat("@") {
            self.name()?;
            directives.push(self.arguments()?);
        }
        Ok(directives)
    }

    fn arguments(&mut self) -> Result<Vec<Shape>, String> {
        let mut values = Vec::new();
        if !self.eat("(") {
            return Ok(values);
        }
        while !self.eat(")") {
            self.name()?;
            self.expect(":")?;
            values.push(self.value()?);
        }
        Ok(values)
    }

    fn selection_set(&mut self) -> Result<Vec<SelectionShapes>, String> {
        self.expect("{")?;
        let mut selections = Vec::new();
        while !self.eat("}") {
            selections.push(self.selection()?);
        }
        Ok(selections)
    }

    fn selection(&mut self) -> Result<SelectionShapes, String> {
        if self.eat("...") {
            // `... on T`, `... @d { }`, `... { }`: an inline fragment. `...Name`: a spread.
            let inline = match (self.peek(), self.peek_second()) {
                (Some(Token::Name("on")), Some(Token::Name(_))) => {
                    self.at += 2;
                    true
                },
                (Some(Token::Name(_)), _) => {
                    self.name()?;
                    false
                },
                _ => true,
            };
            let directives = self.directives()?;
            let selection = if inline {
                self.selection_set()?
            } else {
                Vec::new()
            };
            return Ok(SelectionShapes {
                arguments: Vec::new(),
                directives,
                selection,
            });
        }
        self.name()?;
        if self.eat(":") {
            self.name()?;
        }
        let arguments = self.arguments()?;
        let directives = self.directives()?;
        let selection = if self.peek() == Some(Token::Punct("{")) {
            self.selection_set()?
        } else {
            Vec::new()
        };
        Ok(SelectionShapes {
            arguments,
            directives,
            selection,
        })
    }

    fn value(&mut self) -> Result<Shape, String> {
        match self.next()? {
            Token::Punct("$") => {
                self.name()?;
                Ok(Shape::Leaf)
            },
            Token::Name(_) | Token::Literal => Ok(Shape::Leaf),
            Token::Punct("[") => {
                let mut items = Vec::new();
                while !self.eat("]") {
                    items.push(self.value()?);
                }
                Ok(Shape::List(items))
            },
            Token::Punct("{") => {
                let mut fields = Vec::new();
                while !self.eat("}") {
                    let key = self.name()?.to_string();
                    self.expect(":")?;
                    fields.push((key, self.value()?));
                }
                Ok(Shape::Object(fields))
            },
            other @ Token::Punct(_) => Err(format!("a value expected, found {other:?}")),
        }
    }
}

#[cfg(test)]
#[path = "key_order_tests.rs"]
mod key_order_tests;
