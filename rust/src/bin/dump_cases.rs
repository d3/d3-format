//! Rust side of the differential protocol (MIGRATION_TO_RUST.md section 3.6).
//!
//! Reads the canonical input JSONL that `rust/tools/gen-corpus.mjs` produces from
//! stdin and writes canonical output JSONL to stdout, one output record per input
//! case, in input order. `rust/tools/differential.mjs` computes the same records
//! from the JavaScript oracle and compares them exactly.
//!
//! The transport is complete, and as of Phase 4 so is [`evaluate`]: every operation
//! the corpus can name is answered by the public API, and nothing reports
//! `"unimplemented"` any more. The variant and [`EXIT_UNIMPLEMENTED`] are kept
//! because they are what stops a *future* corpus operation from being silently read
//! as agreement, and because the exit code is part of the protocol.
//!
//! Everything here is dependency-free on purpose: `rust/Cargo.lock` is a reviewed,
//! hash-pinned artifact, and a differential harness is not a good reason to add a
//! JSON crate to a library that will publish without one. The input is canonical --
//! ASCII-only, fixed key order, no numeric inputs as JSON numbers -- so a small
//! strict reader is sufficient and is easier to audit than a general one.

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use d3_format::decimal;
use d3_format::{FormatLimits, FormatSpecifier, Locale, LocaleDefinition};

/// Returned when the corpus was read and answered, but no case could be computed.
const EXIT_UNIMPLEMENTED: u8 = 3;
/// Returned when the corpus itself could not be read.
const EXIT_BAD_INPUT: u8 = 2;

// ---------------------------------------------------------------------------
// Minimal JSON
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    /// Only ever a corpus *metadata* number: precision, an index, a count. Numeric
    /// inputs travel as bit patterns in strings, per section 3.2's contract 6.
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// Insertion-ordered, because canonical JSON's key order is part of its identity.
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(value) => Some(value),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(value) if value.fract() == 0.0 => Some(*value as i64),
            _ => None,
        }
    }

    fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Parser { bytes, at: 0 }
    }

    fn error<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("byte {}: {}", self.at, what))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(
            self.peek(),
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            self.error(&format!("expected {:?}", byte as char))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            self.error("invalid literal")
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            None => self.error("unexpected end of input"),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Str),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(_) => self.number(),
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return self.error("expected ',' or ']'"),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Obj(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            entries.push((key, self.value()?));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Obj(entries));
                }
                _ => return self.error("expected ',' or '}'"),
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(
            self.peek(),
            Some(b'0'..=b'9') | Some(b'.') | Some(b'e') | Some(b'E') | Some(b'+') | Some(b'-')
        ) {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| "invalid utf-8 in number".to_string())?;
        text.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("byte {}: invalid number {:?}", start, text))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = match self.peek() {
                Some(byte) => byte,
                None => return self.error("unterminated string"),
            };
            self.at += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = match self.peek() {
                        Some(escape) => escape,
                        None => return self.error("unterminated escape"),
                    };
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&first) {
                                // A surrogate pair is written as two escapes; the corpus
                                // never emits a lone surrogate, and a Rust `String` could
                                // not hold one anyway, so an unpaired half is an error.
                                if self.peek() != Some(b'\\') {
                                    return self.error("unpaired high surrogate");
                                }
                                self.at += 1;
                                self.expect(b'u')?;
                                let second = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&second) {
                                    return self
                                        .error("high surrogate not followed by a low surrogate");
                                }
                                0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                            } else if (0xdc00..0xe000).contains(&first) {
                                return self.error("unpaired low surrogate");
                            } else {
                                first
                            };
                            match char::from_u32(code) {
                                Some(character) => out.push(character),
                                None => return self.error("escape is not a scalar value"),
                            }
                        }
                        _ => return self.error("unknown escape"),
                    }
                }
                _ => {
                    // The corpus is ASCII-only, but accepting raw UTF-8 costs three
                    // lines and makes the reader usable on hand-written input.
                    let start = self.at - 1;
                    let width = utf8_width(byte);
                    self.at = start + width;
                    match std::str::from_utf8(self.bytes.get(start..self.at).unwrap_or(&[])) {
                        Ok(text) => out.push_str(text),
                        Err(_) => return self.error("invalid utf-8 in string"),
                    }
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let slice = match self.bytes.get(self.at..self.at + 4) {
            Some(slice) => slice,
            None => return self.error("truncated \\u escape"),
        };
        self.at += 4;
        let text = match std::str::from_utf8(slice) {
            Ok(text) => text,
            Err(_) => return self.error("invalid \\u escape"),
        };
        u32::from_str_radix(text, 16)
            .map_err(|_| format!("byte {}: invalid \\u escape", self.at - 4))
    }
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn parse_json(line: &str) -> Result<Json, String> {
    let mut parser = Parser::new(line.as_bytes());
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.at != parser.bytes.len() {
        return Err(format!("byte {}: trailing content", parser.at));
    }
    Ok(value)
}

