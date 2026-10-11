//! Panic and hang target for `tylax::latex_to_typst`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = tylax::latex_to_typst(input);
    }
});
