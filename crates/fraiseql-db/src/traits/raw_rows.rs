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
    compact::<false>(src, out, None)
}

/// Where values of one kind sit in compacted text: under which member keys, each once,
/// and whether one sits where no key names it, an array element or the text itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Keys {
    /// The distinct keys such a value is the value of, as ranges of the compacted text.
    keyed:   Vec<std::ops::Range<usize>>,
    /// Such a value that is no member's value.
    unkeyed: bool,
}

/// The kinds of JSON value a built-in scalar position holds as served (GraphQL § 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    /// An `Int`: a number written as a signed 32-bit integer.
    Int,
    /// A `Float`: a number.
    Float,
    /// A `String`: a string.
    String,
    /// A `Boolean`: `true` or `false`.
    Boolean,
}

impl ScalarKind {
    /// The kind whose discriminant is `byte`, as [`Told`] stores it.
    const fn from_u8(byte: u8) -> Self {
        match byte {
            0 => Self::Int,
            1 => Self::Float,
            2 => Self::String,
            _ => Self::Boolean,
        }
    }
}

/// A member value's JSON kind, other than `null`, a number told apart by whether an `Int`
/// holds it as written. Its discriminant is its bit in a mask of kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    String,
    Boolean,
    Container,
    /// A number written as a signed 32-bit integer.
    Int,
    /// Any other number.
    Number,
}

impl ValueKind {
    const ALL: [Self; 5] = [
        Self::String,
        Self::Boolean,
        Self::Container,
        Self::Int,
        Self::Number,
    ];

    /// This kind's bit in a mask of kinds.
    const fn bit(self) -> u8 {
        1 << self as u8
    }

    /// The kind of the number written `token`.
    fn of_number(token: &[u8]) -> Self {
        if holds_an_int(token) {
            Self::Int
        } else {
            Self::Number
        }
    }
}

/// Whether a value of `kind` is one `scalar` does not hold as served (§ 3.5): for an `Int`,
/// anything but a number written as a signed 32-bit integer; for a `Float`, anything but a
/// number; for a `String`, anything but a string; for a `Boolean`, anything but `true` or
/// `false`.
const fn breaks(scalar: ScalarKind, kind: ValueKind) -> bool {
    !matches!(
        (scalar, kind),
        (ScalarKind::Int, ValueKind::Int)
            | (ScalarKind::Float, ValueKind::Int | ValueKind::Number)
            | (ScalarKind::String, ValueKind::String)
            | (ScalarKind::Boolean, ValueKind::Boolean)
    )
}

/// The kinds of value `scalar` refuses ([`breaks`]), one bit per [`ValueKind`].
fn refused_by(scalar: ScalarKind) -> u8 {
    ValueKind::ALL
        .into_iter()
        .filter(|&kind| breaks(scalar, kind))
        .fold(0, |refused, kind| refused | kind.bit())
}

/// The kinds of value refused by the scalar of some of `scalar_keys`.
fn refused_by_any(scalar_keys: &[(&[u8], ScalarKind)]) -> u8 {
    scalar_keys.iter().fold(0, |refused, &(_, scalar)| refused | refused_by(scalar))
}

/// The response keys of the built-in scalar fields a read selects, with their kinds, as its
/// reader named them ([`RawRows::with_scalar_keys`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Told {
    /// Per key, in the order named: its kind, its length (4 bytes, little-endian), its
    /// bytes. One allocation, whatever the number of keys.
    named:   Vec<u8>,
    /// Per [`tag`], the kinds of value refused by the scalar of some key with that tag, one
    /// bit per [`ValueKind`]: a value under a key, holding no escape, whose tag has no bit
    /// for its kind is refused by none, unread further.
    by_tag:  [u8; TAGS],
    /// The kinds of value refused by the scalar of some key: what a key holding an escape,
    /// which may decode to any of them, may refuse.
    refused: u8,
}

/// The bits of a [`tag`].
const TAG_BITS: u32 = 6;

/// How many values [`tag`] takes.
const TAGS: usize = 1 << TAG_BITS;

/// No keys: what `compact::<false>` holds its never-taken told steps to.
static UNTOLD: Told = Told {
    named:   Vec::new(),
    by_tag:  [0; TAGS],
    refused: 0,
};

