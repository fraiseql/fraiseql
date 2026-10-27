//! A `Decimal`'s exact digits, kept from the text the database serialized.
//!
//! A `jsonb` number decoded into a `serde_json::Value` is an `f64`: `1.50` loses its scale,
//! and every digit past the seventeenth is rounded. A `Decimal` travels as a JSON string
//! carrying the stored digits instead, so a read that reaches one takes the row as text and
//! [`quote_decimals`] wraps each number token at a `Decimal` position in quotes before it
//! is parsed. Every other byte is copied as it is.

use std::{borrow::Cow, collections::HashMap, sync::Arc};

#[cfg(test)]
mod tests;

/// Where a `Decimal` sits in a document, as the schema types it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalNode {
    /// A `Decimal` value, or a list of them at any depth.
    Decimal,
    /// An object of the named type, or a list of them.
    Object(String),
    /// An object whose concrete type its `__typename` names, or a list of them; the named
    /// type is read when it names none.
    Abstract(Option<String>),
}

/// The keys of one object type that lead to a `Decimal`, each under every key the document
/// may hold it by: its response key and its stored key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecimalFields(HashMap<String, DecimalNode>);

impl DecimalFields {
    /// No `Decimal` reachable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `key` leads to `node`.
    pub fn insert(&mut self, key: impl Into<String>, node: DecimalNode) {
        self.0.insert(key.into(), node);
    }

    /// Whether no key leads to a `Decimal`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn get(&self, key: &str) -> Option<&DecimalNode> {
        self.0.get(key)
    }
}

/// Every `Decimal` a document of one type can hold: the root, and the fields of each type
/// below it that leads to one. A type that leads to none is absent, and is copied as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecimalShape {
    root:  DecimalNode,
    types: Arc<HashMap<String, DecimalFields>>,
}

impl DecimalShape {
    /// A document read as `root`, its types' fields in `types`.
    #[must_use]
    pub const fn new(root: DecimalNode, types: Arc<HashMap<String, DecimalFields>>) -> Self {
        Self { root, types }
    }

    fn fields(&self, type_name: &str) -> Option<&DecimalFields> {
        self.types.get(type_name)
    }
}

/// Objects nested deeper than this are copied without being looked into. A query's depth
/// limit is far below it; a stored document's depth is not bounded.
const MAX_OBJECT_DEPTH: usize = 128;

/// Lists nested deeper than this below an object position are copied without being looked
/// into: a schema's list depth is far below it, and a stored document's is not bounded.
const MAX_LIST_DEPTH: usize = 32;

/// Append `src`, a valid JSON text, to `out` with every number at a `Decimal` position of
/// `shape` written as a string of the same digits.
///
/// A `Decimal` stored as a string, or `null`, is copied as it is. A key `node` does not name
/// is copied without being looked into. `src` must be valid JSON, as the database guarantees
/// for a `json` or `jsonb` column.
pub fn quote_decimals(src: &[u8], shape: &DecimalShape, out: &mut Vec<u8>) {
    out.reserve(src.len() + 16);
    let end = Scan {
        src,
        out,
        shape,
        objects: 0,
    }
    .value(skip_ws(src, 0), Some(&shape.root), 0);
    out.extend_from_slice(src.get(end..).unwrap_or_default());
}

/// Copies `src` to `out` value by value, quoting `Decimal` numbers on the way.
struct Scan<'s, 'o> {
    src:     &'s [u8],
    out:     &'o mut Vec<u8>,
    shape:   &'s DecimalShape,
    /// Objects open above the current value.
    objects: usize,
}

