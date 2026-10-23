//! Batch LaTeX math → Typst conversion for the visual diff in
//! `tools/visual_diff/`.
//!
//! Reads a formula list, either a JSON array or JSONL of `{"id", "latex"}`
//! objects (other fields are ignored), and writes one JSONL line
//! `{"id", "latex", "typst"}` per formula. A panic gives `"typst": null` and
//! an `"error"` field instead of stopping the batch.
//!
//! ```text
//! cargo run --release --example l2t_batch -- tests/corpus_merged.json > out.jsonl
//! ```

use std::io::{BufWriter, Read, Write};
use std::panic;

use serde_json::{json, Value};
use tylax::latex_to_typst;

fn read_formulas(text: &str) -> Vec<Value> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<Value>>(trimmed).expect("valid JSON array")
    } else {
        trimmed
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("valid JSONL line"))
            .collect()
    }
}

fn main() {
    let mut text = String::new();
    match std::env::args().nth(1).as_deref() {
        None | Some("-") => {
            std::io::stdin()
                .read_to_string(&mut text)
                .expect("read stdin");
        }
        Some(path) => text = std::fs::read_to_string(path).expect("read formula list"),
    }

    // Keep panics out of stderr: they are reported per formula instead.
    panic::set_hook(Box::new(|_| {}));

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for (index, entry) in read_formulas(&text).into_iter().enumerate() {
        let id = match &entry["id"] {
            Value::String(s) => s.clone(),
            Value::Null => index.to_string(),
            other => other.to_string(),
        };
        let latex = entry["latex"].as_str().unwrap_or_default().to_string();
        let record = match panic::catch_unwind(|| latex_to_typst(&latex)) {
            Ok(typst) => json!({"id": id, "latex": latex, "typst": typst.trim()}),
            Err(cause) => {
                let message = cause
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "panic".to_string());
                json!({"id": id, "latex": latex, "typst": null, "error": message})
            }
        };
        writeln!(out, "{record}").expect("write JSONL");
    }
}