impl Told {
    fn new(scalar_keys: &[(&[u8], ScalarKind)]) -> Self {
        let mut told = Self {
            named:   Vec::with_capacity(scalar_keys.iter().map(|(key, _)| key.len() + 5).sum()),
            by_tag:  [0; TAGS],
            refused: 0,
        };
        for &(key, scalar) in scalar_keys {
            told.named.push(scalar as u8);
            told.named
                .extend_from_slice(&u32::try_from(key.len()).unwrap_or(u32::MAX).to_le_bytes());
            told.named.extend_from_slice(key);
            let refused = refused_by(scalar);
            told.by_tag[usize::from(tag(key))] |= refused;
            told.refused |= refused;
        }
        told
    }

    /// The keys and their kinds, in the order named.
    fn iter(&self) -> impl Iterator<Item = (&[u8], ScalarKind)> {
        let mut rest = self.named.as_slice();
        std::iter::from_fn(move || {
            let (&[scalar, a, b, c, d], tail) = rest.split_first_chunk::<5>()?;
            let (key, tail) = tail.split_at(u32::from_le_bytes([a, b, c, d]) as usize);
            rest = tail;
            Some((key, ScalarKind::from_u8(scalar)))
        })
    }

    /// The kinds of value `key`, holding an escape when `escaped`, may be a key refusing:
    /// by its [`tag`] when it holds none; any key's when it does.
    #[inline]
    const fn may_refuse(&self, key: &[u8], escaped: bool) -> u8 {
        if escaped {
            self.refused
        } else {
            self.by_tag[tag(key) as usize]
        }
    }

    /// Whether a value of `kind` under `key`, as written, which may refuse it, is one a
    /// scalar that key is named as refuses. A key holding an escape decodes to one
    /// starting with its first byte, or, when that byte opens the escape, to any: it counts
    /// as each named key it may decode to.
    #[cold]
    fn refuses(&self, kind: ValueKind, key: &[u8]) -> bool {
        let escaped = memchr::memchr(b'\\', key).is_some();
        self.iter().any(|(named, scalar)| {
            breaks(scalar, kind)
                && if escaped {
                    key.first() == Some(&b'\\') || named.first() == key.first()
                } else {
                    named == key
                }
        })
    }
}

/// A key's first and last bytes and its length, folded into one of [`TAGS`] values: keys
/// with different tags differ.
const fn tag(key: &[u8]) -> u8 {
    let first = match key.first() {
        Some(&first) => first,
        None => 0,
    };
    let last = match key.last() {
        Some(&last) => last,
        None => 0,
    };
    (first ^ last.rotate_left(4)).wrapping_add(key.len().to_le_bytes()[0].wrapping_mul(157))
        & ((1 << TAG_BITS) - 1)
}

/// The values of compacted text a reader may have to look at before serving it as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Marks {
    /// Where each `null` sits.
    nulls:      Keys,
    /// Whether a member value sits under a key [`told`](Self::told) names whose scalar
    /// does not hold it as served, or under a key holding an escape that may decode to one.
    wrong_kind: bool,
    /// The keys the reader named, when it did: each member value is then held to them as
    /// the rows are copied. Without them nothing is noted, and the question is answered by
    /// reading the rows.
    told:       Option<Told>,
    /// The kinds of value the rows hold anywhere below a row's own, one bit per
    /// [`ValueKind`], a number counting as both kinds of number: under keys whose scalars
    /// refuse none of them, no value is of another kind.
    present:    u8,
}

/// The told keys while one row is copied by `compact::<true>`: what the last key copied
/// may refuse, and whether a value was found refused.
struct Noting<'t> {
    told:        &'t Told,
    /// The kinds of value the last key copied may be a key refusing
    /// ([`Told::may_refuse`]).
    key_refuses: u8,
    /// Where the last key copied sits in the text, between its quotes.
    key:         (usize, usize),
    wrong_kind:  bool,
}

impl Noting<'_> {
    /// Note the key just copied to `out[at..end]`, written in `end - at` bytes for `len`
    /// decoded: more when it holds an escape.
    #[inline]
    fn key(&mut self, out: &[u8], at: usize, end: usize, len: u64) {
        let written = out.get(at..end).unwrap_or_default();
        self.key_refuses = self.told.may_refuse(written, written.len() as u64 != len);
        self.key = (at, end);
    }

    /// Note a value of `kind` about to follow `out`: in an object, it is the value of the
    /// last key copied, and is held to the scalars that key is named as. A number's kind is
    /// told from its `token` only when the key may refuse a number.
    #[inline]
    fn value(&mut self, in_object: bool, out: &[u8], kind: ValueKind, token: &[u8]) {
        // Once one value is noted, whether another is changes nothing. Whatever an `Int`
        // refuses, a number no `Int` holds is refused by too.
        if self.wrong_kind || !in_object || self.key_refuses & kind.bit() == 0 {
            return;
        }
        let kind = if kind == ValueKind::Number {
            ValueKind::of_number(token)
        } else {
            kind
        };
        if self.key_refuses & kind.bit() != 0 {
            let key = out.get(self.key.0..self.key.1).unwrap_or_default();
            self.wrong_kind = self.told.refuses(kind, key);
        }
    }
}

