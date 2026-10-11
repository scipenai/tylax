//! Compilation audit over a LaTeX math corpus.
//!
//! Converts every corpus entry with tylax and compiles the output with the real
//! `typst` compiler. It checks only that the generated Typst compiles, not that
//! it matches any reference rendering (exact-output regressions live in
//! `corpus_golden.rs`). This catches the class of bug where a conversion emits
//! an unbalanced delimiter, an invalid token or an unknown name.
//!
//! By default it audits the merged OCR math corpus (the "错题本") and asserts
//! that every entry compiles. Environment variables:
//!
//! - `TYLAX_CORPUS=<path>`: audit this file instead, either a JSON array of
//!   `{"id", "latex"}` objects or plain text with one formula per line. The test
//!   then reports counts and does not fail on a compile error.
//! - `TYLAX_CORPUS_REPORT=<path>`: write one JSON object per failing case
//!   (`id`, `latex`, `typst`, `kind`, `errors`) to this JSONL file. Each
//!   error is `{"message", "column"}`, where `column` is the character offset
//!   into `typst` that Typst reports, or `null`.
//! - `TYLAX_CORPUS_OUTPUTS=<path>`: write every case (`id`, `latex`, `typst`)
//!   to this JSONL file, whether it compiles or not.
//! - `TYLAX_CORPUS_LIMIT=<n>`: audit only the first `n` cases.
//!
//! Cases are compiled in batches, one formula per line, so that each error can
//! be traced to its case by line number. Each case a batch blames is compiled
//! alone to confirm the error and to record its own messages, since an
//! unclosed string can carry an error onto a later line.
//!
//! Ignored by default: it shells out to `typst` and needs the `data-loading`
//! feature. Run with:
//!
//! ```text
//! cargo test --release --features data-loading --test corpus_compile_audit -- --ignored --nocapture
//! ```

#![cfg(feature = "data-loading")]

use std::collections::BTreeSet;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tylax::latex_to_typst;

const BATCH_SIZE: usize = 100;
const COMPILE_TIMEOUT: Duration = Duration::from_secs(60);
const PREAMBLE: &str = "#set page(width: 30cm, height: auto)\n";

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    latex: String,
}

/// A converted case. `typst` is `None` when the conversion panicked.
struct Case {
    id: String,
    latex: String,
    typst: Option<String>,
}

struct Failure {
    index: usize,
    kind: &'static str,
    errors: Vec<CompileError>,
}

struct CompileError {
    /// Index of the formula in the compiled batch, or `usize::MAX` if the
    /// error is not on a formula line.
    position: usize,
    /// 0-based character offset into the formula.
    column: Option<usize>,
    message: String,
}

impl CompileError {
    fn unplaced(message: String) -> Self {
        CompileError {
            position: usize::MAX,
            column: None,
            message,
        }
    }
}

enum Outcome {
    Ok,
    Errors(Vec<CompileError>),
    Timeout,
}

/// Read a corpus file: a JSON array of `{id, latex}`, or one formula per line
/// with ids `<file stem>:<line number>`.
fn load_corpus(path: &Path) -> Vec<CorpusCase> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read corpus {}: {error}", path.display()));
    if text.trim_start().starts_with('[') {
        return serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("invalid corpus JSON in {}: {error}", path.display()));
    }
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    text.split('\n')
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| CorpusCase {
            id: format!("{stem}:{}", index + 1),
            latex: line.trim_end_matches('\r').to_string(),
        })
        .collect()
}

/// Convert every case, catching panics so that one bad input does not end the
/// audit. The output is flattened onto one line; a newline is whitespace in
/// Typst math.
fn convert_all(cases: Vec<CorpusCase>) -> Vec<Case> {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let converted = cases
        .into_iter()
        .map(|case| {
            let typst = panic::catch_unwind(AssertUnwindSafe(|| latex_to_typst(&case.latex)))
                .ok()
                .map(|output| output.trim().replace(['\r', '\n'], " "));
            Case {
                id: case.id,
                latex: case.latex,
                typst,
            }
        })
        .collect();
    panic::set_hook(previous_hook);
    converted
}

