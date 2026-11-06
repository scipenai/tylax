//! Panic and hang target for `tylax::typst_document_to_latex`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = tylax::typst_document_to_latex(input);
    }
});
