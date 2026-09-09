//! Exact ECMAScript number conversion.
//!
//! **This is an unstable internal surface. It is not part of the crate's public
//! API.** That API is the specifier type, `Locale` and its builder, `Formatter`,
//! the error types, `FormatLimits`, and the precision helpers, and this module is
//! not among them. It is reachable from outside the crate for one mechanical
//! reason: `tests/decimal.rs` is an integration test, and an integration test
//! links the library as an external crate, so nothing private is visible to it.
//! The module is therefore `pub` and `#[doc(hidden)]`, and it is **excluded from
//! the crate's compatibility and semantic-versioning guarantees**: any item here
//! may change signature, change behaviour, or disappear in a patch release. Depend
//! on it and a `cargo update` may break your build. A later release decides
//! whether to promote part of it to a supported API or seal it away entirely;
//! until then the only sanctioned callers are this crate and its own tests.
//!
//! This is the shared engine the whole port converts through. Every
//! conversion here operates on the exact binary64 value: a finite `f64` is
//! `significand * 2^exponent` with an integer significand, so it has a finite exact
//! decimal expansion, and each function below reads the digits of that expansion
//! rather than of some intermediate approximation.
//!
//! Two properties are load-bearing and are easy to get subtly wrong.
//!
//! **Rounding is half away from zero against the exact value, not half even.**
//! Every conversion reduces to "generate `n` decimal digits, then decide whether
//! the exact remainder reaches one half of the last digit's unit". Because ties go
//! away from zero and anything above a half also rounds up, the rule collapses to
//! "round up unless the remainder is strictly below one half" -- see [`Half`] and
//! [`round_half_away`]. Section 0.3 records that a half-even simulation passes all
//! 1,078 committed assertions, so this distinction is only visible to the
//! structured tie corpora, never to the ported suite.
//!
//! **The decimal exponent comes from shortest-decimal computation.** d3 reads it
//! out of `toExponential()`, which reports the exponent of the shortest
//! round-tripping decimal. That is not always `floor(log10(x))`: a value just below
//! a power of ten whose shortest form rounds up to that power reports the larger
//! exponent. [`shortest`] therefore implements the exact free-format algorithm
//! (Steele and White / Burger and Dybvig) over the crate-private `BigNat`, which
//! is deliberately not linked: it is private, and rustdoc rightly refuses to make a
//! link out of a name a reader cannot follow.
//!
//! The two conversions also break ties differently, which is easy to miss because
//! the words are so similar. `toFixed`, `toExponential(p)` and `toPrecision` pick
//! the *larger* candidate on an exact tie; `Number::toString` picks the *even* one.
//! `(1125899906842624.25).toFixed(1)` is therefore `"1125899906842624.3"` while
//! `String(1125899906842624.25)` is `"1125899906842624.2"`.
//!
//! Nothing here reaches for Rust's own float formatting. The one thing the standard
//! library is trusted with is arithmetic that section 4.4 requires be done in
//! ordinary `f64`: the `x * 100.0` of percent scaling and the `scale * value` of
//! prefix scaling.

use core::cmp::Ordering;

use crate::bignat::BigNat;
use crate::limits::{FormatError, FormatLimits};

/// `log10(2)`, used only to seed the decimal scaling estimate. The estimate is
/// corrected by exact comparison afterwards, so its accuracy affects speed alone.
const LOG10_2: f64 = core::f64::consts::LOG10_2;

/// The magnitude at which `toFixed` hands over to base-10 `Number::toString` and at
/// which type `d` hands over to `toLocaleString("en")`. The two rules are separate;
/// see [`to_fixed`] and [`format_decimal`].
const FIXED_NOTATION_LIMIT: f64 = 1e21;

/// Fraction digits `Intl.NumberFormat` emits by default, which is what type `d`
/// inherits from `toLocaleString("en")`.
const LOCALE_MAX_FRACTION_DIGITS: usize = 3;

/// Upper bound on the digits the free-format algorithm can need. Seventeen is the
/// real maximum for binary64; the loop is bounded well above it so that it can
/// never spin, and never has to.
const MAX_SHORTEST_DIGITS: usize = 32;

/// The 17 SI prefix scales `formatPrefix` can reach, pinned by bit pattern.
///
/// d3 computes these as `Math.pow(10, -e)` for `e` in `-24, -21, ..., 24`. Section
/// 0.3 measured all 17 against their decimal literals under Node 24.18.0 and found
/// them identical, and [`tests::si_scale_table_matches_its_decimal_literals`]
/// re-checks that here. They are stored as bit patterns rather than literals so
/// that the table states the exact doubles d3 multiplies by, which is the thing the
/// compatibility contract is about; the same measurement also showed that
/// multiplying by this scale and dividing by `10^e` are observably different
/// operations, so section 4.4 requires the multiplication.
const SI_SCALES: [u64; 17] = [
    0x44EA_7843_79D9_9DB4, // e = -24 -> 1e24
    0x444B_1AE4_D6E2_EF50, // e = -21 -> 1e21
    0x43AB_C16D_674E_C800, // e = -18 -> 1e18
    0x430C_6BF5_2634_0000, // e = -15 -> 1e15
    0x426D_1A94_A200_0000, // e = -12 -> 1e12
    0x41CD_CD65_0000_0000, // e =  -9 -> 1e9
    0x412E_8480_0000_0000, // e =  -6 -> 1e6
    0x408F_4000_0000_0000, // e =  -3 -> 1e3
    0x3FF0_0000_0000_0000, // e =   0 -> 1e0
    0x3F50_624D_D2F1_A9FC, // e =   3 -> 1e-3
    0x3EB0_C6F7_A0B5_ED8D, // e =   6 -> 1e-6
    0x3E11_2E0B_E826_D695, // e =   9 -> 1e-9
    0x3D71_9799_812D_EA11, // e =  12 -> 1e-12
    0x3CD2_03AF_9EE7_5616, // e =  15 -> 1e-15
    0x3C32_725D_D1D2_43AC, // e =  18 -> 1e-18
    0x3B92_E3B4_0A0E_9B4F, // e =  21 -> 1e-21
    0x3AF3_57C2_99A8_8EA7, // e =  24 -> 1e-24
];

