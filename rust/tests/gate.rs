//! Standard library semantics the port relies on.
//!
//! These assertions are the compatibility contract from §2.3 of
//! `MIGRATION_TO_RUST.md`. They run under Rust 1.80.0 (MSRV), the pinned
//! 1.98.1 channel, and current stable. If one of them ever fails, the code
//! must not paper over it: either drop the reliance on that standard library
//! behavior or revise the contract with evidence.

#[test]
fn rust_semantics_used_by_the_port() {
    assert_eq!(format!("{:.0}", 2.5_f64), "2");
    assert_eq!(format!("{:.2}", 0.125_f64), "0.12");
    assert_eq!(format!("{}", 1e21_f64), "1000000000000000000000");
    assert_eq!(format!("{}", 42.0_f64), "42");
    assert_eq!(format!("{}", f64::MAX).len(), 309);
    assert_eq!(format!("{:e}", 1.29e-30_f64), "1.29e-30");
    assert_eq!((-1.5_f64).round(), -2.0);
    assert_eq!(0.5_f64.round_ties_even(), 0.0);
    assert_eq!((-1_i32).div_euclid(3), -1);
    assert_eq!(1e40_f64 as u128, u128::MAX);
    assert!("".parse::<f64>().is_err());
    assert_eq!("inf".parse::<f64>().unwrap(), f64::INFINITY);
    assert_eq!("😀".len(), 4);
    assert_eq!("😀".chars().map(char::len_utf16).sum::<usize>(), 2);
    assert_eq!(0.07_f64 * 100.0, 7.000000000000001);
    assert_eq!(1e-9_f64.to_bits(), 0x3E11_2E0B_E826_D695);
    assert_eq!(1e-9_f64 * 12_345_678_900.0, 12.345678900000001);
    assert_ne!(1e-9_f64 * 12_345_678_900.0, 12_345_678_900.0 / 1e9);
}
