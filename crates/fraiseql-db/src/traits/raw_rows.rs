//! Rows read as the bytes the database serialized, never parsed into a `Value`.

/// What a compact JSON text holds, counted per kind of token.
///
/// Enough to price the text the way a parsed value would be priced, without parsing it:
/// a string's length is its decoded length, as a `String` would hold it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JsonTally {
    /// Objects.
    pub objects:      u64,
    /// Members of every object.
    pub members:      u64,
    /// Bytes of every member's key, decoded.
    pub key_bytes:    u64,
    /// Arrays.
    pub arrays:       u64,
    /// Elements of every array.
    pub elements:     u64,
    /// String values.
    pub strings:      u64,
    /// Bytes of every string value, decoded.
    pub string_bytes: u64,
    /// Numbers.
    pub numbers:      u64,
    /// `null`, `true` and `false`.
    pub literals:     u64,
}

/// What compacting one JSON text found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compacted {
    /// The text's tokens.
    pub tally: JsonTally,
    /// Whether the compact text is byte for byte what `serde_json` writes for the value it
    /// parses to. It is not when a string carries a `\u` escape, or a number lies past the
    /// range of an `f64`, which `serde_json` refuses to parse.
    pub exact: bool,
    /// Whether the text holds a `null`.
    pub null:  bool,
}

/// Append `src`, a valid JSON text, to `out` without insignificant whitespace.
///
/// The bytes are copied, never parsed into a tree: the scan only tracks whether it is
/// inside a string, and counts tokens as it passes them. A string's contents are copied a
/// run at a time, up to its next quote or backslash. A number is written as `serde_json`
/// writes the value it parses to, so `1.50` becomes `1.5`: an integer `serde_json` keeps
/// is copied, and any other number is parsed and written again. `src` must be valid JSON, as the
/// database guarantees for a `json` or `jsonb` column; on anything else the output is
/// whatever the scan copied.
pub fn compact_json(src: &[u8], out: &mut Vec<u8>) -> Compacted {
    let mut found = Compacted {
        exact: true,
        ..Compacted::default()
    };
    // Per open container: whether it is an object.
    let mut open: Vec<bool> = Vec::new();
    // Where the number being copied starts, in `src` and in `out`.
    let mut number_start = None;
    out.reserve(src.len());
    let mut i = 0;
    while let Some(&b) = src.get(i) {
        if let Some((start, out_start)) = number_start {
            if matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                out.push(b);
                i += 1;
                continue;
            }
            found.exact &= write_number(&src[start..i], out, out_start);
            number_start = None;
        }
        let tally = &mut found.tally;
        match b {
            b' ' | b'\n' | b'\r' | b'\t' => {
                i += 1;
                continue;
            },
            b'"' => {
                // A string where an object expects a key is one; `:` follows it.
                let key = open.last() == Some(&true) && expects_key(out);
                if key {
                    tally.members += 1;
                } else {
                    count_element(&open, tally);
                }
                out.push(b'"');
                let (end, len) = copy_string(src, i + 1, out, &mut found.exact);
                let tally = &mut found.tally;
                if key {
                    tally.key_bytes += len;
                } else {
                    tally.strings += 1;
                    tally.string_bytes += len;
                }
                i = end;
                continue;
            },
            b'{' | b'[' => {
                count_element(&open, tally);
                if b == b'{' {
                    tally.objects += 1;
                } else {
                    tally.arrays += 1;
                }
                open.push(b == b'{');
            },
            b'}' | b']' => {
                open.pop();
            },
            b'n' | b't' | b'f' => {
                count_element(&open, tally);
                tally.literals += 1;
                found.null |= b == b'n';
                // The rest of the literal is copied below as plain bytes.
            },
            b'0'..=b'9' | b'-' => {
                count_element(&open, tally);
                tally.numbers += 1;
                number_start = Some((i, out.len()));
            },
            _ => {},
        }
        out.push(b);
        i += 1;
    }
    if let Some((start, out_start)) = number_start {
        found.exact &= write_number(&src[start..], out, out_start);
    }
    found
}