// ---------------------------------------------------------------------------
// Exact decomposition
// ---------------------------------------------------------------------------

/// A finite non-zero magnitude, exactly, as `significand * 2^exponent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Split {
    significand: u64,
    exponent: i32,
}

/// Decomposes `|x|`. `None` for zero and for the non-finite values, which every
/// caller has to special-case anyway.
fn split(x: f64) -> Option<Split> {
    if !x.is_finite() || x == 0.0 {
        return None;
    }
    let bits = x.to_bits() & !(1u64 << 63);
    let biased = ((bits >> 52) & 0x7FF) as i32;
    let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
    if biased == 0 {
        Some(Split {
            significand: fraction,
            exponent: -1074,
        })
    } else {
        Some(Split {
            significand: fraction | (1u64 << 52),
            exponent: biased - 1075,
        })
    }
}

// ---------------------------------------------------------------------------
// Bounded output
// ---------------------------------------------------------------------------

/// A `String` that respects [`FormatLimits::max_output_bytes`] and grows through
/// `try_reserve`, as section 4.2 requires of every returned string.
struct Out {
    buffer: String,
    maximum: usize,
}

impl Out {
    fn new(limits: &FormatLimits) -> Out {
        Out {
            buffer: String::new(),
            maximum: limits.max_output_bytes,
        }
    }

    fn reserve(&mut self, additional: usize) -> Result<(), FormatError> {
        let total = self
            .buffer
            .len()
            .checked_add(additional)
            .ok_or(FormatError::AllocationFailed)?;
        if total > self.maximum {
            return Err(FormatError::OutputLimit {
                requested: total,
                maximum: self.maximum,
            });
        }
        self.buffer
            .try_reserve(additional)
            .map_err(|_| FormatError::AllocationFailed)
    }

    fn text(&mut self, text: &str) -> Result<(), FormatError> {
        self.reserve(text.len())?;
        self.buffer.push_str(text);
        Ok(())
    }

    fn digits(&mut self, digits: &[u8]) -> Result<(), FormatError> {
        self.reserve(digits.len())?;
        for digit in digits {
            self.buffer.push((b'0' + digit) as char);
        }
        Ok(())
    }

    fn zeros(&mut self, count: usize) -> Result<(), FormatError> {
        self.reserve(count)?;
        for _ in 0..count {
            self.buffer.push('0');
        }
        Ok(())
    }

    /// The exponent suffix ECMAScript spells: `e`, an explicit sign, and no padding.
    fn exponent_suffix(&mut self, exponent: i32) -> Result<(), FormatError> {
        self.text(if exponent < 0 { "e-" } else { "e+" })?;
        let magnitude = exponent.unsigned_abs();
        self.text(&magnitude.to_string())
    }

    fn finish(self) -> String {
        self.buffer
    }
}

fn digit_vec(count: usize, limits: &FormatLimits) -> Result<Vec<u8>, FormatError> {
    if count > limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: count,
            maximum: limits.max_output_bytes,
        });
    }
    let mut digits = Vec::new();
    digits
        .try_reserve(count)
        .map_err(|_| FormatError::AllocationFailed)?;
    Ok(digits)
}

fn zero_digits(count: usize, limits: &FormatLimits) -> Result<Vec<u8>, FormatError> {
    let mut digits = digit_vec(count, limits)?;
    digits.resize(count, 0);
    Ok(digits)
}

// ---------------------------------------------------------------------------
// Digit generation
// ---------------------------------------------------------------------------

/// How the exact discarded remainder compares with one half of the last retained
/// digit's unit.
///
/// This is the only place a rounding rule can differ. Half away from zero rounds up
/// on [`Half::Exact`]; half even would not. Section 0.3 records that the committed
/// suite cannot tell the two apart, which is why the tie corpora exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// The remainder is strictly below one half.
    Below,
    /// The remainder is exactly one half: an exact tie against the binary64 value.
    Exact,
    /// The remainder is strictly above one half.
    Above,
}

/// ECMAScript's rule, applied to a window of significant digits: round up unless
/// the exact remainder is below one half.
///
/// Returns `true` when the increment carried past the leading digit. The window
/// keeps its length: on a carry it becomes a leading 1 followed by zeros, and the
/// caller raises the decimal exponent by one, which is the renormalization
/// `toExponential` and `toPrecision` perform. `to_fixed` reads the same window as
/// an integer and appends the extra zero instead.
pub fn round_half_away(digits: &mut [u8], half: Half) -> bool {
    if half == Half::Below {
        return false;
    }
    if !increment(digits) {
        return false;
    }
    if let Some(first) = digits.first_mut() {
        *first = 1;
    }
    true
}

/// Adds one to the number `digits` spells. Returns `true` when the carry ran off
/// the front, in which case every digit is now zero and the true value is one
/// followed by `digits.len()` zeros.
fn increment(digits: &mut [u8]) -> bool {
    for digit in digits.iter_mut().rev() {
        if *digit < 9 {
            *digit += 1;
            return false;
        }
        *digit = 0;
    }
    true
}

/// Long division of `|x|` by successive powers of ten, in exact integer arithmetic.
///
/// After construction `r / s` is `|x| / 10^exponent` and lies in `[1, 10)`, so the
/// first digit taken is the leading significant digit and `exponent` is its decimal
/// place value.
struct Generator {
    r: BigNat,
    s: BigNat,
    scratch: BigNat,
    exponent: i32,
}

