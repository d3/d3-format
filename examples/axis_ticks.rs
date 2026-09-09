//! Labelling an axis the way d3 does: one SI prefix for the whole axis, and a
//! precision derived from the tick step instead of guessed.
//!
//! ```console
//! $ cargo run --example axis_ticks
//! ```

use d3_format::{as_precision, precision_prefix, Locale};

fn main() {
    let locale = Locale::en_us();

    // A linear axis from zero to 1.3 million, ticked every hundred thousand.
    let step = 100_000.0;
    let ticks: Vec<f64> = (0..=13).map(|tick| f64::from(tick) * step).collect();
    let largest = *ticks.last().expect("the axis has ticks");

    // How many decimal places a value of this magnitude needs, once an SI prefix
    // has scaled it, for one `step` to still be visible.
    let digits = as_precision(precision_prefix(step, largest))
        .expect("a finite, non-zero step has a precision");

    // The prefix is chosen once, from the reference value, so the whole column
    // shares a unit rather than drifting between k, M and G partway down.
    let label = locale
        .prefix_formatter(&format!(",.{digits}"), largest)
        .expect("a valid specifier");

    for value in ticks {
        println!("{:>8}", label.format_number(value).expect("in range"));
    }
}
