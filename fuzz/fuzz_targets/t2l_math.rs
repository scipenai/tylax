//! Panic and hang target for `tylax::typst_to_latex_with_options` in math-only mode.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = tylax::typst_to_latex_with_options(input, &tylax::T2LOptions::math_only());
    }
});