impl Generator {
    fn new(value: Split) -> Result<Generator, FormatError> {
        let (mut r, mut s) = if value.exponent >= 0 {
            (
                BigNat::try_from_u64_shl(value.significand, value.exponent as u32)?,
                BigNat::try_from_u64_shl(1, 0)?,
            )
        } else {
            (
                BigNat::try_from_u64_shl(value.significand, 0)?,
                BigNat::try_pow2(value.exponent.unsigned_abs())?,
            )
        };

        // Seed with the binary exponent, then correct by exact comparison. The seed
        // is never more than a digit or two out, and being out only costs a loop
        // iteration; it can never make the result wrong.
        let bits = 64 - value.significand.leading_zeros() as i32;
        let mut exponent = (f64::from(bits + value.exponent - 1) * LOG10_2).floor() as i32;
        if exponent >= 0 {
            s.try_mul_pow10(exponent as u32)?;
        } else {
            r.try_mul_pow10(exponent.unsigned_abs())?;
        }

        let mut scratch = BigNat::zero();
        while r < s {
            r.try_mul_small(10)?;
            exponent -= 1;
        }
        loop {
            scratch.try_clone_from(&s)?;
            scratch.try_mul_small(10)?;
            if r < scratch {
                break;
            }
            core::mem::swap(&mut s, &mut scratch);
            exponent += 1;
        }

        Ok(Generator {
            r,
            s,
            scratch,
            exponent,
        })
    }

    /// Takes `count` digits, most significant first, and reports the exact
    /// remainder's relation to one half of the last one's unit.
    fn take(
        &mut self,
        count: usize,
        limits: &FormatLimits,
    ) -> Result<(Vec<u8>, Half), FormatError> {
        let mut digits = digit_vec(count, limits)?;
        for index in 0..count {
            if index > 0 {
                if self.r.is_zero() {
                    // The exact expansion has ended; every remaining digit is a
                    // real zero and the remainder is exactly nothing.
                    digits.resize(count, 0);
                    return Ok((digits, Half::Below));
                }
                self.r.try_mul_small(10)?;
            }
            digits.push(next_digit(&mut self.r, &self.s));
        }
        let half = self.half_against(1)?;
        Ok((digits, half))
    }

    /// The remainder's relation to one half when no digit is retained at all and the
    /// leading digit sits one place below the last retained position.
    fn half_before_first_digit(&mut self) -> Result<Half, FormatError> {
        self.half_against(10)
    }

    /// Compares `2 * r` with `factor * s`.
    fn half_against(&mut self, factor: u32) -> Result<Half, FormatError> {
        self.scratch.try_clone_from(&self.r)?;
        self.scratch.try_mul_small(2)?;
        let mut bound = BigNat::zero();
        bound.try_clone_from(&self.s)?;
        bound.try_mul_small(factor)?;
        Ok(match self.scratch.cmp(&bound) {
            Ordering::Less => Half::Below,
            Ordering::Equal => Half::Exact,
            Ordering::Greater => Half::Above,
        })
    }
}

/// `floor(r / s)` for `r < 10 * s`, leaving the remainder in `r`.
fn next_digit(r: &mut BigNat, s: &BigNat) -> u8 {
    let mut digit = 0u8;
    while digit < 9 && *r >= *s {
        r.sub_assign(s);
        digit += 1;
    }
    digit
}

/// Exactly `count` significant digits of `|x|`, rounded half away from zero, with
/// the decimal place value of the first digit.
///
/// `None` when `x` is zero or not finite. Exposed so that the test suite can round
/// the very same exact remainder with a different tie rule; see
/// `tests/decimal.rs`.
pub fn significant_digits(
    x: f64,
    count: u32,
    limits: &FormatLimits,
) -> Result<Option<(Vec<u8>, i32, Half)>, FormatError> {
    let value = match split(x) {
        None => return Ok(None),
        Some(value) => value,
    };
    let mut generator = Generator::new(value)?;
    let (digits, half) = generator.take(count.max(1) as usize, limits)?;
    Ok(Some((digits, generator.exponent, half)))
}

// ---------------------------------------------------------------------------
// Shortest round-tripping decimal
// ---------------------------------------------------------------------------