/// Writes a JSON string literal escaping every non-ASCII and every control code
/// point, mirroring `jsonString` in `harvest.mjs`. The two must agree byte for byte
/// or the comparison in `differential.mjs` would report encoding noise as drift.
fn write_json_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(character),
            _ => {
                let mut buffer = [0u16; 2];
                for unit in character.encode_utf16(&mut buffer) {
                    let _ = write!(out, "\\u{:04x}", unit);
                }
            }
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

/// What the port produced for one case.
enum Outcome {
    /// A value the differential can compare: text, or an f64 by its bit pattern.
    Text(String),
    F64(f64),
    /// A rendered `FormatSpecifier`. Tagged apart from `Text` because the recording
    /// tags it apart: a normalized specifier is not a formatted number.
    Specifier(String),
    /// The port rejected the input. A typed error is a legitimate answer for the
    /// deviations in `DIVERGENCES.md`; `differential.mjs` checks it against the
    /// case's skip reason rather than treating it as agreement.
    Rejected(String),
    /// No implementation exists for an operation the corpus named.
    ///
    /// Unreachable for the operations the corpus generates today. It survives as the
    /// answer for an operation added to `gen-corpus.mjs` and not to this binary,
    /// which is the case that must not be readable as agreement.
    Unimplemented(&'static str),
}

/// The answer for a corpus operation this binary does not know.
const NOT_YET: &str = "dump_cases has no implementation for this operation; every operation \
gen-corpus.mjs emits is implemented, so this is a corpus that has grown past the harness";

/// The single seam between the differential transport and the port.
///
/// Every case the corpus can produce is answered here, either with a value or with
/// the typed rejection `DIVERGENCES.md` licenses.
fn evaluate(case: &Json) -> Outcome {
    let op = match case.get("op").and_then(Json::as_str) {
        Some(op) => op,
        None => return Outcome::Unimplemented(NOT_YET),
    };

    // A null value is a JavaScript coercion case -- `format(spec)(undefined)`, or a
    // type `c` formatter handed text. DIVERGENCES.md entry `no-input-coercion`
    // excuses it, and `differential.mjs` requires the port to decline rather than
    // answer, so the decline has to reach every operation and not only the numeric
    // ones.
    let value = match case.get("value").and_then(Json::as_str).map(parse_bits) {
        Some(Some(value)) => value,
        _ => {
            return match op {
                "numberToString" | "toFixed" | "toExponential" | "toPrecision"
                | "toLocaleStringEn" | "toStringRadix" | "format" | "formatPrefix"
                | "precisionFixed" | "precisionRound" | "precisionPrefix" => {
                    Outcome::Rejected("no-input-coercion: the input is not a number".to_string())
                }
                "formatSpecifier" => return evaluate_specifier(case),
                _ => Outcome::Unimplemented(NOT_YET),
            }
        }
    };
    let precision = case.get("precision").and_then(Json::as_i64);
    let radix = case.get("radix").and_then(Json::as_i64);
    let reference = case
        .get("reference")
        .and_then(Json::as_str)
        .and_then(parse_bits);
    let limits = FormatLimits::DEFAULT;

    match op {
        "format" | "formatPrefix" => return evaluate_format(case, op, value, reference, limits),
        "formatSpecifier" => return evaluate_specifier(case),
        "precisionFixed" => return Outcome::F64(d3_format::precision_fixed(value)),
        "precisionRound" => match reference {
            Some(max) => return Outcome::F64(d3_format::precision_round(value, max)),
            None => {
                return Outcome::Rejected(
                    "precisionRound needs a second numeric argument".to_string(),
                )
            }
        },
        "precisionPrefix" => match reference {
            Some(reference) => return Outcome::F64(d3_format::precision_prefix(value, reference)),
            None => {
                return Outcome::Rejected(
                    "precisionPrefix needs a second numeric argument".to_string(),
                )
            }
        },
        _ => {}
    }

    let result = match op {
        "numberToString" => decimal::number_to_string(value, &limits),
        "toFixed" => match u32_precision(precision) {
            Some(precision) => decimal::to_fixed(value, precision, &limits),
            None => return Outcome::Rejected("toFixed needs a precision in 0..=100".to_string()),
        },
        "toExponential" => match precision {
            None => decimal::to_exponential(value, None, &limits),
            Some(_) => match u32_precision(precision) {
                Some(precision) => decimal::to_exponential(value, Some(precision), &limits),
                None => {
                    return Outcome::Rejected(
                        "toExponential needs a precision in 0..=100".to_string(),
                    )
                }
            },
        },
        "toPrecision" => match precision {
            // `toPrecision()` with no argument is `Number::toString`, per the spec.
            None => decimal::number_to_string(value, &limits),
            Some(_) => match u32_precision(precision) {
                Some(precision) => decimal::to_precision(value, precision, &limits),
                None => {
                    return Outcome::Rejected(
                        "toPrecision needs a precision in 1..=100".to_string(),
                    )
                }
            },
        },
        "toLocaleStringEn" => decimal::to_locale_string_en(value, &limits),
        "toStringRadix" => match radix {
            // Types `b`, `o`, `x` and `X` are the only callers, so 2, 8 and 16 are
            // the whole reachable domain, and there the exact expansion is what V8
            // prints. Outside it V8 runs its own approximation -- it stops at half
            // an ulp and pads with zeros, and `Number.prototype.toString` is only
            // specified to be "a generalization of" `Number::toString` for a radix
            // other than ten. The port is exact, so it declines to claim agreement
            // where it has not demonstrated any.
            Some(radix @ (2 | 8 | 16)) => {
                // d3 always arrives through `Math.round(x).toString(radix)`, so the
                // fractional expansion -- also V8's own approximation -- is out of
                // scope as well.
                if value.is_finite() && value.fract() != 0.0 {
                    return Outcome::Rejected(
                        "toString(radix) on a non-integer is outside the ported domain".to_string(),
                    );
                }
                decimal::round_to_string_radix(value, radix as u32, &limits)
            }
            _ => {
                return Outcome::Rejected(
                    "toString(radix) is ported for the radices d3 reaches: 2, 8 and 16".to_string(),
                )
            }
        },
        _ => return Outcome::Unimplemented(NOT_YET),
    };

    match result {
        Ok(text) => Outcome::Text(text),
        Err(error) => Outcome::Rejected(error.to_string()),
    }
}

/// `locale.format(specifier)(value)` and `locale.formatPrefix(specifier, ref)(value)`.
fn evaluate_format(
    case: &Json,
    op: &str,
    value: f64,
    reference: Option<f64>,
    limits: FormatLimits,
) -> Outcome {
    let specifier = match case.get("specifier").and_then(Json::as_str) {
        Some(specifier) => specifier,
        None => return Outcome::Rejected(format!("{op} needs a specifier")),
    };
    let definition = match case.get("locale_definition") {
        Some(definition @ Json::Obj(_)) => read_locale(definition),
        // d3's `formatLocale({})`, which is every default.
        _ => LocaleDefinition::default(),
    };
    let locale = match Locale::from_definition(definition) {
        Ok(locale) => locale,
        Err(error) => return Outcome::Rejected(error.to_string()),
    };
    let formatter = match (op, reference) {
        ("formatPrefix", Some(reference)) => {
            locale.prefix_formatter_with_limits(specifier, reference, limits)
        }
        ("formatPrefix", None) => {
            return Outcome::Rejected("formatPrefix needs a numeric reference".to_string())
        }
        _ => locale.formatter_with_limits(specifier, limits),
    };
    match formatter.and_then(|formatter| formatter.format_number(value)) {
        Ok(text) => Outcome::Text(text),
        Err(error) => Outcome::Rejected(error.to_string()),
    }
}

/// `String(formatSpecifier(specifier))`.
fn evaluate_specifier(case: &Json) -> Outcome {
    match case.get("specifier").and_then(Json::as_str) {
        None => Outcome::Rejected("formatSpecifier needs a specifier".to_string()),
        Some(text) => match text.parse::<FormatSpecifier>() {
            Ok(specifier) => Outcome::Specifier(specifier.to_string()),
            Err(error) => Outcome::Rejected(error.to_string()),
        },
    }
}

/// Reads a canonical locale definition, where a null is an *absent* key.
///
/// Canonical form spells "the definition omitted this" as null, and d3 substitutes
/// its own default for an absent key. `LocaleDefinition`'s `None` means the same
/// thing, so the two line up field for field.
fn read_locale(definition: &Json) -> LocaleDefinition {
    let text = |key: &str| {
        definition
            .get(key)
            .and_then(Json::as_str)
            .map(str::to_owned)
    };
    let strings = |key: &str| match definition.get(key) {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned))
            .collect::<Option<Vec<String>>>(),
        _ => None,
    };
    LocaleDefinition {
        decimal: text("decimal"),
        thousands: text("thousands"),
        grouping: match definition.get("grouping") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|item| item.as_i64().and_then(|size| u32::try_from(size).ok()))
                .collect::<Option<Vec<u32>>>(),
            _ => None,
        },
        currency: strings("currency").and_then(|items| <[String; 2]>::try_from(items).ok()),
        numerals: strings("numerals").and_then(|items| <[String; 10]>::try_from(items).ok()),
        percent: text("percent"),
        minus: text("minus"),
        nan: text("nan"),
    }
}

