//! F-string type definitions and formatting functions.
//!
//! This module contains the AST types for f-strings (formatted string literals)
//! and the runtime formatting functions used by the bytecode VM.
//!
//! F-strings can contain literal text and interpolated expressions with optional
//! conversion flags (`!s`, `!r`, `!a`) and format specifications.

use std::str::FromStr;

use crate::{
    exception_private::{ExcType, RunError, SimpleException},
    expressions::ExprLoc,
    heap::{Heap, HeapData},
    intern::{Interns, StringId},
    resource::ResourceTracker,
    types::{PyTrait, Type},
    value::Value,
};

// ============================================================================
// F-string type definitions
// ============================================================================

/// Conversion flags for f-string interpolations.
///
/// These control how the value is converted to string before formatting:
/// - `None`: Use default string conversion (equivalent to `str()`)
/// - `Str` (`!s`): Explicitly call `str()`
/// - `Repr` (`!r`): Call `repr()` for debugging representation
/// - `Ascii` (`!a`): Call `ascii()` for ASCII-safe representation
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConversionFlag {
    #[default]
    None,
    /// `!s` - convert using `str()`
    Str,
    /// `!r` - convert using `repr()`
    Repr,
    /// `!a` - convert using `ascii()` (escapes non-ASCII characters)
    Ascii,
}

/// A single part of an f-string.
///
/// F-strings are composed of literal text segments and interpolated expressions.
/// For example, `f"Hello {name}!"` has three parts:
/// - `Literal(interned_hello)` (StringId for "Hello ")
/// - `Interpolation { expr: name, ... }`
/// - `Literal(interned_exclaim)` (StringId for "!")
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FStringPart {
    /// Literal text segment (e.g., "Hello " in `f"Hello {name}"`)
    /// The StringId references the interned string in the Interns table.
    Literal(StringId),
    /// Interpolated expression with optional conversion and format spec
    Interpolation {
        /// The expression to evaluate
        expr: Box<ExprLoc>,
        /// Conversion flag: `None`, `!s` (str), `!r` (repr), `!a` (ascii)
        conversion: ConversionFlag,
        /// Optional format specification (can contain nested interpolations)
        format_spec: Option<FormatSpec>,
        /// Debug prefix for `=` specifier (e.g., "a=" for f'{a=}', " a = " for f'{ a = }').
        /// When present, this text is prepended to the output and repr conversion is used
        /// by default (unless an explicit conversion is specified).
        debug_prefix: Option<StringId>,
    },
}

/// Format specification for f-string interpolations.
///
/// Can be either a pre-parsed static spec or contain nested interpolations.
/// For example:
/// - `f"{value:>10}"` has `FormatSpec::Static { ... }`
/// - `f"{value:{width}}"` has `FormatSpec::Dynamic` with the `width` variable
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FormatSpec {
    /// Pre-parsed static format spec (e.g., ">10s", ".2f")
    ///
    /// Parsing happens at parse time to avoid runtime string parsing overhead.
    /// Invalid specs cause a parse error immediately.
    ///
    /// The `raw_string` field is set when the fill character is non-ASCII
    /// (can't be compactly encoded), allowing the compiler to fall back to
    /// runtime parsing using the original string.
    Static {
        /// The parsed format specification.
        parsed: ParsedFormatSpec,
        /// Original string, stored only when the fill char is non-ASCII.
        /// This is `Some(string_id)` when the fill can't be compactly encoded.
        raw_string: Option<crate::intern::StringId>,
    },
    /// Dynamic format spec with nested f-string parts
    ///
    /// These must be evaluated at runtime, then parsed into a `ParsedFormatSpec`.
    Dynamic(Vec<FStringPart>),
}

