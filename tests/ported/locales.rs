//! Locale-data assertions ported from `test/locale-test.js`.

/// `locale data is valid`, from `test/locale-test.js`.
///
/// The oracle walks the 59 locale JSON files and asserts that `currency`,
/// `decimal`, `grouping` and `thousands` are present in each, then constructs a
/// locale from it. The generator enforces the same schema when it emits the table;
/// this test enforces it on the generated table, which is what the crate actually
/// ships.
///
/// The table is only reachable under the `locales` feature, which is what the
/// `#[cfg]` below is for. Without the feature there is no shipped table to check,
/// so the assertion has nothing to say rather than something weaker to say; the
/// runner in `main.rs` reports the same skip.
#[test]
#[ignore = "runs in the target root; see the target root's documentation"]
pub(crate) fn every_generated_locale_declares_the_required_keys() {
    #[cfg(feature = "locales")]
    {
        use d3_format::{locales, Locale};

        let mut checked = 0_usize;
        for generated in &locales::LOCALES {
            let name = generated.name;
            assert!(generated.currency.is_some(), "{name} declares no currency");
            assert!(generated.decimal.is_some(), "{name} declares no decimal");
            assert!(generated.grouping.is_some(), "{name} declares no grouping");
            assert!(
                generated.thousands.is_some(),
                "{name} declares no thousands"
            );

            // `formatLocale(locale)` -- the oracle's last line, which must not throw.
            let locale = Locale::named(name)
                .unwrap_or_else(|| panic!("{name} is in the table but did not build"));

            // Everything the definition declared survives into the built locale,
            // which is the part `formatLocale` would silently drop.
            assert_eq!(Some(locale.decimal()), generated.decimal, "{name}");
            assert_eq!(locale.grouping(), generated.grouping, "{name}");
            assert_eq!(Some(locale.thousands()), generated.thousands, "{name}");
            let [prefix, suffix] = generated.currency.expect("checked above");
            assert_eq!(locale.currency_prefix(), prefix, "{name}");
            assert_eq!(locale.currency_suffix(), suffix, "{name}");

            checked += 1;
        }
        assert_eq!(checked, 59, "the oracle walks 59 locale files");
        assert_eq!(locales::names().count(), 59);
    }
}

/// Every assertion in this module, for the runner in the target root.
///
/// Gated on the same feature as the assertion it runs. The runner already picks a
/// branch on that feature, so an ungated definition here is dead code in the two
/// builds that do not take it, and `cargo test --no-default-features` says so.
#[cfg(feature = "locales")]
pub(crate) fn all() {
    every_generated_locale_declares_the_required_keys();
}