/// The shortest decimal that reads back as `|x|`, with the place value of its first
/// digit. `None` for zero and the non-finite values.
///
/// This is ECMAScript's `Number::toString` digit selection: fewest digits first,
/// then closest to the exact value, and on a tie the candidate whose last digit is
/// even.
pub fn shortest(x: f64, limits: &FormatLimits) -> Result<Option<(Vec<u8>, i32)>, FormatError> {
    let value = match split(x) {
        None => return Ok(None),
        Some(value) => value,
    };
    let significand = value.significand;
    let binary_exponent = value.exponent;

    // A boundary decimal reads back as `x` exactly when the significand is even,
    // because that is when round-half-to-even lands on `x`.
    let inclusive = significand % 2 == 0;
    // At the bottom of a binade the predecessor is half an ulp closer than the
    // successor, unless the predecessor is subnormal and the spacing is unchanged.
    let narrow_below = significand == (1u64 << 52) && binary_exponent != -1074;

    let (mut r, mut s, mut plus, mut minus) = if binary_exponent >= 0 {
        let shift = binary_exponent as u32;
        if narrow_below {
            (
                BigNat::try_from_u64_shl(significand, shift + 2)?,
                BigNat::try_from_u64_shl(4, 0)?,
                BigNat::try_pow2(shift + 1)?,
                BigNat::try_pow2(shift)?,
            )
        } else {
            (
                BigNat::try_from_u64_shl(significand, shift + 1)?,
                BigNat::try_from_u64_shl(2, 0)?,
                BigNat::try_pow2(shift)?,
                BigNat::try_pow2(shift)?,
            )
        }
    } else {
        let shift = binary_exponent.unsigned_abs();
        if narrow_below {
            (
                BigNat::try_from_u64_shl(significand, 2)?,
                BigNat::try_pow2(shift + 2)?,
                BigNat::try_from_u64_shl(2, 0)?,
                BigNat::try_from_u64_shl(1, 0)?,
            )
        } else {
            (
                BigNat::try_from_u64_shl(significand, 1)?,
                BigNat::try_pow2(shift + 1)?,
                BigNat::try_from_u64_shl(1, 0)?,
                BigNat::try_from_u64_shl(1, 0)?,
            )
        }
    };

    let bits = 64 - significand.leading_zeros() as i32;
    let mut scale = (f64::from(bits + binary_exponent) * LOG10_2).ceil() as i32;
    if scale >= 0 {
        s.try_mul_pow10(scale as u32)?;
    } else {
        let factor = scale.unsigned_abs();
        r.try_mul_pow10(factor)?;
        plus.try_mul_pow10(factor)?;
        minus.try_mul_pow10(factor)?;
    }

    let mut scratch = BigNat::zero();
    loop {
        scratch.try_clone_from(&r)?;
        scratch.try_add_assign(&plus)?;
        let above_one = if inclusive { scratch >= s } else { scratch > s };
        if !above_one {
            break;
        }
        s.try_mul_small(10)?;
        scale += 1;
    }
    loop {
        scratch.try_clone_from(&r)?;
        scratch.try_add_assign(&plus)?;
        scratch.try_mul_small(10)?;
        let below_a_tenth = if inclusive { scratch <= s } else { scratch < s };
        if !below_a_tenth {
            break;
        }
        r.try_mul_small(10)?;
        plus.try_mul_small(10)?;
        minus.try_mul_small(10)?;
        scale -= 1;
    }

    let mut digits = digit_vec(MAX_SHORTEST_DIGITS, limits)?;
    for _ in 0..MAX_SHORTEST_DIGITS {
        r.try_mul_small(10)?;
        plus.try_mul_small(10)?;
        minus.try_mul_small(10)?;
        let digit = next_digit(&mut r, &s);

        let can_stop_low = if inclusive { r <= minus } else { r < minus };
        scratch.try_clone_from(&r)?;
        scratch.try_add_assign(&plus)?;
        let can_stop_high = if inclusive { scratch >= s } else { scratch > s };

        if !can_stop_low && !can_stop_high {
            digits.push(digit);
            continue;
        }
        let last = if can_stop_low && !can_stop_high {
            digit
        } else if can_stop_high && !can_stop_low {
            digit + 1
        } else {
            // Both ends accept: take the candidate closer to the exact value. On an
            // exact tie `Number::toString` chooses the *even* digit -- a different
            // rule from the half-away-from-zero of `toFixed`, `toExponential(p)` and
            // `toPrecision`, and the reason `(1125899906842624.25).toFixed(1)` is
            // "1125899906842624.3" while `String` of the same value is
            // "1125899906842624.2".
            scratch.try_clone_from(&r)?;
            scratch.try_mul_small(2)?;
            match scratch.cmp(&s) {
                Ordering::Less => digit,
                Ordering::Greater => digit + 1,
                Ordering::Equal if digit % 2 == 0 => digit,
                Ordering::Equal => digit + 1,
            }
        };

        if last < 10 {
            digits.push(last);
            break;
        }
        // Rounding the last digit up carried. Nothing is wrong: `9.99e22` rounding
        // to `1e23` is exactly how `String(1e23)` is spelled, and the carry has to
        // run back through the digits already emitted. Trailing zeros the carry
        // creates are dropped, because the shortest form does not keep them.
        digits.push(9);
        if increment(&mut digits) {
            digits.clear();
            digits.push(1);
            scale += 1;
        } else {
            while digits.len() > 1 && digits.last() == Some(&0) {
                digits.pop();
            }
        }
        break;
    }

    // The scaling guarantees a non-zero leading digit for every normal value; the
    // few subnormals with a significand of a handful of bits are the only ones that
    // could produce a leading zero, and dropping it is an exact rewrite.
    let mut leading = 0;
    while leading + 1 < digits.len() && digits[leading] == 0 {
        leading += 1;
    }
    if leading > 0 {
        digits.drain(0..leading);
        scale -= leading as i32;
    }

    Ok(Some((digits, scale - 1)))
}

// ---------------------------------------------------------------------------
// The ECMAScript conversions
// ---------------------------------------------------------------------------

/// Renders `digits` in ECMAScript's exponential layout: one digit, an optional
/// fraction, then the exponent.
fn write_exponential(out: &mut Out, digits: &[u8], exponent: i32) -> Result<(), FormatError> {
    out.digits(&digits[..1])?;
    if digits.len() > 1 {
        out.text(".")?;
        out.digits(&digits[1..])?;
    }
    out.exponent_suffix(exponent)
}

/// ECMAScript `Number.prototype.toFixed`.
///
/// The `1e21` switch is part of `toFixed` itself: above that magnitude it returns
/// base-10 `Number::toString`, so `(1e21).toFixed(2)` is `"1e+21"`. Type `d` has a
/// separate rule at the same threshold; see [`format_decimal`].
pub fn to_fixed(x: f64, precision: u32, limits: &FormatLimits) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    if x.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    // `-0.0 < 0.0` is false, which is exactly the spec's test: `(-0).toFixed(2)` is
    // "0.00" while `(-0.0001).toFixed(2)` is "-0.00".
    let negative = x < 0.0;
    if negative {
        out.text("-")?;
    }
    let magnitude = if negative { -x } else { x };
    if !magnitude.is_finite() {
        out.text("Infinity")?;
        return Ok(out.finish());
    }
    if magnitude >= FIXED_NOTATION_LIMIT {
        write_number(&mut out, magnitude, limits)?;
        return Ok(out.finish());
    }

    check_precision(precision, limits)?;
    let precision = precision as usize;
    // The digits of `n`, the integer for which `n / 10^precision` is closest to the
    // exact value, with an exact tie resolved to the larger `n`.
    let mut scaled = match split(magnitude) {
        None => digit_vec(1, limits)?,
        Some(value) => {
            let mut generator = Generator::new(value)?;
            let count = generator.exponent + precision as i32 + 1;
            if count > 0 {
                let (mut digits, half) = generator.take(count as usize, limits)?;
                if round_half_away(&mut digits, half) {
                    // The window renormalized to `1` followed by zeros, which is a
                    // tenth of the integer it stands for; restoring the factor of
                    // ten is one more zero on the end.
                    let mut carried = digit_vec(digits.len() + 1, limits)?;
                    carried.extend_from_slice(&digits);
                    carried.push(0);
                    carried
                } else {
                    digits
                }
            } else if count == 0 && generator.half_before_first_digit()? != Half::Below {
                let mut one = digit_vec(1, limits)?;
                one.push(1);
                one
            } else {
                digit_vec(1, limits)?
            }
        }
    };
    if scaled.is_empty() {
        scaled.push(0);
    }

    // ECMAScript pads `ToString(n)` with leading zeros to `precision + 1` digits and
    // then inserts the point, which is what puts the "0." in front of "0.00".
    let whole = scaled.len().saturating_sub(precision);
    out.zeros(if whole == 0 { 1 } else { 0 })?;
    out.digits(&scaled[..whole])?;
    if precision > 0 {
        out.text(".")?;
        out.zeros(precision.saturating_sub(scaled.len()))?;
        out.digits(&scaled[whole..])?;
    }
    Ok(out.finish())
}

