//! Arbitrary-precision natural numbers, cut down to exactly what exact `m * 2^k`
//! decimal conversion needs.
//!
//! MIGRATION_TO_RUST.md section 4.2 keeps the default runtime dependency graph
//! empty, so there is no third-party bignum or formatting crate here. The decimal
//! engine drives every conversion through one scaled long-division loop, and that
//! loop needs six things from a natural number: build it from `m * 2^k`, multiply
//! it by a small constant or by a power of ten, compare it, subtract it, and divide
//! it by a small constant. Nothing else is implemented, because nothing else is
//! reachable.
//!
//! Section 4.2 also requires that limbs use checked arithmetic and `try_reserve`.
//! Every operation that can grow the limb vector is therefore fallible and returns
//! [`FormatError`]: an allocator refusal and a length calculation that will not fit
//! in `usize` are reported rather than aborting, wrapping, or panicking.

use core::cmp::Ordering;

use crate::limits::FormatError;

/// Upper bound on the limbs a single value may occupy.
///
/// The widest intermediate any `f64` can produce is the scaled dividend for the
/// smallest subnormal: 53 significand bits, shifted by the 1074-bit denominator and
/// then multiplied by `10^324` for the decimal scaling, plus the 21 factors of ten
/// the digit loop applies. That is under 1300 bits. 8192 bits is therefore roughly
/// six times the reachable maximum, and exists only so that a caller cannot drive
/// unbounded growth through a future entry point.
const MAX_LIMBS: usize = 256;

/// Largest power of ten that fits in a limb, and its exponent.
const POW10_CHUNK: u32 = 1_000_000_000;
const POW10_CHUNK_EXP: u32 = 9;

const POW10: [u32; 10] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
];

/// A natural number, little-endian in base `2^32`.
///
/// The representation is normalized: the most significant limb is never zero, so
/// zero is the empty vector and [`Ord`] can compare lengths first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BigNat {
    limbs: Vec<u32>,
}

fn reserve(limbs: &mut Vec<u32>, additional: usize) -> Result<(), FormatError> {
    // `map_or` rather than `is_none_or`: the latter is newer than the declared
    // MSRV of 1.80, and section 6.2's MSRV job would reject it.
    if limbs
        .len()
        .checked_add(additional)
        .map_or(true, |total| total > MAX_LIMBS)
    {
        return Err(FormatError::AllocationFailed);
    }
    limbs
        .try_reserve(additional)
        .map_err(|_| FormatError::AllocationFailed)
}

impl BigNat {
    /// The number zero, holding no allocation.
    pub fn zero() -> BigNat {
        BigNat { limbs: Vec::new() }
    }

    /// `value * 2^shift`.
    pub fn try_from_u64_shl(value: u64, shift: u32) -> Result<BigNat, FormatError> {
        let mut out = BigNat::zero();
        if value == 0 {
            return Ok(out);
        }
        let limb_shift = (shift / 32) as usize;
        reserve(
            &mut out.limbs,
            limb_shift.checked_add(3).unwrap_or(MAX_LIMBS),
        )?;
        out.limbs.resize(limb_shift, 0);
        let bit_shift = shift % 32;
        let widened = (value as u128) << bit_shift;
        out.limbs.push(widened as u32);
        out.limbs.push((widened >> 32) as u32);
        out.limbs.push((widened >> 64) as u32);
        out.trim();
        Ok(out)
    }