/// Reads a 16-digit big-endian bit pattern back into the `f64` it denotes.
fn parse_bits(text: &str) -> Option<f64> {
    u64::from_str_radix(text, 16).ok().map(f64::from_bits)
}

fn u32_precision(precision: Option<i64>) -> Option<u32> {
    match precision {
        Some(precision) if (0..=100).contains(&precision) => Some(precision as u32),
        _ => None,
    }
}

/// Renders one output record. Key order is fixed and matches `differential.mjs`.
fn render_outcome(id: &str, op: &str, outcome: &Outcome) -> String {
    let mut out = String::with_capacity(128);
    out.push_str("{\"id\":");
    write_json_string(&mut out, id);
    out.push_str(",\"op\":");
    write_json_string(&mut out, op);
    match outcome {
        Outcome::Text(text) => {
            out.push_str(
                ",\"status\":\"ok\",\"output\":{\"kind\":\"text\",\"bits\":null,\"text\":",
            );
            write_json_string(&mut out, text);
            out.push_str("},\"error\":null}");
        }
        Outcome::F64(value) => {
            out.push_str(",\"status\":\"ok\",\"output\":{\"kind\":\"f64\",\"bits\":\"");
            let _ = write!(out, "{:016X}", value.to_bits());
            out.push_str("\",\"text\":null},\"error\":null}");
        }
        Outcome::Specifier(text) => {
            out.push_str(
                ",\"status\":\"ok\",\"output\":{\"kind\":\"specifier\",\"bits\":null,\"text\":",
            );
            write_json_string(&mut out, text);
            out.push_str("},\"error\":null}");
        }
        Outcome::Rejected(reason) => {
            out.push_str(",\"status\":\"rejected\",\"output\":null,\"error\":");
            write_json_string(&mut out, reason);
            out.push('}');
        }
        Outcome::Unimplemented(reason) => {
            out.push_str(",\"status\":\"unimplemented\",\"output\":null,\"error\":");
            write_json_string(&mut out, reason);
            out.push('}');
        }
    }
    out
}

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut input) {
        eprintln!("dump_cases: cannot read stdin: {error}");
        return ExitCode::from(EXIT_BAD_INPUT);
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let mut cases = 0usize;
    let mut unimplemented = 0usize;
    let mut header_seen = false;

    for (index, line) in input.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let record = match parse_json(line) {
            Ok(record) => record,
            Err(error) => {
                eprintln!("dump_cases: line {}: {error}", index + 1);
                return ExitCode::from(EXIT_BAD_INPUT);
            }
        };

        if record.get("kind").and_then(Json::as_str) == Some("header") {
            if header_seen {
                eprintln!(
                    "dump_cases: line {}: a corpus has exactly one header",
                    index + 1
                );
                return ExitCode::from(EXIT_BAD_INPUT);
            }
            header_seen = true;
            continue;
        }

        let id = match record.get("id").and_then(Json::as_str) {
            Some(id) => id.to_string(),
            None => {
                eprintln!("dump_cases: line {}: case has no string id", index + 1);
                return ExitCode::from(EXIT_BAD_INPUT);
            }
        };
        let op = match record.get("op").and_then(Json::as_str) {
            Some(op) => op.to_string(),
            None => {
                eprintln!("dump_cases: line {}: case {id} has no string op", index + 1);
                return ExitCode::from(EXIT_BAD_INPUT);
            }
        };

        // Validate every field before answering, so that a corpus this binary
        // accepts is one it can answer and a malformed case fails as malformed
        // rather than as a disagreement.
        if let Err(error) = validate_case(&record) {
            eprintln!("dump_cases: line {}: case {id}: {error}", index + 1);
            return ExitCode::from(EXIT_BAD_INPUT);
        }

        let outcome = evaluate(&record);
        if matches!(outcome, Outcome::Unimplemented(_)) {
            unimplemented += 1;
        }
        cases += 1;

        if let Err(error) = writeln!(out, "{}", render_outcome(&id, &op, &outcome)) {
            eprintln!("dump_cases: cannot write stdout: {error}");
            return ExitCode::from(EXIT_BAD_INPUT);
        }
    }

    if let Err(error) = out.flush() {
        eprintln!("dump_cases: cannot flush stdout: {error}");
        return ExitCode::from(EXIT_BAD_INPUT);
    }

    if unimplemented > 0 {
        eprintln!(
            "dump_cases: answered {cases} case(s), {unimplemented} of them \"unimplemented\": \
the corpus names an operation this binary does not implement. \
Exiting {EXIT_UNIMPLEMENTED} so that no caller can read this run as agreement."
        );
        return ExitCode::from(EXIT_UNIMPLEMENTED);
    }

    eprintln!("dump_cases: answered {cases} case(s)");
    ExitCode::SUCCESS
}