/// Refuses a precision so large that generating its digits would be a denial of
/// service. ECMAScript's own domain for all three conversions is `0..=100`, and d3
/// narrows that to `0..=20` and `1..=21`, so nothing reachable comes close.
fn check_precision(precision: u32, limits: &FormatLimits) -> Result<(), FormatError> {
    if precision as usize >= limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: precision as usize,
            maximum: limits.max_output_bytes,
        });
    }
    Ok(())
}

/// ECMAScript `Number.prototype.toExponential`.
///
/// `None` selects the shortest round-tripping coefficient, which is what d3 reaches
/// through `formatDecimalParts`'s falsy-precision path.
pub fn to_exponential(
    x: f64,
    precision: Option<u32>,
    limits: &FormatLimits,
) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    if x.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    let negative = x < 0.0;
    if negative {
        out.text("-")?;
    }
    let magnitude = if negative { -x } else { x };
    if !magnitude.is_finite() {
        out.text("Infinity")?;
        return Ok(out.finish());
    }

    match precision {
        Some(precision) => {
            check_precision(precision, limits)?;
            let count = precision as usize + 1;
            let (digits, exponent) = match split(magnitude) {
                None => (zero_digits(count, limits)?, 0),
                Some(value) => {
                    let mut generator = Generator::new(value)?;
                    let (mut digits, half) = generator.take(count, limits)?;
                    let mut exponent = generator.exponent;
                    if round_half_away(&mut digits, half) {
                        exponent += 1;
                    }
                    (digits, exponent)
                }
            };
            write_exponential(&mut out, &digits, exponent)?;
        }
        None => match shortest(magnitude, limits)? {
            None => write_exponential(&mut out, &[0], 0)?,
            Some((digits, exponent)) => write_exponential(&mut out, &digits, exponent)?,
        },
    }
    Ok(out.finish())
}

/// ECMAScript `Number.prototype.toPrecision`.
///
/// Notation is chosen from the exponent *after* rounding, so a value that rounds up
/// across a power of ten switches to exponential form.
pub fn to_precision(x: f64, precision: u32, limits: &FormatLimits) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    if x.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    let negative = x < 0.0;
    if negative {
        out.text("-")?;
    }
    let magnitude = if negative { -x } else { x };
    if !magnitude.is_finite() {
        out.text("Infinity")?;
        return Ok(out.finish());
    }

    check_precision(precision, limits)?;
    let count = (precision as usize).max(1);
    let (digits, exponent) = match split(magnitude) {
        None => (zero_digits(count, limits)?, 0),
        Some(value) => {
            let mut generator = Generator::new(value)?;
            let (mut digits, half) = generator.take(count, limits)?;
            let mut exponent = generator.exponent;
            if round_half_away(&mut digits, half) {
                exponent += 1;
            }
            (digits, exponent)
        }
    };

    if exponent < -6 || exponent >= count as i32 {
        write_exponential(&mut out, &digits, exponent)?;
    } else if exponent == count as i32 - 1 {
        out.digits(&digits)?;
    } else if exponent >= 0 {
        let whole = exponent as usize + 1;
        out.digits(&digits[..whole])?;
        out.text(".")?;
        out.digits(&digits[whole..])?;
    } else {
        out.text("0.")?;
        out.zeros((-exponent - 1) as usize)?;
        out.digits(&digits)?;
    }
    Ok(out.finish())
}

/// Base-10 `Number::toString`: the shortest round-tripping decimal, in fixed
/// notation for magnitudes in `[1e-6, 1e21)` and exponential notation outside it.
pub fn number_to_string(x: f64, limits: &FormatLimits) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    if x.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    let negative = x < 0.0;
    if negative {
        out.text("-")?;
    }
    let magnitude = if negative { -x } else { x };
    write_number(&mut out, magnitude, limits)?;
    Ok(out.finish())
}

/// The unsigned body of [`number_to_string`], shared with `toFixed`'s `1e21`
/// fallback so that the two cannot drift apart.
fn write_number(out: &mut Out, magnitude: f64, limits: &FormatLimits) -> Result<(), FormatError> {
    if !magnitude.is_finite() {
        return out.text("Infinity");
    }
    let (digits, exponent) = match shortest(magnitude, limits)? {
        // `String(-0)` is "0"; the sign was already decided by the caller and
        // negative zero is not negative for this purpose.
        None => return out.text("0"),
        Some(parts) => parts,
    };
    // ECMAScript states the rule in terms of `n`, the position just past the last
    // integer digit, which is one more than the leading digit's place value.
    let n = exponent + 1;
    let k = digits.len() as i32;
    if n >= k && n <= 21 {
        out.digits(&digits)?;
        out.zeros((n - k) as usize)
    } else if n > 0 && n <= 21 {
        out.digits(&digits[..n as usize])?;
        out.text(".")?;
        out.digits(&digits[n as usize..])
    } else if n > -6 && n <= 0 {
        out.text("0.")?;
        out.zeros((-n) as usize)?;
        out.digits(&digits)
    } else {
        write_exponential(out, &digits, exponent)
    }
}