impl<'s> Scan<'s, '_> {
    /// Copy the value at `at` and return where it ends.
    fn value(&mut self, at: usize, node: Option<&DecimalNode>, lists: usize) -> usize {
        match (self.src.get(at), node) {
            (Some(_), Some(DecimalNode::Decimal)) => self.decimal(at),
            (Some(b'['), Some(node)) if lists < MAX_LIST_DEPTH => self.list(at, node, lists + 1),
            (Some(b'{'), Some(DecimalNode::Object(type_name))) => {
                self.object(at, self.shape.fields(type_name))
            },
            (Some(b'{'), Some(DecimalNode::Abstract(fallback))) => {
                let fields = match concrete_type(self.src, at) {
                    Some(name) => self.shape.fields(&name),
                    None => fallback.as_deref().and_then(|name| self.shape.fields(name)),
                };
                self.object(at, fields)
            },
            _ => self.copy(at),
        }
    }

    /// Copy the value at `at` unchanged.
    fn copy(&mut self, at: usize) -> usize {
        let end = end_of_value(self.src, at);
        self.out.extend_from_slice(&self.src[at..end]);
        end
    }

    /// Copy the value at a `Decimal` position, quoting every number in it: a number, or a
    /// list of them at any depth.
    fn decimal(&mut self, at: usize) -> usize {
        let end = end_of_value(self.src, at);
        let mut i = at;
        while i < end {
            match self.src[i] {
                b'"' => {
                    let close = end_of_string(self.src, i);
                    self.out.extend_from_slice(&self.src[i..close]);
                    i = close;
                },
                b'-' | b'0'..=b'9' => {
                    let close = end_of_scalar(self.src, i);
                    self.out.push(b'"');
                    self.out.extend_from_slice(&self.src[i..close]);
                    self.out.push(b'"');
                    i = close;
                },
                b => {
                    self.out.push(b);
                    i += 1;
                },
            }
        }
        end
    }

    fn list(&mut self, at: usize, node: &DecimalNode, lists: usize) -> usize {
        self.out.push(b'[');
        let mut i = at + 1;
        loop {
            i = self.whitespace(i);
            match self.src.get(i) {
                Some(b']') => {
                    self.out.push(b']');
                    return i + 1;
                },
                Some(b',') => {
                    self.out.push(b',');
                    i += 1;
                },
                Some(_) => i = self.value(i, Some(node), lists),
                None => return i,
            }
        }
    }

    fn object(&mut self, at: usize, fields: Option<&'s DecimalFields>) -> usize {
        let Some(fields) = fields.filter(|_| self.objects < MAX_OBJECT_DEPTH) else {
            return self.copy(at);
        };
        self.objects += 1;
        let end = self.members(at, fields);
        self.objects -= 1;
        end
    }

    fn members(&mut self, at: usize, fields: &'s DecimalFields) -> usize {
        self.out.push(b'{');
        let mut i = at + 1;
        loop {
            i = self.whitespace(i);
            match self.src.get(i) {
                Some(b'}') => {
                    self.out.push(b'}');
                    return i + 1;
                },
                Some(b',') => {
                    self.out.push(b',');
                    i += 1;
                },
                Some(b'"') => {
                    let close = end_of_string(self.src, i);
                    self.out.extend_from_slice(&self.src[i..close]);
                    let key = string_text(&self.src[i..close]);
                    i = self.whitespace(close);
                    if self.src.get(i) == Some(&b':') {
                        self.out.push(b':');
                        i = self.whitespace(i + 1);
                    }
                    i = self.value(i, key.as_deref().and_then(|k| fields.get(k)), 0);
                },
                Some(_) => return self.copy_rest(i),
                None => return i,
            }
        }
    }

    /// Copy the whitespace at `at` and return where it ends.
    fn whitespace(&mut self, at: usize) -> usize {
        let end = skip_ws(self.src, at);
        self.out.extend_from_slice(&self.src[at..end]);
        end
    }

    /// Copy everything left: the text is not JSON there, so nothing more is quoted.
    fn copy_rest(&mut self, at: usize) -> usize {
        self.out.extend_from_slice(&self.src[at..]);
        self.src.len()
    }
}

/// The type an object's top-level `__typename` member names, wherever it sits among the
/// members.
fn concrete_type(src: &[u8], at: usize) -> Option<Cow<'_, str>> {
    let mut i = at + 1;
    loop {
        i = skip_ws(src, i);
        match src.get(i)? {
            b',' => i += 1,
            b'"' => {
                let close = end_of_string(src, i);
                let is_typename = string_text(&src[i..close]).as_deref() == Some("__typename");
                i = skip_ws(src, close);
                if src.get(i) == Some(&b':') {
                    i = skip_ws(src, i + 1);
                }
                let end = end_of_value(src, i);
                if is_typename {
                    return string_text(&src[i..end]);
                }
                i = end;
            },
            _ => return None,
        }
    }
}

/// The decoded text of a JSON string token, quotes included in `token`. `None` when it is
/// not a string.
fn string_text(token: &[u8]) -> Option<Cow<'_, str>> {
    let inner = token.strip_prefix(b"\"").and_then(|t| t.strip_suffix(b"\""))?;
    if inner.contains(&b'\\') {
        serde_json::from_slice::<String>(token).ok().map(Cow::Owned)
    } else {
        std::str::from_utf8(inner).ok().map(Cow::Borrowed)
    }
}

fn skip_ws(src: &[u8], mut i: usize) -> usize {
    while matches!(src.get(i), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        i += 1;
    }
    i
}

/// One past the string token opening at `at`.
fn end_of_string(src: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while let Some(&b) = src.get(i) {
        match b {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    src.len()
}

/// One past the number or literal at `at`.
fn end_of_scalar(src: &[u8], at: usize) -> usize {
    let mut i = at;
    while matches!(src.get(i), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' | b'a'..=b'z')) {
        i += 1;
    }
    i.max(at + 1).min(src.len())
}

/// One past the value at `at`, found without recursion whatever its depth.
fn end_of_value(src: &[u8], at: usize) -> usize {
    match src.get(at) {
        Some(b'"') => end_of_string(src, at),
        Some(b'{' | b'[') => {
            let mut depth = 0_usize;
            let mut i = at;
            while let Some(&b) = src.get(i) {
                match b {
                    b'"' => {
                        i = end_of_string(src, i);
                        continue;
                    },
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    },
                    _ => {},
                }
                i += 1;
            }
            src.len()
        },
        Some(_) => end_of_scalar(src, at),
        None => at,
    }
}