    /// `1 << shift`.
    pub fn try_pow2(shift: u32) -> Result<BigNat, FormatError> {
        BigNat::try_from_u64_shl(1, shift)
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    /// Number of significant bits; zero has none.
    pub fn bit_len(&self) -> u32 {
        match self.limbs.last() {
            None => 0,
            Some(top) => {
                // `self.limbs.len()` is at most MAX_LIMBS, so this cannot overflow.
                (self.limbs.len() as u32 - 1) * 32 + (32 - top.leading_zeros())
            }
        }
    }

    /// Copies `other` into `self`, reusing whatever capacity `self` already has.
    pub fn try_clone_from(&mut self, other: &BigNat) -> Result<(), FormatError> {
        self.limbs.clear();
        reserve(&mut self.limbs, other.limbs.len())?;
        self.limbs.extend_from_slice(&other.limbs);
        Ok(())
    }

    /// `self *= factor`, for a factor that fits in one limb.
    pub fn try_mul_small(&mut self, factor: u32) -> Result<(), FormatError> {
        if factor == 0 {
            self.limbs.clear();
            return Ok(());
        }
        if factor == 1 || self.is_zero() {
            return Ok(());
        }
        let mut carry: u64 = 0;
        for limb in &mut self.limbs {
            let product = (*limb as u64) * (factor as u64) + carry;
            *limb = product as u32;
            carry = product >> 32;
        }
        while carry != 0 {
            reserve(&mut self.limbs, 1)?;
            self.limbs.push(carry as u32);
            carry >>= 32;
        }
        Ok(())
    }

    /// `self *= 10^exponent`.
    pub fn try_mul_pow10(&mut self, mut exponent: u32) -> Result<(), FormatError> {
        if self.is_zero() {
            return Ok(());
        }
        while exponent >= POW10_CHUNK_EXP {
            self.try_mul_small(POW10_CHUNK)?;
            exponent -= POW10_CHUNK_EXP;
        }
        if exponent > 0 {
            self.try_mul_small(POW10[exponent as usize])?;
        }
        Ok(())
    }

    /// `self += other`.
    pub fn try_add_assign(&mut self, other: &BigNat) -> Result<(), FormatError> {
        if other.is_zero() {
            return Ok(());
        }
        let extra = other.limbs.len().saturating_sub(self.limbs.len());
        if extra != 0 {
            reserve(&mut self.limbs, extra)?;
            self.limbs.resize(other.limbs.len(), 0);
        }
        let mut carry: u64 = 0;
        for (index, limb) in self.limbs.iter_mut().enumerate() {
            let addend = other.limbs.get(index).copied().unwrap_or(0) as u64;
            let sum = (*limb as u64) + addend + carry;
            *limb = sum as u32;
            carry = sum >> 32;
            if carry == 0 && index >= other.limbs.len() {
                break;
            }
        }
        if carry != 0 {
            reserve(&mut self.limbs, 1)?;
            self.limbs.push(carry as u32);
        }
        Ok(())
    }

    /// `self -= other`, which the caller must already know is no larger.
    ///
    /// The digit loop only subtracts after comparing, so the precondition holds by
    /// construction. A violation clamps to zero rather than panicking, because
    /// section 0.4 requires that formatting never panic for any `f64` bit pattern.
    pub fn sub_assign(&mut self, other: &BigNat) {
        if other.is_zero() {
            return;
        }
        if *self < *other {
            self.limbs.clear();
            return;
        }
        let mut borrow: i64 = 0;
        for (index, limb) in self.limbs.iter_mut().enumerate() {
            let subtrahend = other.limbs.get(index).copied().unwrap_or(0) as i64;
            let difference = (*limb as i64) - subtrahend - borrow;
            if difference < 0 {
                *limb = (difference + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                *limb = difference as u32;
                borrow = 0;
            }
            if borrow == 0 && index >= other.limbs.len() {
                break;
            }
        }
        self.trim();
    }

    /// `self /= divisor`, returning the remainder.
    pub fn divmod_small(&mut self, divisor: u32) -> u32 {
        if divisor == 0 || self.is_zero() {
            return 0;
        }
        let mut remainder: u64 = 0;
        for limb in self.limbs.iter_mut().rev() {
            let current = (remainder << 32) | (*limb as u64);
            *limb = (current / (divisor as u64)) as u32;
            remainder = current % (divisor as u64);
        }
        self.trim();
        remainder as u32
    }

    /// The low `bits` bits, as a value that must fit in one limb.
    ///
    /// Only used for the power-of-two radices, where `bits` is 1, 3 or 4.
    pub fn low_bits(&self, bits: u32) -> u32 {
        let mask = if bits >= 32 {
            u32::MAX
        } else {
            (1u32 << bits) - 1
        };
        match self.limbs.first() {
            None => 0,
            Some(low) => low & mask,
        }
    }

    /// `self >>= bits`.
    pub fn shr_assign(&mut self, bits: u32) {
        if bits == 0 || self.is_zero() {
            return;
        }
        let limb_shift = (bits / 32) as usize;
        if limb_shift >= self.limbs.len() {
            self.limbs.clear();
            return;
        }
        if limb_shift != 0 {
            self.limbs.drain(0..limb_shift);
        }
        let bit_shift = bits % 32;
        if bit_shift != 0 {
            let mut carry: u32 = 0;
            for limb in self.limbs.iter_mut().rev() {
                let value = *limb;
                *limb = (value >> bit_shift) | carry;
                carry = value << (32 - bit_shift);
            }
        }
        self.trim();
    }

    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }
}

impl Ord for BigNat {
    fn cmp(&self, other: &BigNat) -> Ordering {
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Equal => {}
            unequal => return unequal,
        }
        for (left, right) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
            match left.cmp(right) {
                Ordering::Equal => continue,
                unequal => return unequal,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for BigNat {
    fn partial_cmp(&self, other: &BigNat) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decimal rendering, for tests only: the engine never needs it, because it
    /// generates digits through the scaled division loop instead.
    fn decimal(value: &BigNat) -> String {
        if value.is_zero() {
            return "0".to_string();
        }
        let mut work = value.clone();
        let mut chunks = Vec::new();
        while !work.is_zero() {
            chunks.push(work.divmod_small(POW10_CHUNK));
        }
        let mut out = chunks.pop().map(|top| top.to_string()).unwrap_or_default();
        while let Some(chunk) = chunks.pop() {
            out.push_str(&format!("{chunk:09}"));
        }
        out
    }

    #[test]
    fn builds_from_a_shifted_u64() {
        let value = BigNat::try_from_u64_shl(3, 100).unwrap();
        assert_eq!(
            decimal(&value),
            "3802951800684688204490109616128" // 3 * 2^100
        );
        assert_eq!(value.bit_len(), 102);
        assert!(BigNat::try_from_u64_shl(0, 100).unwrap().is_zero());
    }

    #[test]
    fn multiplies_by_powers_of_ten() {
        let mut value = BigNat::try_from_u64_shl(7, 0).unwrap();
        value.try_mul_pow10(30).unwrap();
        assert_eq!(decimal(&value), "7000000000000000000000000000000");
    }

    #[test]
    fn adds_and_subtracts_across_limb_boundaries() {
        let mut value = BigNat::try_from_u64_shl(u64::MAX, 0).unwrap();
        let one = BigNat::try_from_u64_shl(1, 0).unwrap();
        value.try_add_assign(&one).unwrap();
        assert_eq!(decimal(&value), "18446744073709551616");
        value.sub_assign(&one);
        assert_eq!(decimal(&value), "18446744073709551615");
    }

    #[test]
    fn subtracting_more_than_it_holds_clamps_to_zero_instead_of_panicking() {
        let mut value = BigNat::try_from_u64_shl(1, 0).unwrap();
        let bigger = BigNat::try_from_u64_shl(2, 0).unwrap();
        value.sub_assign(&bigger);
        assert!(value.is_zero());
    }

    #[test]
    fn shifts_right_across_limbs_and_off_the_end() {
        let mut value = BigNat::try_from_u64_shl(1, 1000).unwrap();
        assert_eq!(value.bit_len(), 1001);
        value.shr_assign(999);
        assert_eq!(decimal(&value), "2");
        value.shr_assign(64);
        assert!(value.is_zero());
    }

    #[test]
    fn orders_by_magnitude() {
        let small = BigNat::try_from_u64_shl(1, 64).unwrap();
        let large = BigNat::try_from_u64_shl(1, 65).unwrap();
        assert!(small < large);
        assert_eq!(small.cmp(&small.clone()), Ordering::Equal);
        assert!(BigNat::zero() < small);
    }

    #[test]
    fn reaches_the_1024_bit_representation_of_f64_max() {
        // f64::MAX is (2^53 - 1) * 2^971, the widest integer any f64 denotes.
        let mantissa = (1u64 << 53) - 1;
        let value = BigNat::try_from_u64_shl(mantissa, 971).unwrap();
        assert_eq!(value.bit_len(), 1024);
        assert_eq!(decimal(&value).len(), 309);
        assert!(decimal(&value).starts_with("17976931348623157"));
    }

    #[test]
    fn refuses_growth_beyond_the_limb_cap_instead_of_allocating() {
        assert_eq!(
            BigNat::try_from_u64_shl(1, 1_000_000),
            Err(FormatError::AllocationFailed)
        );
        let mut value = BigNat::try_from_u64_shl(1, 0).unwrap();
        let mut last = Ok(());
        for _ in 0..64 {
            last = value.try_mul_pow10(1000);
            if last.is_err() {
                break;
            }
        }
        assert_eq!(last, Err(FormatError::AllocationFailed));
    }

    #[test]
    fn divides_by_a_single_limb() {
        let mut value = BigNat::try_from_u64_shl(1, 0).unwrap();
        value.try_mul_pow10(38).unwrap();
        let remainder = value.divmod_small(7);
        assert_eq!(remainder, (10u128.pow(38) % 7) as u32);
        assert_eq!(decimal(&value), (10u128.pow(38) / 7).to_string());
    }

    #[test]
    fn reads_low_bits_for_the_power_of_two_radices() {
        let mut value = BigNat::try_from_u64_shl(0b1011, 0).unwrap();
        assert_eq!(value.low_bits(1), 1);
        assert_eq!(value.low_bits(3), 0b011);
        assert_eq!(value.low_bits(4), 0b1011);
        value.shr_assign(4);
        assert!(value.is_zero());
    }
}