/// Compile `formulas` as one document, one formula per line after the
/// preamble, and return the errors with the index of the formula they are on.
fn compile(dir: &Path, name: &str, formulas: &[&str]) -> Outcome {
    let input = dir.join(format!("{name}.typ"));
    let output = dir.join(format!("{name}.pdf"));
    let mut source = String::from(PREAMBLE);
    for formula in formulas {
        // A formula that is one display environment converts to a whole
        // equation, which must not be wrapped again. Keep it on one line.
        let formula = formula.trim();
        if formula.starts_with('$') || formula.starts_with("#math.equation") {
            source.push_str(&formula.replace('\n', " "));
            source.push('\n');
        } else {
            source.push_str(&format!("${formula}$\n"));
        }
    }
    std::fs::write(&input, source).expect("write temporary Typst source");

    let mut child = Command::new("typst")
        .args([
            "compile",
            "--ignore-system-fonts",
            "--diagnostic-format",
            "short",
        ])
        .arg("--root")
        .arg(dir)
        .arg(&input)
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Typst must be installed for the corpus audit");
    let stderr = child.stderr.take().expect("piped stderr");
    let reader = std::thread::spawn(move || std::io::read_to_string(stderr).unwrap_or_default());
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for typst") {
            break Some(status);
        }
        if start.elapsed() > COMPILE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stderr = reader.join().unwrap_or_default();
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);

    let Some(status) = status else {
        return Outcome::Timeout;
    };
    if status.success() {
        return Outcome::Ok;
    }
    let preamble_lines = PREAMBLE.lines().count();
    let errors = stderr
        .lines()
        .filter_map(|line| {
            // `<path>:<line>:<column>: error: <message>`; warnings are skipped.
            let (position, message) = line.split_once(": error: ")?;
            let mut parts = position.rsplitn(3, ':');
            // Typst's column is 0-based and counts the opening `$`.
            let column = parts.next()?.parse::<usize>().ok();
            let line_number = parts.next()?.parse::<usize>().ok();
            let position = line_number
                .and_then(|n| n.checked_sub(preamble_lines + 1))
                .unwrap_or(usize::MAX);
            Some(CompileError {
                position,
                column: column.and_then(|c| c.checked_sub(1)),
                message: message.to_string(),
            })
        })
        .collect::<Vec<_>>();
    if errors.is_empty() {
        // Exit status without a parsable error: blame no line in particular.
        Outcome::Errors(vec![CompileError::unplaced(stderr.trim().to_string())])
    } else {
        Outcome::Errors(errors)
    }
}

/// Compile one case alone and return its failure, if any.
fn check_alone(dir: &Path, name: &str, index: usize, typst: &str) -> Option<Failure> {
    match compile(dir, name, &[typst]) {
        Outcome::Ok => None,
        Outcome::Timeout => Some(Failure {
            index,
            kind: "timeout",
            errors: vec![CompileError::unplaced(format!(
                "typst took longer than {COMPILE_TIMEOUT:?}"
            ))],
        }),
        Outcome::Errors(errors) => Some(Failure {
            index,
            kind: "compile",
            errors,
        }),
    }
}

/// Audit one batch of case indices. Compile the batch; confirm each case it
/// blames by compiling that case alone; drop the blamed cases and repeat. When
/// no blamed case fails alone, or an error has no line, fall back to checking
/// every remaining case alone.
fn audit_batch(dir: &Path, name: &str, cases: &[Case], batch: &[usize]) -> Vec<Failure> {
    let mut failures = Vec::new();
    let mut remaining: Vec<usize> = batch
        .iter()
        .copied()
        .filter(|&index| {
            if cases[index].typst.is_none() {
                failures.push(Failure {
                    index,
                    kind: "panic",
                    errors: vec![CompileError::unplaced(
                        "latex_to_typst panicked".to_string(),
                    )],
                });
                false
            } else {
                true
            }
        })
        .collect();

    while !remaining.is_empty() {
        let formulas: Vec<&str> = remaining
            .iter()
            .map(|&index| cases[index].typst.as_deref().unwrap())
            .collect();
        let blamed: BTreeSet<usize> = match compile(dir, name, &formulas) {
            Outcome::Ok => break,
            Outcome::Timeout => BTreeSet::new(),
            Outcome::Errors(errors) => errors
                .iter()
                .map(|error| error.position)
                .filter(|&position| position < remaining.len())
                .collect(),
        };
        let mut confirmed = false;
        for &position in &blamed {
            let index = remaining[position];
            if let Some(failure) = check_alone(dir, name, index, formulas[position]) {
                failures.push(failure);
                confirmed = true;
            }
        }
        if !confirmed {
            for &index in &remaining {
                let typst = cases[index].typst.as_deref().unwrap();
                failures.extend(check_alone(dir, name, index, typst));
            }
            break;
        }
        remaining = remaining
            .iter()
            .enumerate()
            .filter(|(position, _)| !blamed.contains(position))
            .map(|(_, &index)| index)
            .collect();
    }
    failures
}