/// Parsed format specification following Python's format mini-language.
///
/// Format: `[[fill]align][sign][z][#][0][width][grouping_option][.precision][type]`
///
/// This struct is parsed at parse time for static format specs, avoiding runtime
/// string parsing. For dynamic format specs, parsing happens after evaluation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ParsedFormatSpec {
    /// Fill character for padding (default: space)
    pub fill: char,
    /// Alignment: '<' (left), '>' (right), '^' (center), '=' (sign-aware)
    pub align: Option<char>,
    /// Sign handling: '+' (always), '-' (negative only), ' ' (space for positive)
    pub sign: Option<char>,
    /// Whether to zero-pad numbers
    pub zero_pad: bool,
    /// Minimum field width
    pub width: usize,
    /// Precision for floats or max width for strings
    pub precision: Option<usize>,
    /// Type character: 's', 'd', 'f', 'e', 'g', etc.
    pub type_char: Option<char>,
}

impl FromStr for ParsedFormatSpec {
    type Err = String;

    /// Parses a format specification string into its components.
    ///
    /// Returns an error if the specifier contains invalid or unrecognized characters.
    /// The error includes the original specifier for use in error messages.
    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        if spec.is_empty() {
            return Ok(Self {
                fill: ' ',
                ..Default::default()
            });
        }

        let mut result = Self {
            fill: ' ',
            ..Default::default()
        };
        let mut chars = spec.chars().peekable();

        // Parse fill and align: [[fill]align]
        let first = chars.peek().copied();
        let second_pos = spec.chars().nth(1);

        if let Some(second) = second_pos {
            if matches!(second, '<' | '>' | '^' | '=') {
                // First char is fill, second is align
                result.fill = first.unwrap_or(' ');
                chars.next();
                result.align = chars.next();
            } else if matches!(first, Some('<' | '>' | '^' | '=')) {
                result.align = chars.next();
            }
        } else if matches!(first, Some('<' | '>' | '^' | '=')) {
            result.align = chars.next();
        }

        // Parse sign: +, -, or space
        if matches!(chars.peek(), Some('+' | '-' | ' ')) {
            result.sign = chars.next();
        }

        // Skip '#' (alternate form) for now
        if chars.peek() == Some(&'#') {
            chars.next();
        }

        // Parse zero-padding flag (must come before width)
        if chars.peek() == Some(&'0') {
            result.zero_pad = true;
            chars.next();
        }

        // Parse width
        let mut width_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                width_str.push(c);
                chars.next();
            } else {
                break;
            }
        }
        if !width_str.is_empty() {
            result.width = width_str.parse().unwrap_or(0);
        }

        // Skip grouping option (comma or underscore)
        if matches!(chars.peek(), Some(',' | '_')) {
            chars.next();
        }

        // Parse precision: .N
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut prec_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    prec_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if !prec_str.is_empty() {
                result.precision = Some(prec_str.parse().unwrap_or(0));
            }
        }

        // Parse type character: s, d, f, e, g, etc.
        if let Some(&c) = chars.peek()
            && matches!(
                c,
                's' | 'd' | 'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'n' | '%' | 'b' | 'o' | 'x' | 'X' | 'c'
            )
        {
            result.type_char = Some(c);
            chars.next();
        }

        // Error if there are any unconsumed characters
        if chars.peek().is_some() {
            return Err(spec.to_owned());
        }

        Ok(result)
    }
}

impl std::fmt::Display for ParsedFormatSpec {
    /// Converts the parsed format spec back to a format string.
    ///
    /// This is used by the compiler when the format spec cannot be compactly
    /// encoded (e.g., non-ASCII fill characters) and must be stored as a string
    /// for runtime parsing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Fill and align: only output fill if there's an alignment
        if let Some(align) = self.align {
            if self.fill != ' ' {
                write!(f, "{}", self.fill)?;
            }
            write!(f, "{align}")?;
        }

        // Sign
        if let Some(sign) = self.sign {
            write!(f, "{sign}")?;
        }

        // Zero-padding
        if self.zero_pad {
            write!(f, "0")?;
        }

        // Width
        if self.width > 0 {
            write!(f, "{}", self.width)?;
        }

        // Precision
        if let Some(prec) = self.precision {
            write!(f, ".{prec}")?;
        }

