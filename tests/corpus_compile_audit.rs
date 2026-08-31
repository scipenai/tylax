//! Compilation audit over the merged OCR math corpus (the "错题本").
//!
//! Converts every corpus entry with tylax and feeds the concatenated output to
//! the real `typst` compiler in one document. It asserts only that the generated
//! Typst is *syntactically valid* — not that it matches any reference rendering
//! (exact-output regressions live in `corpus_golden.rs`). This catches the class
//! of bug where a conversion emits an unbalanced delimiter or an invalid token.
//!
//! Ignored by default: it shells out to `typst` and needs the `data-loading`
//! feature. Run with:
//!
//! ```text
//! cargo test --test corpus_compile_audit --features data-loading -- --ignored
//! ```

#![cfg(feature = "data-loading")]

use std::process::Command;

use serde::Deserialize;
use tylax::latex_to_typst;

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    latex: String,
}

#[test]
#[ignore = "shells out to the real typst compiler; run with --features data-loading -- --ignored"]
fn merged_corpus_compiles() {
    let cases: Vec<CorpusCase> =
        serde_json::from_str(include_str!("corpus_merged.json")).expect("valid merged corpus JSON");

    let mut source = String::new();
    for (index, case) in cases.iter().enumerate() {
        let output = latex_to_typst(&case.latex);
        source.push_str(&format!("// CASE {index}: {}\n", case.id));
        source.push_str(&format!("#let corpus_case_{index} = ${}$\n", output.trim()));
    }

    let stem = format!("tylax-corpus-{}", std::process::id());
    let input = std::env::temp_dir().join(format!("{stem}.typ"));
    let output = std::env::temp_dir().join(format!("{stem}.pdf"));
    std::fs::write(&input, source).expect("write temporary Typst source");

    let result = Command::new("typst")
        .arg("compile")
        .arg(&input)
        .arg(&output)
        .output()
        .expect("Typst must be installed for the corpus audit");

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success(),
        "generated Typst must compile ({} cases); typst says:\n{}",
        cases.len(),
        stderr
    );
}