/// Checks the shape of a case without interpreting it.
///
/// This is what makes the Phase 1 binary worth running: it proves the corpus and the
/// Rust reader agree about every field, so the only thing Phase 3 has to get right
/// is the formatting itself.
fn validate_case(record: &Json) -> Result<(), String> {
    let hex16 = |field: &str, value: &Json| -> Result<(), String> {
        if value.is_null() {
            return Ok(());
        }
        match value.as_str() {
            Some(text)
                if text.len() == 16
                    && text
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) =>
            {
                u64::from_str_radix(text, 16)
                    .map(|_| ())
                    .map_err(|_| format!("{field} is not hex"))
            }
            _ => Err(format!(
                "{field} must be a 16-digit uppercase bit pattern or null"
            )),
        }
    };

    for field in ["value", "reference"] {
        match record.get(field) {
            Some(value) => hex16(field, value)?,
            None => return Err(format!("missing field {field}")),
        }
    }

    match record.get("precision") {
        Some(Json::Null) => {}
        Some(value) if value.as_i64().is_some() => {}
        _ => return Err("precision must be an integer or null".to_string()),
    }

    match record.get("specifier") {
        Some(Json::Null) => {}
        Some(Json::Str(_)) => {}
        _ => return Err("specifier must be a string or null".to_string()),
    }

    match record.get("locale_definition") {
        Some(Json::Null) => {}
        Some(definition @ Json::Obj(_)) => validate_locale(definition)?,
        _ => return Err("locale_definition must be an object or null".to_string()),
    }

    Ok(())
}