        // Type character
        if let Some(type_char) = self.type_char {
            write!(f, "{type_char}")?;
        }

        Ok(())
    }
}

// ============================================================================
// Format errors
// ============================================================================

/// Error type for format specification failures.
///
/// These errors are returned from formatting functions and should be converted
/// to appropriate Python exceptions (usually ValueError) by the VM.
#[derive(Debug, Clone)]
pub enum FormatError {
    /// Invalid alignment for the given type (e.g., '=' alignment on strings).
    InvalidAlignment(String),
    /// Value out of range (e.g., character code > 0x10FFFF).
    Overflow(String),
    /// Generic value error (e.g., invalid base, invalid Unicode).
    ValueError(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAlignment(msg) | Self::Overflow(msg) | Self::ValueError(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

/// Formats a value according to a format specification, applying type-appropriate formatting.
///
/// Dispatches to the appropriate formatting function based on the value type and format spec:
/// - Integers: `format_int`, `format_int_base`, `format_char`
/// - Floats: `format_float_f`, `format_float_e`, `format_float_g`, `format_float_percent`
/// - Strings: `format_string`
///
/// Returns a `ValueError` if the format type character is incompatible with the value type.
pub fn format_with_spec(
    value: &Value,
    spec: &ParsedFormatSpec,
    heap: &Heap<impl ResourceTracker>,
    interns: &Interns,
) -> Result<String, RunError> {
    let value_type = value.py_type(heap);

    match (value, spec.type_char) {
        // Integer formatting (convert i32 to i64 for formatting functions)
        (Value::Int(n), None | Some('d')) => Ok(format_int(i64::from(*n), spec)),
        (Value::Int(n), Some('b')) => Ok(format_int_base(i64::from(*n), 2, spec)?),
        (Value::Int(n), Some('o')) => Ok(format_int_base(i64::from(*n), 8, spec)?),
        (Value::Int(n), Some('x')) => Ok(format_int_base(i64::from(*n), 16, spec)?),
        (Value::Int(n), Some('X')) => Ok(format_int_base(i64::from(*n), 16, spec)?.to_uppercase()),
        (Value::Int(n), Some('c')) => Ok(format_char(i64::from(*n), spec)?),

        // Float formatting (via heap)
        (Value::Ref(id), None | Some('g' | 'G')) if matches!(heap.get(*id), HeapData::Float(_)) => {
            if let HeapData::Float(f) = heap.get(*id) {
                Ok(format_float_g(*f, spec))
            } else {
                unreachable!()
            }
        }
        (Value::Ref(id), Some('f' | 'F')) if matches!(heap.get(*id), HeapData::Float(_)) => {
            if let HeapData::Float(f) = heap.get(*id) {
                Ok(format_float_f(*f, spec))
            } else {
                unreachable!()
            }
        }
        (Value::Ref(id), Some('e')) if matches!(heap.get(*id), HeapData::Float(_)) => {
            if let HeapData::Float(f) = heap.get(*id) {
                Ok(format_float_e(*f, spec, false))
            } else {
                unreachable!()
            }
        }
        (Value::Ref(id), Some('E')) if matches!(heap.get(*id), HeapData::Float(_)) => {
            if let HeapData::Float(f) = heap.get(*id) {
                Ok(format_float_e(*f, spec, true))
            } else {
                unreachable!()
            }
        }
        (Value::Ref(id), Some('%')) if matches!(heap.get(*id), HeapData::Float(_)) => {
            if let HeapData::Float(f) = heap.get(*id) {
                Ok(format_float_percent(*f, spec))
            } else {
                unreachable!()
            }
        }

        // Int to float formatting (Python allows this)
        (Value::Int(n), Some('f' | 'F')) => Ok(format_float_f(f64::from(*n), spec)),
        (Value::Int(n), Some('e')) => Ok(format_float_e(f64::from(*n), spec, false)),
        (Value::Int(n), Some('E')) => Ok(format_float_e(f64::from(*n), spec, true)),
        (Value::Int(n), Some('g' | 'G')) => Ok(format_float_g(f64::from(*n), spec)),
        (Value::Int(n), Some('%')) => Ok(format_float_percent(f64::from(*n), spec)),

        // String formatting (including InternString and heap strings)
        (_, None | Some('s')) if value_type == Type::Str => {
            let s = value.py_str(heap, interns);
            Ok(format_string(&s, spec)?)
        }

        // Bool as int
        (Value::Bool(b), Some('d')) => Ok(format_int(i64::from(*b), spec)),

        // No type specifier: convert to string and format
        (_, None) => {
            let s = value.py_str(heap, interns);
            Ok(format_string(&s, spec)?)
        }

        // Type mismatch errors
        (_, Some(c)) => Err(SimpleException::new_msg(
            ExcType::ValueError,
            format!("Unknown format code '{c}' for object of type '{value_type}'"),
        )
        .into()),
    }
}

/// Encodes a ParsedFormatSpec into a u64 for storage in bytecode constants.
///
/// Encoding layout (fits in 48 bits):
/// Encodes a format spec into a u32 for storage in the constant pool.
///
/// Uses a compact bit-packing that fits in 31 bits (leaving room for the negative marker
/// used to distinguish format specs from regular integers in the constant pool).
///
/// Bit layout (31 bits total):
/// - fill:      bits 0-6   (7 bits, ASCII 0-127, non-ASCII truncated to space)
/// - type_char: bits 7-10  (4 bits, 0-15)
/// - align:     bits 11-13 (3 bits, 0-4)
/// - sign:      bits 14-15 (2 bits, 0-3)
/// - zero_pad:  bit 16     (1 bit)
/// - width:     bits 17-23 (7 bits, 0-127, clamped if larger)
/// - precision: bits 24-30 (7 bits, 0-126 for actual value, 127 means "no precision")
pub fn encode_format_spec(spec: &ParsedFormatSpec) -> u32 {
    // Fill char: ASCII only (7 bits), non-ASCII defaults to space
    let fill = if spec.fill.is_ascii() {
        u32::from(spec.fill as u8)
    } else {
        u32::from(b' ')
    };
    let fill = fill & 0x7F; // 7 bits

    let type_char = spec.type_char.map_or(0u32, |c| match c {
        'b' => 1,
        'c' => 2,
        'd' => 3,
        'e' => 4,
        'E' => 5,
        'f' => 6,
        'F' => 7,
        'g' => 8,
        'G' => 9,
        'n' => 10,
        'o' => 11,
        's' => 12,
        'x' => 13,
        'X' => 14,
        '%' => 15,
        _ => 0,
    });

    let align = match spec.align {
        None => 0u32,
        Some('<') => 1,
        Some('>') => 2,
        Some('^') => 3,
        Some('=') => 4,
        Some(_) => 0,
    };

    let sign = match spec.sign {
        None => 0u32,
        Some('+') => 1,
        Some('-') => 2,
        Some(' ') => 3,
        Some(_) => 0,
    };

    let zero_pad = u32::from(spec.zero_pad);

    // Width: 7 bits (0-127), clamp if larger
    // Cast is intentional: we clamp to 127 so truncation is handled
    #[expect(clippy::cast_possible_truncation, reason = "value is clamped to 127")]
    let width = (spec.width as u32).min(127);

    // Precision: 7 bits (0-126 for actual value, 127 means "no precision")
    // Cast is intentional: we clamp to 126 so truncation is handled
    #[expect(clippy::cast_possible_truncation, reason = "value is clamped to 126")]
    let precision = spec.precision.map_or(127u32, |p| (p as u32).min(126));

    fill | (type_char << 7) | (align << 11) | (sign << 14) | (zero_pad << 16) | (width << 17) | (precision << 24)
}

/// Decodes a u32 back into a ParsedFormatSpec.
///
/// Reverses the bit-packing done by `encode_format_spec`. Used by the VM
/// when executing `FormatValue` to retrieve the format specification from
/// the constant pool (where it's stored as a negative integer marker).
///
/// Bit layout (31 bits total):
/// - fill:      bits 0-6   (7 bits)
/// - type_char: bits 7-10  (4 bits)
/// - align:     bits 11-13 (3 bits)
/// - sign:      bits 14-15 (2 bits)
/// - zero_pad:  bit 16     (1 bit)
/// - width:     bits 17-23 (7 bits)
/// - precision: bits 24-30 (7 bits, 127 means "no precision")
pub fn decode_format_spec(encoded: u32) -> ParsedFormatSpec {
    let fill = ((encoded & 0x7F) as u8) as char;
    let type_bits = ((encoded >> 7) & 0x0F) as u8;
    let align_bits = (encoded >> 11) & 0x07;
    let sign_bits = (encoded >> 14) & 0x03;
    let zero_pad = ((encoded >> 16) & 0x01) != 0;
    let width = ((encoded >> 17) & 0x7F) as usize;
    let precision_raw = ((encoded >> 24) & 0x7F) as usize;

    let align = match align_bits {
        1 => Some('<'),
        2 => Some('>'),
        3 => Some('^'),
        4 => Some('='),
        _ => None,
    };

    let sign = match sign_bits {
        1 => Some('+'),
        2 => Some('-'),
        3 => Some(' '),
        _ => None,
    };

    // 127 means "no precision" in the compact encoding
    let precision = if precision_raw == 127 {
        None
    } else {
        Some(precision_raw)
    };

    let type_char = match type_bits {
        1 => Some('b'),
        2 => Some('c'),
        3 => Some('d'),
        4 => Some('e'),
        5 => Some('E'),
        6 => Some('f'),
        7 => Some('F'),
        8 => Some('g'),
        9 => Some('G'),
        10 => Some('n'),
        11 => Some('o'),
        12 => Some('s'),
        13 => Some('x'),
        14 => Some('X'),
        15 => Some('%'),
        _ => None,
    };

    ParsedFormatSpec {
        fill,
        align,
        sign,
        zero_pad,
        width,
        precision,
        type_char,
    }
}

// ============================================================================
// Formatting functions
// ============================================================================

/// Formats a string value according to a format specification.
///
/// Applies the following transformations in order:
/// 1. Truncation: If `precision` is set, limits the string to that many characters
/// 2. Alignment: Pads to `width` using `fill` character (default left-aligned for strings)
///
/// Returns an error if `=` alignment is used (sign-aware padding only valid for numbers).
pub fn format_string(value: &str, spec: &ParsedFormatSpec) -> Result<String, FormatError> {
    // Handle precision (string truncation)
    let value = if let Some(prec) = spec.precision {
        value.chars().take(prec).collect::<String>()
    } else {
        value.to_owned()
    };

    // Validate alignment for strings (= is only for numbers)
    if spec.align == Some('=') {
        return Err(FormatError::InvalidAlignment(
            "'=' alignment not allowed in string format specifier".to_owned(),
        ));
    }

    // Default alignment for strings is left ('<')
    let align = spec.align.unwrap_or('<');
    Ok(pad_string(&value, spec.width, align, spec.fill))
}

/// Formats an integer in decimal with a format specification.
///
/// Applies the following:
/// - Sign prefix based on `sign` spec: `+` (always show), `-` (negatives only), ` ` (space for positive)
/// - Zero-padding: When `zero_pad` is true or `=` alignment, inserts zeros between sign and digits
/// - Alignment: Right-aligned by default for numbers, pads to `width` with `fill` character
pub fn format_int(n: i64, spec: &ParsedFormatSpec) -> String {
    let is_negative = n < 0;
    let abs_str = n.abs().to_string();

    // Build the sign prefix
    let sign = if is_negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };

    // Default alignment for numbers is right ('>')
    let align = spec.align.unwrap_or('>');

    // Handle sign-aware zero-padding or regular padding
    if spec.zero_pad || align == '=' {
        let fill = if spec.zero_pad { '0' } else { spec.fill };
        let total_len = sign.len() + abs_str.len();
        if spec.width > total_len {
            let padding = spec.width - total_len;
            let pad_str: String = std::iter::repeat_n(fill, padding).collect();
            format!("{sign}{pad_str}{abs_str}")
        } else {
            format!("{sign}{abs_str}")
        }
    } else {
        let value = format!("{sign}{abs_str}");
        pad_string(&value, spec.width, align, spec.fill)
    }
}

/// Formats an integer in binary (base 2), octal (base 8), or hexadecimal (base 16).
///
/// Used for format types `b`, `o`, `x`, and `X`. The sign is prepended for negative numbers.
/// Does not include base prefixes like `0b`, `0o`, `0x` (those require the `#` flag which
/// is not yet implemented). Returns an error for invalid base values.
pub fn format_int_base(n: i64, base: u32, spec: &ParsedFormatSpec) -> Result<String, FormatError> {
    let is_negative = n < 0;
    let abs_val = n.unsigned_abs();

    let abs_str = match base {
        2 => format!("{abs_val:b}"),
        8 => format!("{abs_val:o}"),
        16 => format!("{abs_val:x}"),
        _ => return Err(FormatError::ValueError("Invalid base".to_owned())),
    };

    let sign = if is_negative { "-" } else { "" };
    let value = format!("{sign}{abs_str}");

    let align = spec.align.unwrap_or('>');
    Ok(pad_string(&value, spec.width, align, spec.fill))
}

/// Formats an integer as a Unicode character (format type `c`).
///
/// Converts the integer to its corresponding Unicode code point. Valid range is 0 to 0x10FFFF.
/// Returns `Overflow` error if out of range, `ValueError` if not a valid Unicode scalar value
/// (e.g., surrogate code points). Left-aligned by default like strings.
pub fn format_char(n: i64, spec: &ParsedFormatSpec) -> Result<String, FormatError> {
    if !(0..=0x0010_FFFF).contains(&n) {
        return Err(FormatError::Overflow("%c arg not in range(0x110000)".to_owned()));
    }
    let n_u32 = u32::try_from(n).expect("format_char n validated in 0..=0x10FFFF range");
    let c = char::from_u32(n_u32).ok_or_else(|| FormatError::ValueError("Invalid Unicode code point".to_owned()))?;
    let value = c.to_string();
    let align = spec.align.unwrap_or('<');
    Ok(pad_string(&value, spec.width, align, spec.fill))
}

/// Formats a float in fixed-point notation (format types `f` and `F`).
///
/// Always includes a decimal point with `precision` digits after it (default 6).
/// Handles sign prefix, zero-padding between sign and digits when `zero_pad` or `=` alignment.
/// Right-aligned by default. NaN and infinity are formatted as `nan`/`inf` (or `NAN`/`INF` for `F`).
pub fn format_float_f(f: f64, spec: &ParsedFormatSpec) -> String {
    let precision = spec.precision.unwrap_or(6);
    let is_negative = f.is_sign_negative() && !f.is_nan();
    let abs_val = f.abs();

    let abs_str = format!("{abs_val:.precision$}");

    let sign = if is_negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };

    let align = spec.align.unwrap_or('>');

    if spec.zero_pad || align == '=' {
        let fill = if spec.zero_pad { '0' } else { spec.fill };
        let total_len = sign.len() + abs_str.len();
        if spec.width > total_len {
            let padding = spec.width - total_len;
            let pad_str: String = std::iter::repeat_n(fill, padding).collect();
            format!("{sign}{pad_str}{abs_str}")
        } else {
            format!("{sign}{abs_str}")
        }
    } else {
        let value = format!("{sign}{abs_str}");
        pad_string(&value, spec.width, align, spec.fill)
    }
}

/// Formats a float in exponential/scientific notation (format types `e` and `E`).
///
/// Produces output like `1.234568e+03` with `precision` digits after decimal (default 6).
/// The `uppercase` parameter controls whether to use `E` or `e` for the exponent marker.
/// Exponent is always formatted with a sign and at least 2 digits (Python convention).
pub fn format_float_e(f: f64, spec: &ParsedFormatSpec, uppercase: bool) -> String {
    let precision = spec.precision.unwrap_or(6);
    let is_negative = f.is_sign_negative() && !f.is_nan();
    let abs_val = f.abs();

    let abs_str = if uppercase {
        format!("{abs_val:.precision$E}")
    } else {
        format!("{abs_val:.precision$e}")
    };

    // Fix exponent format to match Python (e+03 not e3)
    let abs_str = fix_exp_format(&abs_str);

    let sign = if is_negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };

