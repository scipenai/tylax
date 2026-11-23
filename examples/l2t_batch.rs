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

use std::any::Any;
use std::cell::RefCell;
use std::io::{BufWriter, Read, Write};
use std::panic::{self, AssertUnwindSafe};

use serde_json::{json, Value};
use tylax::latex_to_typst;

thread_local! {
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn format_panic_payload(payload: &(dyn Any + Send)) -> &str {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        msg
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.as_str()
    } else {
        "panic"
    }
}

struct PanicHookGuard<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for PanicHookGuard<F> {
    fn drop(&mut self) {
        if let Some(restore) = self.0.take() {
            restore();
        }
    }
}

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
    let previous_hook = panic::take_hook();
    let _guard = PanicHookGuard(Some(move || panic::set_hook(previous_hook)));
    panic::set_hook(Box::new(|info| {
        let msg = format_panic_payload(info.payload());
        let detail = match info.location() {
            Some(loc) => format!("at {}:{}:{}: {msg}", loc.file(), loc.line(), loc.column()),
            None => msg.to_string(),
        };
        LAST_PANIC.with(|slot| *slot.borrow_mut() = Some(detail));
    }));

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for (index, entry) in read_formulas(&text).into_iter().enumerate() {
        let id = match &entry["id"] {
            Value::String(s) => s.clone(),
            Value::Null => index.to_string(),
            other => other.to_string(),
        };
        let latex = entry["latex"].as_str().unwrap_or_default().to_string();
        LAST_PANIC.with(|slot| *slot.borrow_mut() = None);
        let record = match panic::catch_unwind(AssertUnwindSafe(|| latex_to_typst(&latex))) {
            Ok(typst) => json!({"id": id, "latex": latex, "typst": typst.trim()}),
            Err(cause) => {
                let message = LAST_PANIC
                    .with(|slot| slot.borrow_mut().take())
                    .unwrap_or_else(|| format_panic_payload(cause.as_ref()).to_string());
                json!({"id": id, "latex": latex, "typst": null, "error": message})
            }
        };
        writeln!(out, "{record}").expect("write JSONL");
    }
}