/// Audit every case on all cores and return the failures in corpus order.
fn audit(cases: &[Case]) -> Vec<Failure> {
    let dir = std::env::temp_dir().join(format!("tylax-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let batches: Vec<Vec<usize>> = (0..cases.len())
        .collect::<Vec<_>>()
        .chunks(BATCH_SIZE)
        .map(<[usize]>::to_vec)
        .collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for worker in 0..workers {
            let (dir, batches, next, done, failures) = (&dir, &batches, &next, &done, &failures);
            scope.spawn(move || loop {
                let batch_index = next.fetch_add(1, Ordering::Relaxed);
                let Some(batch) = batches.get(batch_index) else {
                    break;
                };
                let found = audit_batch(dir, &format!("worker{worker}"), cases, batch);
                failures.lock().unwrap().extend(found);
                let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                if finished % 100 == 0 {
                    eprintln!("corpus audit: {finished}/{} batches", batches.len());
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
    let mut failures = failures.into_inner().unwrap();
    failures.sort_by_key(|failure| failure.index);
    failures
}

fn write_report(path: &Path, cases: &[Case], failures: &[Failure]) {
    let file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create report {}: {error}", path.display()));
    let mut writer = std::io::BufWriter::new(file);
    for failure in failures {
        let case = &cases[failure.index];
        let record = serde_json::json!({
            "id": case.id,
            "latex": case.latex,
            "typst": case.typst,
            "kind": failure.kind,
            "errors": failure
                .errors
                .iter()
                .map(|error| serde_json::json!({"message": error.message, "column": error.column}))
                .collect::<Vec<_>>(),
        });
        writeln!(writer, "{record}").expect("write report line");
    }
}

fn write_outputs(path: &Path, cases: &[Case]) {
    let file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create {}: {error}", path.display()));
    let mut writer = std::io::BufWriter::new(file);
    for case in cases {
        let record = serde_json::json!({"id": case.id, "latex": case.latex, "typst": case.typst});
        writeln!(writer, "{record}").expect("write output line");
    }
}

#[test]
#[ignore = "shells out to the real typst compiler; run with --features data-loading -- --ignored"]
fn corpus_compiles() {
    let corpus_path = std::env::var_os("TYLAX_CORPUS").map(PathBuf::from);
    let mut corpus = match &corpus_path {
        Some(path) => load_corpus(path),
        None => serde_json::from_str(include_str!("corpus_merged.json"))
            .expect("valid merged corpus JSON"),
    };
    if let Some(limit) = std::env::var("TYLAX_CORPUS_LIMIT")
        .ok()
        .and_then(|limit| limit.parse().ok())
    {
        corpus.truncate(limit);
    }

    let start = Instant::now();
    let cases = convert_all(corpus);
    let converted_in = start.elapsed();
    let failures = audit(&cases);

    if let Some(path) = std::env::var_os("TYLAX_CORPUS_REPORT") {
        write_report(Path::new(&path), &cases, &failures);
    }
    if let Some(path) = std::env::var_os("TYLAX_CORPUS_OUTPUTS") {
        write_outputs(Path::new(&path), &cases);
    }

    let count = |kind| failures.iter().filter(|f| f.kind == kind).count();
    let total = cases.len();
    let rate = if total == 0 {
        0.0
    } else {
        100.0 * failures.len() as f64 / total as f64
    };
    eprintln!(
        "corpus audit: {total} cases, {} failed ({rate:.2}%): {} compile errors, {} panics, \
         {} timeouts; converted in {converted_in:.1?}, total {:.1?}",
        failures.len(),
        count("compile"),
        count("panic"),
        count("timeout"),
        start.elapsed(),
    );

    if corpus_path.is_none() {
        let details: Vec<String> = failures
            .iter()
            .map(|failure| {
                let case = &cases[failure.index];
                format!(
                    "{} ({}): {}\n  typst: {}",
                    case.id,
                    failure.kind,
                    failure
                        .errors
                        .iter()
                        .map(|error| error.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                    case.typst.as_deref().unwrap_or("<panic>"),
                )
            })
            .collect();
        assert!(
            failures.is_empty(),
            "generated Typst must compile ({total} cases); failures:\n{}",
            details.join("\n")
        );
    }
}