/// ECMAScript `Math.round`.
///
/// Named for the way d3 reaches it: the integer types take `Math.abs(value)` first
/// and only then round, so every reachable argument is non-negative or NaN. The
/// full semantics are implemented anyway, because they are what makes the ordering
/// in section 4.4 observable -- `Math.round(-2.5)` is `-2`, while rounding the
/// magnitude gives `3`.
pub fn js_round_nonnegative(x: f64) -> f64 {
    // Node preserves the payload and the sign of a NaN through `Math.round`, so the
    // bit pattern has to survive untouched.
    if x.is_nan() || x.is_infinite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    // `x - floor(x)` is the fractional part and is always exact.
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 {
        -0.0
    } else {
        rounded
    }
}

/// Type `d`.
///
/// A separate rule from `toFixed`'s `1e21` fallback, at the same threshold: above
/// it d3 asks `toLocaleString("en")` and strips the grouping, which expands the
/// *shortest* digits positionally rather than printing the exact integer the bits
/// denote.
pub fn format_decimal(x: f64, limits: &FormatLimits) -> Result<String, FormatError> {
    let rounded = js_round_nonnegative(x);
    if rounded.abs() >= FIXED_NOTATION_LIMIT {
        to_locale_string_en(rounded, limits)
    } else {
        number_to_string(rounded, limits)
    }
}

/// `x.toLocaleString("en")` with the grouping separators removed, as type `d`
/// consumes it.
///
/// d3 only reaches this with an integral magnitude of at least `1e21`, or with an
/// infinity. The fractional path is implemented so the function is total: the
/// default `Intl.NumberFormat` settings keep at most three fraction digits, round
/// them half away from zero, and drop trailing zeros -- and they do that to the
/// shortest decimal, not to the exact value.
pub fn to_locale_string_en(x: f64, limits: &FormatLimits) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    if x.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    if x.is_sign_negative() {
        out.text("-")?;
    }
    let magnitude = x.abs();
    if magnitude.is_infinite() {
        out.text("\u{221e}")?;
        return Ok(out.finish());
    }
    let (digits, exponent) = match shortest(magnitude, limits)? {
        None => {
            out.text("0")?;
            return Ok(out.finish());
        }
        Some(parts) => parts,
    };

    // Lay the digits out positionally, then apply the fraction-digit cap.
    let mut whole: Vec<u8>;
    let mut fraction: Vec<u8>;
    if exponent >= 0 {
        let integer_len = exponent as usize + 1;
        if digits.len() <= integer_len {
            whole = digit_vec(integer_len, limits)?;
            whole.extend_from_slice(&digits);
            whole.resize(integer_len, 0);
            fraction = Vec::new();
        } else {
            whole = digit_vec(integer_len, limits)?;
            whole.extend_from_slice(&digits[..integer_len]);
            fraction = digit_vec(digits.len() - integer_len, limits)?;
            fraction.extend_from_slice(&digits[integer_len..]);
        }
    } else {
        whole = digit_vec(1, limits)?;
        whole.push(0);
        let leading = (-exponent - 1) as usize;
        fraction = digit_vec(leading + digits.len(), limits)?;
        fraction.resize(leading, 0);
        fraction.extend_from_slice(&digits);
    }

    if fraction.len() > LOCALE_MAX_FRACTION_DIGITS {
        let round_up = fraction[LOCALE_MAX_FRACTION_DIGITS] >= 5;
        fraction.truncate(LOCALE_MAX_FRACTION_DIGITS);
        if round_up && increment(&mut fraction) && increment(&mut whole) {
            whole
                .try_reserve(1)
                .map_err(|_| FormatError::AllocationFailed)?;
            whole.insert(0, 1);
        }
    }
    while fraction.last() == Some(&0) {
        fraction.pop();
    }

    out.digits(&whole)?;
    if !fraction.is_empty() {
        out.text(".")?;
        out.digits(&fraction)?;
    }
    Ok(out.finish())
}

/// `Math.round(x).toString(radix)`, the shape types `b`, `o`, `x` and `X` use.
///
/// The expansion is exact. For the radices d3 actually reaches -- 2, 8 and 16 --
/// that is also what V8 produces, because a binary64 integer's low digits in a
/// power-of-two radix are genuinely zero. `Number.prototype.toString` is only
/// "a generalization of" `Number::toString` in the specification, though, and V8's
/// generalization stops emitting digits once the remainder falls below half an ulp
/// and pads with zeros: `(1e21).toString(36)` is `5v1j4f4ds7c000` in Node and
/// `5v1j4f4ds79m9s` exactly. Nothing in d3-format reaches a non-power-of-two radix,
/// so the port keeps the exact answer rather than reproducing an approximation that
/// the specification does not require and that no d3 output depends on.
pub fn round_to_string_radix(
    x: f64,
    radix: u32,
    limits: &FormatLimits,
) -> Result<String, FormatError> {
    let mut out = Out::new(limits);
    let rounded = js_round_nonnegative(x);
    if rounded.is_nan() {
        out.text("NaN")?;
        return Ok(out.finish());
    }
    if rounded < 0.0 {
        out.text("-")?;
    }
    let magnitude = rounded.abs();
    if magnitude.is_infinite() {
        out.text("Infinity")?;
        return Ok(out.finish());
    }
    let value = match split(magnitude) {
        None => {
            out.text("0")?;
            return Ok(out.finish());
        }
        Some(value) => value,
    };

    let mut integer = if value.exponent >= 0 {
        BigNat::try_from_u64_shl(value.significand, value.exponent as u32)?
    } else {
        // `Math.round` leaves an integer, so the discarded bits are all zero.
        let mut shifted = BigNat::try_from_u64_shl(value.significand, 0)?;
        shifted.shr_assign(value.exponent.unsigned_abs());
        shifted
    };
    if integer.is_zero() {
        out.text("0")?;
        return Ok(out.finish());
    }

    let bits_per_digit = match radix {
        2 => Some(1),
        4 => Some(2),
        8 => Some(3),
        16 => Some(4),
        32 => Some(5),
        _ => None,
    };
    // A radix-`r` digit carries at least one bit, so the bit length is an upper
    // bound on the digit count. For `f64::MAX` in binary that is the full 1,024.
    let mut digits = digit_vec(integer.bit_len() as usize, limits)?;
    while !integer.is_zero() {
        match bits_per_digit {
            Some(width) => {
                digits.push(integer.low_bits(width) as u8);
                integer.shr_assign(width);
            }
            None => digits.push(integer.divmod_small(radix) as u8),
        }
    }
    out.reserve(digits.len())?;
    for digit in digits.iter().rev() {
        out.buffer.push(char::from(if *digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        }));
    }
    Ok(out.finish())
}