/// Checks a locale definition's *shape*, not its completeness.
///
/// Every field may be null. The oracle tests build partial definitions --
/// `formatLocale({decimal: "/"})` is a real recorded case -- and `src/locale.js`
/// substitutes its own default for each absent key. Canonical form spells "absent"
/// as null, so rejecting nulls here would reject legitimate recorded input. Applying
/// those defaults is `LocaleBuilder`'s job, not the transport's.
fn validate_locale(definition: &Json) -> Result<(), String> {
    for field in ["decimal", "thousands", "percent", "minus", "nan"] {
        match definition.get(field) {
            Some(Json::Null) | None => {}
            Some(Json::Str(_)) => {}
            _ => {
                return Err(format!(
                    "locale_definition.{field} must be a string or null"
                ))
            }
        }
    }
    match definition.get("grouping") {
        Some(Json::Null) | None => {}
        Some(Json::Arr(items))
            if !items.is_empty() && items.iter().all(|i| i.as_i64().is_some()) => {}
        _ => {
            return Err(
                "locale_definition.grouping must be a non-empty array of integers or null"
                    .to_string(),
            )
        }
    }
    match definition.get("currency") {
        Some(Json::Null) | None => {}
        Some(Json::Arr(items))
            if items.len() == 2 && items.iter().all(|i| i.as_str().is_some()) => {}
        _ => {
            return Err("locale_definition.currency must be a pair of strings or null".to_string())
        }
    }
    match definition.get("numerals") {
        Some(Json::Null) | None => {}
        Some(Json::Arr(items))
            if items.len() == 10 && items.iter().all(|i| i.as_str().is_some()) => {}
        _ => return Err("locale_definition.numerals must be ten strings or null".to_string()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_canonical_shapes_the_corpus_emits() {
        let line = r#"{"id":"c","op":"format","value":"BFF0000000000000","reference":null,"precision":null,"specifier":"~s","locale_definition":{"decimal":"\u066b","thousands":",","grouping":[3],"currency":["","\u00a0e"],"numerals":null}}"#;
        let record = parse_json(line).expect("canonical corpus line parses");
        assert_eq!(record.get("id").and_then(Json::as_str), Some("c"));
        assert_eq!(
            record
                .get("locale_definition")
                .and_then(|l| l.get("decimal"))
                .and_then(Json::as_str),
            Some("\u{66b}")
        );
        validate_case(&record).expect("canonical corpus line validates");
    }

    #[test]
    fn escapes_match_the_javascript_writer() {
        let mut out = String::new();
        write_json_string(&mut out, "a\u{66b}\"\n\u{1d7ce}");
        assert_eq!(out, r#""a\u066b\"\n\ud835\udfce""#);
    }

    #[test]
    fn every_operation_the_corpus_emits_is_answered() {
        // The full set gen-corpus.mjs can produce. A corpus operation that reaches
        // `Unimplemented` fails the whole differential run by design, so the list
        // being complete is the property worth asserting.
        let cases = [
            (
                "format",
                r#"{"id":"c","op":"format","value":"C08F400000000000","specifier":"$,.2f","locale_definition":{"decimal":".","thousands":",","grouping":[3],"currency":["$",""]}}"#,
                r#""text":"\u2212$1,000.00""#,
            ),
            (
                // 3190544 against a reference of 1e6, so the bucket is mega and the
                // value is scaled by the reference's `Math.pow(10, -6)`, not by its
                // own exponent. Checked in Node.
                "formatPrefix",
                r#"{"id":"c","op":"formatPrefix","value":"4148578800000000","reference":"412E848000000000","specifier":",.2","locale_definition":{}}"#,
                r#""text":"3.19M""#,
            ),
            (
                "formatSpecifier",
                r#"{"id":"c","op":"formatSpecifier","value":null,"specifier":"q"}"#,
                r#""kind":"specifier","bits":null,"text":" >-q""#,
            ),
            (
                // `precisionFixed(0.01)` is 2.
                "precisionFixed",
                r#"{"id":"c","op":"precisionFixed","value":"3F847AE147AE147B"}"#,
                r#""kind":"f64","bits":"4000000000000000""#,
            ),
            (
                // `precisionRound(0.01, 100)` is 4.
                "precisionRound",
                r#"{"id":"c","op":"precisionRound","value":"3F847AE147AE147B","reference":"4059000000000000"}"#,
                r#""kind":"f64","bits":"4010000000000000""#,
            ),
            (
                // `precisionPrefix(1e-6, 0.0013)` is 3.
                "precisionPrefix",
                r#"{"id":"c","op":"precisionPrefix","value":"3EB0C6F7A0B5ED8D","reference":"3F554C985F06F694"}"#,
                r#""kind":"f64","bits":"4008000000000000""#,
            ),
        ];
        for (op, line, expected) in cases {
            let record = parse_json(line).expect("case parses");
            let rendered = render_outcome("c", op, &evaluate(&record));
            assert!(rendered.contains(expected), "{op}: {rendered}");
        }
    }

    #[test]
    fn a_type_c_formatter_declines_a_number_rather_than_coercing_it() {
        let line = r#"{"id":"c","op":"format","value":"3FF0000000000000","specifier":"020c","locale_definition":{}}"#;
        let record = parse_json(line).unwrap();
        let rendered = render_outcome("c", "format", &evaluate(&record));
        assert!(rendered.contains(r#""status":"rejected""#), "{rendered}");
    }

    #[test]
    fn the_numeric_conversions_answer() {
        let line = r#"{"id":"c","op":"toFixed","value":"3FC0000000000000","precision":2}"#;
        let record = parse_json(line).unwrap();
        assert!(
            render_outcome("c", "toFixed", &evaluate(&record))
                .contains(r#""status":"ok","output":{"kind":"text","bits":null,"text":"0.13"}"#),
            "0.125 rounds half away from zero"
        );
    }

    #[test]
    fn a_non_numeric_input_is_declined_rather_than_answered() {
        let line = r#"{"id":"c","op":"toFixed","value":null,"precision":2}"#;
        let record = parse_json(line).unwrap();
        assert!(
            render_outcome("c", "toFixed", &evaluate(&record)).contains(r#""status":"rejected""#)
        );
    }
}