/// Copy a string's contents from `from`, just past its opening quote, through its closing
/// quote. Returns where the scan resumes and the string's decoded length; a `\u` escape
/// clears `exact`.
fn copy_string(src: &[u8], from: usize, out: &mut Vec<u8>, exact: &mut bool) -> (usize, u64) {
    let mut i = from;
    let mut len = 0u64;
    while let Some(run) = src.get(i..) {
        let Some(at) = memchr::memchr2(b'"', b'\\', run) else {
            // Unterminated: valid JSON never ends here.
            out.extend_from_slice(run);
            return (src.len(), len + run.len() as u64);
        };
        out.extend_from_slice(&run[..=at]);
        len += at as u64;
        i += at + 1;
        if run[at] == b'"' {
            return (i, len);
        }
        // An escape is two bytes for one decoded character, or six for `\uXXXX`.
        if let Some(&escaped) = src.get(i) {
            out.push(escaped);
            *exact &= escaped != b'u';
            len += 1;
            i += 1;
        }
    }
    (src.len(), len)
}

/// Whether the last significant byte written opens an object or ends a member, so the
/// string that follows is a key.
const fn expects_key(out: &[u8]) -> bool {
    matches!(out.last(), Some(b'{' | b','))
}

/// Count a value as an array element. A member's value adds nothing: the member is
/// counted at its key.
fn count_element(open: &[bool], tally: &mut JsonTally) {
    if open.last() == Some(&false) {
        tally.elements += 1;
    }
}

/// Write `token`, a number already copied to `out` from `at`, as `serde_json` writes the
/// value it parses to. Returns `false`, leaving the token as it is, when `serde_json` would
/// not parse it.
fn write_number(token: &[u8], out: &mut Vec<u8>, at: usize) -> bool {
    if exact_integer(token) {
        return true;
    }
    let Ok(number) = serde_json::from_slice::<serde_json::Number>(token) else {
        return false;
    };
    out.truncate(at);
    if serde_json::to_writer(&mut *out, &number).is_ok() {
        return true;
    }
    out.truncate(at);
    out.extend_from_slice(token);
    false
}

/// Whether a number token parses to an integer `serde_json` writes back unchanged.
fn exact_integer(token: &[u8]) -> bool {
    let digits = token.strip_prefix(b"-").unwrap_or(token);
    !digits.is_empty()
        && digits.len() <= 18
        && digits.iter().all(u8::is_ascii_digit)
        && token != b"-0"
}

/// Rows a read returned, each the database's own serialization, compacted and joined by
/// `,` with no enclosing brackets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawRows {
    bytes: Vec<u8>,
    rows:  Vec<RawRow>,
    exact: bool,
    null:  bool,
}

/// One row of a [`RawRows`]: where it ends, and its tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRow {
    /// One past the row's last byte.
    pub end:   usize,
    /// The row's tokens.
    pub tally: JsonTally,
}

impl RawRows {
    /// No rows.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            rows:  Vec::new(),
            exact: true,
            null:  false,
        }
    }

    /// Append one row, `src` a valid JSON text.
    pub fn push(&mut self, src: &[u8]) {
        if !self.rows.is_empty() {
            self.bytes.push(b',');
        }
        let found = compact_json(src, &mut self.bytes);
        self.exact &= found.exact;
        self.null |= found.null;
        self.rows.push(RawRow {
            end:   self.bytes.len(),
            tally: found.tally,
        });
    }

    /// The rows, joined by `,`.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Each row's end and tokens, in order.
    #[must_use]
    pub fn rows(&self) -> &[RawRow] {
        &self.rows
    }

    /// One row's bytes.
    #[must_use]
    pub fn row(&self, index: usize) -> Option<&[u8]> {
        let row = self.rows.get(index)?;
        let start = match index {
            0 => 0,
            _ => self.rows[index - 1].end + 1,
        };
        Some(&self.bytes[start..row.end])
    }

    /// Whether every row is byte for byte what `serde_json` writes for the value it
    /// parses to. See [`Compacted::exact`].
    #[must_use]
    pub const fn exact(&self) -> bool {
        self.exact
    }

    /// Whether any row holds a `null`.
    #[must_use]
    pub const fn has_null(&self) -> bool {
        self.null
    }

    /// The rows, parsed.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] when a row is not valid JSON.
    pub fn parse(&self) -> serde_json::Result<Vec<serde_json::Value>> {
        (0..self.rows.len())
            .map(|i| serde_json::from_slice(self.row(i).unwrap_or_default()))
            .collect()
    }
}

#[cfg(test)]
mod tests;