impl Keys {
    /// Note a value under the key at `key` of `out`, or under none.
    fn note(&mut self, out: &[u8], key: Option<std::ops::Range<usize>>) {
        match key {
            Some(key) => {
                if !self.keyed.iter().any(|seen| out[seen.clone()] == out[key.clone()]) {
                    self.keyed.push(key);
                }
            },
            None => self.unkeyed = true,
        }
    }

    /// Whether `at` holds of a noted key of `text`, or of `None` when one was noted under
    /// no key.
    fn any_where(&self, text: &[u8], at: impl Fn(Option<&[u8]>) -> bool) -> bool {
        (self.unkeyed && at(None)) || self.keyed.iter().any(|key| at(Some(&text[key.clone()])))
    }
}

/// [`compact_json`], noting in `marks` where each `null` sits, and, when `TOLD`, the keys
/// [`Marks::told`] names: the copy without them holds no step for them.
fn compact<const TOLD: bool>(
    src: &[u8],
    out: &mut Vec<u8>,
    mut marks: Option<&mut Marks>,
) -> Compacted {
    let mut found = Compacted {
        exact: true,
        ..Compacted::default()
    };
    // Per open container: whether it is an object.
    let mut open: Vec<bool> = Vec::new();
    // Where the number being copied starts, in `src` and in `out`.
    let mut number_start = None;
    // Taken for the row, so the copy holds it apart from the rest of `marks`.
    let told = if TOLD {
        marks.as_deref_mut().and_then(|marks| marks.told.take())
    } else {
        None
    };
    let mut noting = Noting {
        told:        told.as_ref().unwrap_or(&UNTOLD),
        key_refuses: 0,
        key:         (0, 0),
        wrong_kind:  marks.as_deref().is_some_and(|marks| marks.wrong_kind),
    };
    let mut booleans = false;
    out.reserve(src.len());
    let mut i = 0;
    while let Some(&b) = src.get(i) {
        if let Some((start, out_start)) = number_start {
            if matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                out.push(b);
                i += 1;
                continue;
            }
            if TOLD {
                let in_object = open.last() == Some(&true);
                noting.value(in_object, &out[..out_start], ValueKind::Number, &src[start..i]);
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
                let in_object = open.last() == Some(&true);
                let key = in_object && expects_key(out);
                if key {
                    tally.members += 1;
                } else {
                    count_element(&open, tally);
                    if TOLD {
                        noting.value(in_object, out, ValueKind::String, &[]);
                    }
                }
                out.push(b'"');
                let at = out.len();
                let (end, len) = copy_string(src, i + 1, out, &mut found.exact);
                if TOLD && key {
                    // Through its closing quote; valid JSON never leaves one unterminated.
                    noting.key(out, at, out.len().saturating_sub(1).max(at), len);
                }
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
                if TOLD {
                    noting.value(open.last() == Some(&true), out, ValueKind::Container, &[]);
                }
                open.push(b == b'{');
            },
            b'}' | b']' => {
                open.pop();
            },
            b'n' | b't' | b'f' => {
                count_element(&open, tally);
                tally.literals += 1;
                if b == b'n' {
                    found.null = true;
                    if let Some(marks) = marks.as_deref_mut() {
                        // In an object, a literal is a member's value: the key just written.
                        let key = (open.last() == Some(&true)).then(|| member_key(out)).flatten();
                        marks.nulls.note(out, key);
                    }
                } else {
                    booleans = true;
                    if TOLD {
                        noting.value(open.last() == Some(&true), out, ValueKind::Boolean, &[]);
                    }
                }
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
    // A number running to the end of the text is the text itself, no member's value.
    if let Some((start, out_start)) = number_start {
        found.exact &= write_number(&src[start..], out, out_start);
    }
    if let Some(marks) = marks {
        marks.wrong_kind = noting.wrong_kind;
        if booleans {
            marks.present |= ValueKind::Boolean.bit();
        }
        if told.is_some() {
            marks.told = told;
        }
    }
    found
}

/// Whether a number token is a signed 32-bit integer written as one: digits, an optional
/// `-`, no fraction or exponent, and not `-0`, which `serde_json` reads as a float.
fn holds_an_int(token: &[u8]) -> bool {
    let digits = token.strip_prefix(b"-").unwrap_or(token);
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) || token == b"-0" {
        return false;
    }
    // Nine digits always fit; ten may; eleven never do.
    match digits.len() {
        0..=9 => true,
        10 => std::str::from_utf8(token).is_ok_and(|text| text.parse::<i32>().is_ok()),
        _ => false,
    }
}

/// The key of the member whose value `out` is about to receive, as the range of `out`
/// between its quotes: `out` ends `"key":`, compacted. A key's opening quote follows the
/// `{` or `,` before it; a quote inside a key is escaped, so `None` when the quote found
/// follows anything else: the key holds one, and is not placed.
fn member_key(out: &[u8]) -> Option<std::ops::Range<usize>> {
    let close = out.len().checked_sub(2)?;
    let open = memchr::memrchr(b'"', &out[..close])?;
    matches!(open.checked_sub(1).map(|before| out[before]), Some(b'{' | b','))
        .then_some(open + 1..close)
}

/// Copy a string's contents from `from`, just past its opening quote, through its closing
/// quote. Returns where the scan resumes and the string's decoded length; a `\u` escape
/// clears `exact`.
#[allow(clippy::inline_always)]
// Reason: once per string of every row; out of line, the call costs more than the copy of a short
// string, and `compact` has two instances the inliner otherwise declines.
#[inline(always)]
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

/// Whether compact JSON `text` holds, under one of `keys`, a value its scalar does not hold
/// as served (§ 3.5): for an `Int`, anything but a number written as a signed 32-bit integer;
/// for a `Float`, anything but a number; for a `String`, anything but a string; for a
/// `Boolean`, anything but `true` or `false`. `null` is no kind's concern here. A key named
/// by several scalars is held to each. A key holding an escape may decode to any of `keys`
/// starting with its first byte, or to any when that byte opens the escape: a value one of
/// those refuses counts. A value under any other key, or in an array, is passed over.
fn wrong_kind_under(text: &[u8], keys: &[(&[u8], ScalarKind)]) -> bool {
    // Per open container: whether it is an object.
    let mut open: Vec<bool> = Vec::new();
    // The last key read, and whether it held an escape.
    let mut key = (0..0, false);
    // Whether a value of `kind` breaks a scalar the current key may be selected as.
    let breaks = |(range, escaped): &(std::ops::Range<usize>, bool), kind: ValueKind| {
        let read = &text[range.clone()];
        keys.iter().filter(|&&(_, scalar)| breaks(scalar, kind)).any(|(k, _)| {
            if *escaped {
                read.first() == Some(&b'\\') || k.first() == read.first()
            } else {
                *k == read
            }
        })
    };
    let mut i = 0;
    while let Some(&b) = text.get(i) {
        let member = open.last() == Some(&true);
        match b {
            b'"' => {
                let start = i + 1;
                let mut end = start;
                let mut escaped = false;
                while let Some(at) =
                    text.get(end..).and_then(|rest| memchr::memchr2(b'"', b'\\', rest))
                {
                    end += at;
                    if text[end] == b'"' {
                        break;
                    }
                    escaped = true;
                    end += 2;
                }
                // A string where an object expects a key is one: `{` or `,` precedes it.
                if member
                    && matches!(i.checked_sub(1).map(|before| text[before]), Some(b'{' | b','))
                {
                    key = (start..end, escaped);
                } else if member && breaks(&key, ValueKind::String) {
                    return true;
                }
                i = end + 1;
            },
            b'{' | b'[' => {
                if member && breaks(&key, ValueKind::Container) {
                    return true;
                }
                open.push(b == b'{');
                i += 1;
            },
            b'}' | b']' => {
                open.pop();
                i += 1;
            },
            b't' | b'f' => {
                if member && breaks(&key, ValueKind::Boolean) {
                    return true;
                }
                i += if b == b't' { 4 } else { 5 };
            },
            b'0'..=b'9' | b'-' => {
                let end = text[i..]
                    .iter()
                    .position(|c| !matches!(c, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                    .map_or(text.len(), |len| i + len);
                if member && breaks(&key, ValueKind::of_number(&text[i..end])) {
                    return true;
                }
                i = end;
            },
            _ => i += 1,
        }
    }
    false
}

/// Rows a read returned, each the database's own serialization, compacted and joined by
/// `,` with no enclosing brackets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawRows {
    bytes: Vec<u8>,
    rows:  Vec<RawRow>,
    exact: bool,
    marks: Marks,
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
            marks: Marks {
                nulls:      Keys {
                    keyed:   Vec::new(),
                    unkeyed: false,
                },
                wrong_kind: false,
                told:       None,
                present:    0,
            },
        }
    }

    /// No rows, to be read through a selection whose built-in scalar fields are
    /// `scalar_keys`, by response key at any depth, with their kinds:
    /// [`has_wrong_kind_under`](Self::has_wrong_kind_under) asked of the same keys is then
    /// answered as the rows are copied, and the rows are not read again.
    #[must_use]
    pub fn with_scalar_keys(scalar_keys: &[(&[u8], ScalarKind)]) -> Self {
        let mut rows = Self::new();
        rows.marks.told = Some(Told::new(scalar_keys));
        rows
    }

    /// Append one row, `src` a valid JSON text.
    pub fn push(&mut self, src: &[u8]) {
        if !self.rows.is_empty() {
            self.bytes.push(b',');
        }
        let found = if self.marks.told.is_some() {
            compact::<true>(src, &mut self.bytes, Some(&mut self.marks))
        } else {
            compact::<false>(src, &mut self.bytes, Some(&mut self.marks))
        };
        self.exact &= found.exact;
        let tally = &found.tally;
        if tally.strings > 0 {
            self.marks.present |= ValueKind::String.bit();
        }
        if tally.numbers > 0 {
            self.marks.present |= ValueKind::Int.bit() | ValueKind::Number.bit();
        }
        // The row itself is one container; any other is a value below it.
        if tally.objects + tally.arrays > 1 {
            self.marks.present |= ValueKind::Container.bit();
        }
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
    pub fn has_null(&self) -> bool {
        self.has_null_where(|_| true)
    }

    /// Whether any row holds a `null` where `at` says one matters: `at` is asked of each
    /// distinct key a `null` is a member's value under, and of `None` when a `null` is no
    /// member's value. A key is asked as the compact text holds it, between its quotes.
    #[must_use]
    pub fn has_null_where(&self, at: impl Fn(Option<&[u8]>) -> bool) -> bool {
        self.marks.nulls.any_where(&self.bytes, at)
    }

    /// Whether any row holds, under one of `keys`, a value its scalar does not hold as
    /// served (§ 3.5; see [`ScalarKind`]): a number no `Int` holds as written
    /// (`3000000000`, `1.5`, `3.0`, `-0`), a string under an `Int`, a number under a
    /// `String`, an object under any of them. A key holding an escape may decode to any of
    /// `keys` starting with its first byte, or to any when that byte opens the escape: a
    /// value one of those refuses counts. A value in an array, or under any other key, is
    /// passed over.
    ///
    /// Rows made [`with_scalar_keys`](Self::with_scalar_keys) of these `keys` answer as they
    /// were copied. Otherwise the rows are read, and only the values under `keys` looked at.
    #[must_use]
    pub fn has_wrong_kind_under(&self, keys: &[(&[u8], ScalarKind)]) -> bool {
        // No value of a kind any of these scalars refuses: none sits under one.
        if self.marks.present & refused_by_any(keys) == 0 {
            return false;
        }
        match &self.marks.told {
            Some(told) if told.iter().eq(keys.iter().copied()) => self.marks.wrong_kind,
            // Copied for other keys, or for none: what was noted says nothing of these.
            _ => wrong_kind_under(&self.bytes, keys),
        }
    }

    /// Whether the rows hold, anywhere below a row, a value of a kind one of `scalars` does
    /// not hold as served (GraphQL § 3.5). When not, no value under any key is of another
    /// kind than its scalar's, whichever keys a reader would ask of
    /// ([`has_wrong_kind_under`](Self::has_wrong_kind_under)). Answered from what copying
    /// the rows counted; nothing is read.
    #[must_use]
    pub fn may_hold_refused_by(&self, scalars: &[ScalarKind]) -> bool {
        let refused = scalars.iter().fold(0, |refused, &scalar| refused | refused_by(scalar));
        self.marks.present & refused != 0
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