    let value = format!("{sign}{abs_str}");
    let align = spec.align.unwrap_or('>');
    pad_string(&value, spec.width, align, spec.fill)
}

/// Formats a float in "general" format (format types `g` and `G`).
///
/// Chooses between fixed-point and exponential notation based on the magnitude:
/// - Uses exponential if exponent < -4 or >= precision
/// - Otherwise uses fixed-point notation
///
/// Unlike `f` and `e` formats, trailing zeros are stripped from the result.
/// Default precision is 6, but minimum is 1 significant digit.
pub fn format_float_g(f: f64, spec: &ParsedFormatSpec) -> String {
    let precision = spec.precision.unwrap_or(6).max(1);
    let is_negative = f.is_sign_negative() && !f.is_nan();
    let abs_val = f.abs();

    // Python's g format: use exponential if exponent < -4 or >= precision
    let exp = if abs_val == 0.0 {
        0
    } else {
        // log10 of valid floats fits in i32; floor() returns a finite f64
        f64_to_i32_trunc(abs_val.log10().floor())
    };

    // precision is typically small (default 6), safe to convert to i32
    let prec_i32 = i32::try_from(precision).unwrap_or(i32::MAX);
    let abs_str = if exp < -4 || exp >= prec_i32 {
        // Use exponential notation
        let exp_prec = precision.saturating_sub(1);
        let formatted = format!("{abs_val:.exp_prec$e}");
        // Python strips trailing zeros from the mantissa
        strip_trailing_zeros_exp(&formatted)
    } else {
        // Use fixed notation - result is non-negative due to .max(0)
        let sig_digits_i32 = (prec_i32 - exp - 1).max(0);
        let sig_digits = usize::try_from(sig_digits_i32).expect("sig_digits guaranteed non-negative");
        let formatted = format!("{abs_val:.sig_digits$}");
        strip_trailing_zeros(&formatted)
    };

    let sign = if is_negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };

    let value = format!("{sign}{abs_str}");
    let align = spec.align.unwrap_or('>');
    pad_string(&value, spec.width, align, spec.fill)
}