/// `src/formatDecimal.js`'s `formatDecimalParts`.
///
/// Returns the significant digits with the decimal point removed, and the exponent,
/// exactly as d3 slices them out of `toExponential`. `Some(0)` is d3's falsy `p`:
/// it selects the shortest coefficient rather than underflowing to `toExponential(-1)`.
pub fn format_decimal_parts(
    x: f64,
    precision: Option<u32>,
    limits: &FormatLimits,
) -> Result<Option<(String, i32)>, FormatError> {
    if !x.is_finite() || x == 0.0 {
        return Ok(None);
    }
    let precision = precision.filter(|&p| p != 0);
    let text = to_exponential(x, precision.map(|p| p - 1), limits)?;
    let split_at = match text.find('e') {
        None => return Ok(None),
        Some(index) => index,
    };
    let coefficient = &text[..split_at];
    let exponent = text[split_at + 1..].parse::<i32>().unwrap_or(0);

    let mut digits = Out::new(limits);
    if coefficient.len() > 1 {
        digits.text(&coefficient[..1])?;
        digits.text(&coefficient[2..])?;
    } else {
        digits.text(coefficient)?;
    }
    Ok(Some((digits.finish(), exponent)))
}

/// `src/exponent.js`: the decimal exponent of `|x|`, or `None` where d3 yields NaN.
pub fn exponent(x: f64) -> Option<i32> {
    let limits = FormatLimits::DEFAULT;
    match shortest(x.abs(), &limits) {
        Ok(Some((_, exponent))) => Some(exponent),
        Ok(None) | Err(_) => None,
    }
}

/// `src/formatRounded.js`, the whole of type `r`.
pub fn format_rounded(
    x: f64,
    precision: u32,
    limits: &FormatLimits,
) -> Result<String, FormatError> {
    let (coefficient, exponent) = match format_decimal_parts(x, Some(precision), limits)? {
        None => return number_to_string(x, limits),
        Some(parts) => parts,
    };
    let mut out = Out::new(limits);
    let length = coefficient.len() as i32;
    if exponent < 0 {
        out.text("0.")?;
        out.zeros((-exponent - 1) as usize)?;
        out.text(&coefficient)?;
    } else if length > exponent + 1 {
        let whole = (exponent + 1) as usize;
        out.text(&coefficient[..whole])?;
        out.text(".")?;
        out.text(&coefficient[whole..])?;
    } else {
        out.text(&coefficient)?;
        out.zeros((exponent - length + 1) as usize)?;
    }
    Ok(out.finish())
}

/// `src/formatPrefixAuto.js`, the whole of type `s`.
///
/// Returns the coefficient and the SI prefix exponent d3 leaves in its module-level
/// `prefixExponent`; `None` there is d3's `undefined`, which suppresses the suffix.
pub fn format_prefix_auto(
    x: f64,
    precision: u32,
    limits: &FormatLimits,
) -> Result<(String, Option<i32>), FormatError> {
    let (coefficient, exponent) = match format_decimal_parts(x, Some(precision), limits)? {
        None => return Ok((to_precision(x, precision, limits)?, None)),
        Some(parts) => parts,
    };
    let prefix_exponent = clamp_si(exponent);
    let offset = exponent - prefix_exponent + 1;
    let length = coefficient.len() as i32;

    let mut out = Out::new(limits);
    if offset == length {
        out.text(&coefficient)?;
    } else if offset > length {
        out.text(&coefficient)?;
        out.zeros((offset - length) as usize)?;
    } else if offset > 0 {
        out.text(&coefficient[..offset as usize])?;
        out.text(".")?;
        out.text(&coefficient[offset as usize..])?;
    } else {
        // Below one yocto. d3 re-derives the coefficient at a lower precision, and
        // `Math.max(0, p + i - 1)` can be zero -- which is falsy, and therefore
        // means "shortest", not "minus one".
        let inner = (precision as i32 + offset - 1).max(0);
        let inner = if inner == 0 { None } else { Some(inner as u32) };
        let digits = match format_decimal_parts(x, inner, limits)? {
            None => String::new(),
            Some((digits, _)) => digits,
        };
        out.text("0.")?;
        out.zeros((-offset) as usize)?;
        out.text(&digits)?;
    }
    Ok((out.finish(), Some(prefix_exponent)))
}

/// `Math.max(-8, Math.min(8, Math.floor(e / 3))) * 3`, with the floor division
/// section 4.4 requires.
fn clamp_si(exponent: i32) -> i32 {
    exponent.div_euclid(3).clamp(-8, 8) * 3
}

/// The SI prefix exponent `formatPrefix` derives from its reference value, or
/// `None` where d3's arithmetic yields NaN.
pub fn si_prefix_exponent(reference: f64) -> Option<i32> {
    exponent(reference).map(clamp_si)
}

/// `Math.pow(10, -e)` for `formatPrefix`, read from the pinned table.
///
/// A zero, NaN or infinite reference has no decimal exponent, and d3's arithmetic
/// carries that through to a NaN scale. Nothing here casts a NaN to an integer or
/// indexes the prefix table with one.
pub fn si_prefix_scale(reference: f64) -> f64 {
    match si_prefix_exponent(reference) {
        None => f64::NAN,
        Some(exponent) => {
            let index = (exponent / 3 + 8) as usize;
            match SI_SCALES.get(index) {
                Some(bits) => f64::from_bits(*bits),
                None => f64::NAN,
            }
        }
    }
}

/// `k * value`, the multiplication `formatPrefix` performs. Never a division.
pub fn si_prefix_scaled(reference: f64, value: f64) -> f64 {
    si_prefix_scale(reference) * value
}

