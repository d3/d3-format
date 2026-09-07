//! Locale-data assertions ported from `test/locale-test.js`.

const PHASE_4: &str =
    "Phase 4 placeholder: Locale construction from a generated definition does not exist yet";

/// `locale data is valid`, from `test/locale-test.js`.
///
/// The oracle walks the 59 locale JSON files and asserts that `currency`,
/// `decimal`, `grouping` and `thousands` are present in each, then constructs a
/// locale from it. `gen-locales.mjs` enforces the same schema at generation time;
/// this test enforces it on the generated table, which is what the crate actually
/// ships.
#[test]
#[ignore = "Phase 4 placeholder: Locale construction from a generated definition does not exist yet"]
fn every_generated_locale_declares_the_required_keys() {
    crate::placeholder(
        PHASE_4,
        "all 59 generated locales to declare currency, decimal, \
         grouping and thousands, and to build a Locale successfully",
    );
}