/// Applies ASCII conversion to a string (escapes non-ASCII characters).
///
/// Used for the `!a` conversion flag in f-strings. Takes a string (typically a repr)
/// and escapes all non-ASCII characters using `\xNN`, `\uNNNN`, or `\UNNNNNNNN`.
pub fn ascii_escape(s: &str) -> String {
    use std::fmt::Write;
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii() {
            result.push(c);
        } else {
            let code = c as u32;
            if code <= 0xFF {
                write!(result, "\\x{code:02x}")
            } else if code <= 0xFFFF {
                write!(result, "\\u{code:04x}")
            } else {
                write!(result, "\\U{code:08x}")
            }
            .expect("string write should be infallible");
        }
    }
    result
}

/// Formats a float as a percentage (format type `%`).
///
/// Multiplies the value by 100 and appends a `%` sign. Uses fixed-point notation
/// with `precision` decimal places (default 6). For example, `0.1234` becomes `12.340000%`.
pub fn format_float_percent(f: f64, spec: &ParsedFormatSpec) -> String {
    let precision = spec.precision.unwrap_or(6);
    let percent_val = f * 100.0;
    let is_negative = percent_val.is_sign_negative() && !percent_val.is_nan();
    let abs_val = percent_val.abs();

    let abs_str = format!("{abs_val:.precision$}%");

    let sign = if is_negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };

    let value = format!("{sign}{abs_str}");
    let align = spec.align.unwrap_or('>');
    pad_string(&value, spec.width, align, spec.fill)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Pads a string to a given width with alignment.