/// The percent scaling of types `%` and `p`: one ordinary `f64` multiplication.
pub fn percent_scaled(x: f64) -> f64 {
    x * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> FormatLimits {
        FormatLimits::DEFAULT
    }

    #[test]
    fn si_scale_table_matches_its_decimal_literals() {
        let literals: [f64; 17] = [
            1e24, 1e21, 1e18, 1e15, 1e12, 1e9, 1e6, 1e3, 1e0, 1e-3, 1e-6, 1e-9, 1e-12, 1e-15,
            1e-18, 1e-21, 1e-24,
        ];
        for (index, literal) in literals.iter().enumerate() {
            assert_eq!(
                f64::from_bits(SI_SCALES[index]).to_bits(),
                literal.to_bits(),
                "SI scale {index}"
            );
        }
    }

    #[test]
    fn splits_the_extremes_exactly() {
        assert_eq!(split(0.0), None);
        assert_eq!(split(-0.0), None);
        assert_eq!(split(f64::NAN), None);
        assert_eq!(split(f64::INFINITY), None);
        assert_eq!(
            split(1.0),
            Some(Split {
                significand: 1 << 52,
                exponent: -52
            })
        );
        assert_eq!(
            split(f64::from_bits(1)),
            Some(Split {
                significand: 1,
                exponent: -1074
            })
        );
        assert_eq!(
            split(f64::MAX),
            Some(Split {
                significand: (1 << 53) - 1,
                exponent: 971
            })
        );
    }

    #[test]
    fn the_two_tie_rules_are_different_rules() {
        // Both spellings are 17 significant digits and both read back as this
        // value, so `Number::toString` is choosing between them -- and it chooses
        // the even one, while `toFixed` chooses the larger one.
        // 2^50 + 1/4. The ulp at 2^50 is exactly 1/4, so both terms and the sum are
        // exact; written as a literal, clippy reads the extra digit as excessive
        // precision and suggests the shortest spelling, which is the very thing
        // under test here.
        let value = 1_125_899_906_842_624.0_f64 + 0.25;
        assert_eq!(
            number_to_string(value, &limits()).unwrap(),
            "1125899906842624.2"
        );
        assert_eq!(to_fixed(value, 1, &limits()).unwrap(), "1125899906842624.3");
        assert_eq!(
            to_precision(value, 17, &limits()).unwrap(),
            "1125899906842624.3"
        );
    }

    #[test]
    fn shortest_reproduces_the_ecmascript_spellings() {
        let cases: [(f64, &str, i32); 8] = [
            (1.0, "1", 0),
            (1.5, "15", 0),
            (0.1, "1", -1),
            (1e21, "1", 21),
            (f64::MAX, "17976931348623157", 308),
            (f64::from_bits(1), "5", -324),
            (f64::MIN_POSITIVE, "22250738585072014", -308),
            (1.29e-30, "129", -30),
        ];
        for (value, digits, exponent) in cases {
            let (actual, actual_exponent) = shortest(value, &limits()).unwrap().unwrap();
            let rendered: String = actual.iter().map(|d| (b'0' + d) as char).collect();
            assert_eq!(
                (rendered.as_str(), actual_exponent),
                (digits, exponent),
                "{value}"
            );
        }
    }

    #[test]
    fn rounding_is_half_away_from_zero_not_half_even() {
        // 2.5 and 0.125 are exact binary values, so both ties are exact. Half even
        // would answer "2" and "0.12"; ECMAScript answers "3" and "0.13".
        assert_eq!(to_precision(2.5, 1, &limits()).unwrap(), "3");
        assert_eq!(to_precision(0.125, 2, &limits()).unwrap(), "0.13");
        assert_eq!(to_fixed(0.125, 2, &limits()).unwrap(), "0.13");
        assert_eq!(to_fixed(2.5, 0, &limits()).unwrap(), "3");

        let (_, _, half) = significant_digits(0.125, 2, &limits()).unwrap().unwrap();
        assert_eq!(half, Half::Exact, "0.125 is an exact tie at two digits");
    }

    #[test]
    fn the_1e21_threshold_is_two_separate_rules() {
        assert_eq!(to_fixed(1e21, 2, &limits()).unwrap(), "1e+21");
        assert_eq!(
            format_decimal(1e21, &limits()).unwrap(),
            "1000000000000000000000"
        );
    }

    #[test]
    fn negative_zero_keeps_ecmascripts_sign_policy() {
        assert_eq!(number_to_string(-0.0, &limits()).unwrap(), "0");
        assert_eq!(to_fixed(-0.0, 2, &limits()).unwrap(), "0.00");
        assert_eq!(to_fixed(-0.0001, 2, &limits()).unwrap(), "-0.00");
        assert_eq!(to_exponential(-0.0, Some(2), &limits()).unwrap(), "0.00e+0");
        assert_eq!(js_round_nonnegative(-0.4).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn radix_output_reaches_the_1024_bit_representation_of_f64_max() {
        let binary = round_to_string_radix(f64::MAX, 2, &limits()).unwrap();
        assert_eq!(binary.len(), 1024);
        assert!(binary.starts_with("1111111111111111111111111111111111111111111111111111"));
        assert!(binary.ends_with('0'));
        assert_eq!(
            round_to_string_radix(f64::MAX, 16, &limits())
                .unwrap()
                .len(),
            256
        );
    }

    #[test]
    fn a_nan_reference_never_indexes_the_prefix_table() {
        for reference in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(si_prefix_scale(reference).is_nan(), "{reference}");
            assert_eq!(si_prefix_exponent(reference), None);
        }
        assert_eq!(si_prefix_scale(-1.5e-7).to_bits(), 1e9f64.to_bits());
        assert_eq!(si_prefix_exponent(-1.5e-7), Some(-9));
    }

    #[test]
    fn output_limits_are_enforced_rather_than_allocated() {
        let tight = FormatLimits {
            max_width_utf16: 1,
            max_output_bytes: 4,
        };
        assert!(matches!(
            to_fixed(1.0, 20, &tight),
            Err(FormatError::OutputLimit { .. })
        ));
        assert_eq!(to_fixed(1.0, 2, &tight).unwrap(), "1.00");
    }
}
