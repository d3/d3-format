//! Allocation limits and the error type they report through.
//!
//! MIGRATION_TO_RUST.md section 5.4 specifies both. Section 7 lists them as Phase 3
//! deliverables while section 4.2 already makes them parameters of every decimal
//! conversion, so Phase 2 introduces exactly the part the decimal engine needs:
//! [`FormatLimits`] in full, and the two [`FormatError`] variants a numeric
//! conversion can actually produce. The specifier, width and input-kind variants
//! arrive with the types they carry, in Phase 3.

use core::fmt;

/// Bounds on what a single formatting operation may allocate.
///
/// The decimal engine only reads [`FormatLimits::max_output_bytes`]; the width
/// bound belongs to the layout stage and is carried here so that Phase 3 does not
/// have to change the shape of the type or the signatures that take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatLimits {
    /// Largest padded width, in UTF-16 code units, a formatter may be built for.
    pub max_width_utf16: u32,
    /// Largest output, in UTF-8 bytes, a single formatting call may produce.
    pub max_output_bytes: usize,
}

impl FormatLimits {
    /// The defaults from section 5.4: one million code units, sixteen mebibytes.
    pub const DEFAULT: FormatLimits = FormatLimits {
        max_width_utf16: 1_000_000,
        max_output_bytes: 16 * 1024 * 1024,
    };
}

impl Default for FormatLimits {
    fn default() -> Self {
        FormatLimits::DEFAULT
    }
}

/// Why a formatting operation could not produce a string.
///
/// Formatting is fallible by design: section 5.4 requires that no accepted input
/// trigger integer wrap, a panic, or an unbounded allocation, so the paths that
/// could do any of those return an error instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatError {
    /// The output would exceed [`FormatLimits::max_output_bytes`].
    OutputLimit {
        /// Bytes the operation would have needed.
        requested: usize,
        /// The limit in force.
        maximum: usize,
    },
    /// A buffer could not be grown.
    ///
    /// Either the allocator refused, or the request was so large that the length
    /// arithmetic could not be carried out in `usize`. Neither is reachable from a
    /// finite `f64` and a d3-reachable precision; the variant exists so that the
    /// engine has somewhere to fail rather than aborting or wrapping.
    AllocationFailed,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::OutputLimit { requested, maximum } => write!(
                f,
                "formatted output would be {requested} bytes, over the {maximum} byte limit"
            ),
            FormatError::AllocationFailed => f.write_str("allocation failed"),
        }
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_section_5_4() {
        let limits = FormatLimits::default();
        assert_eq!(limits.max_width_utf16, 1_000_000);
        assert_eq!(limits.max_output_bytes, 16 * 1024 * 1024);
        assert_eq!(limits, FormatLimits::DEFAULT);
    }

    #[test]
    fn errors_describe_themselves() {
        let error = FormatError::OutputLimit {
            requested: 20,
            maximum: 10,
        };
        assert_eq!(
            error.to_string(),
            "formatted output would be 20 bytes, over the 10 byte limit"
        );
        assert_eq!(
            FormatError::AllocationFailed.to_string(),
            "allocation failed"
        );
    }
}