///
/// Alignment options:
/// - '<': left-align (pad on right)
/// - '>': right-align (pad on left)
/// - '^': center (pad both sides)
fn pad_string(value: &str, width: usize, align: char, fill: char) -> String {
    let value_len = value.chars().count();
    if width <= value_len {
        return value.to_owned();
    }

    let padding = width - value_len;

    match align {
        '<' => {
            let mut s = value.to_owned();
            for _ in 0..padding {
                s.push(fill);
            }
            s
        }
        '>' => {
            let mut s = String::new();
            for _ in 0..padding {
                s.push(fill);
            }
            s.push_str(value);
            s
        }
        '^' => {
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            let mut s = String::new();
            for _ in 0..left_pad {
                s.push(fill);
            }
            s.push_str(value);
            for _ in 0..right_pad {
                s.push(fill);
            }
            s
        }
        _ => value.to_owned(),
    }
}

/// Strips trailing zeros from a decimal float string.
///
/// Used by the `:g` format to remove insignificant trailing zeros.
/// Also removes the decimal point if all fractional digits are stripped.
/// Has no effect if the string doesn't contain a decimal point.
fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_owned();
    }
    let trimmed = s.trim_end_matches('0');
    if let Some(stripped) = trimmed.strip_suffix('.') {
        stripped.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Strips trailing zeros from a float in exponential notation.
///
/// Splits the string at `e` or `E`, strips zeros from the mantissa part,
/// then recombines with the exponent. Also normalizes the exponent format
/// to Python's convention (sign and at least 2 digits).
fn strip_trailing_zeros_exp(s: &str) -> String {
    if let Some(e_pos) = s.find(['e', 'E']) {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let trimmed_mantissa = strip_trailing_zeros(mantissa);
        let fixed_exp = fix_exp_format(exp_part);
        format!("{trimmed_mantissa}{fixed_exp}")
    } else {
        strip_trailing_zeros(s)
    }
}

/// Converts Rust's exponential format to Python's format.
///
/// Rust produces "e3" or "e-3" but Python expects "e+03" or "e-03".
/// This function ensures the exponent has:
/// 1. A sign character ('+' or '-')
/// 2. At least 2 digits
fn fix_exp_format(s: &str) -> String {
    // Find the 'e' or 'E' marker
    let Some(e_pos) = s.find(['e', 'E']) else {
        return s.to_owned();
    };

    let (before_e, e_and_rest) = s.split_at(e_pos);
    let e_char = e_and_rest.chars().next().unwrap();
    let exp_part = &e_and_rest[1..];

    // Parse the exponent sign and value
    let (sign, digits) = if let Some(stripped) = exp_part.strip_prefix('-') {
        ('-', stripped)
    } else if let Some(stripped) = exp_part.strip_prefix('+') {
        ('+', stripped)
    } else {
        ('+', exp_part)
    };

    // Ensure at least 2 digits
    let padded_digits = if digits.len() < 2 {
        format!("{digits:0>2}")
    } else {
        digits.to_owned()
    };

    format!("{before_e}{e_char}{sign}{padded_digits}")
}

/// Truncates f64 to i32 with clamping for out-of-range values.
///
/// Used for exponent calculations where the result should fit in i32.
fn f64_to_i32_trunc(value: f64) -> i32 {
    if value >= f64::from(i32::MAX) {
        i32::MAX
    } else if value <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        // SAFETY for clippy: value is guaranteed to be in (i32::MIN, i32::MAX)
        // after the bounds checks above, so truncation cannot overflow
        #[expect(clippy::cast_possible_truncation, reason = "bounds checked above")]
        let result = value as i32;
        result
    }
}
