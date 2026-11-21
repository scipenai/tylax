//! Expansion limits of the LaTeX macro engine.
//!
//! `with_max_depth` bounds how deeply expansions nest (not how many macros a
//! document uses), `with_max_tokens` bounds tokens produced by expansion (not
//! the length of the document), thousands of definitions or `\expandafter` /
//! `\csname` uses do not exhaust the stack, and a `\makeatletter` definition
//! leaves the rest of the input untouched.

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

use tylax::core::latex2typst::engine::{
    detokenize, expand_latex, expand_latex_with_warnings, tokenize, Engine, EngineWarning,
};

const ALOT: usize = 20_000;

/// Uses per expandafter/csname test: thousands, with total output far below the default token limit.
const MANY_USES: usize = 5_000;

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The last `n` characters of `s` (the end of a diagnostic is the useful part).
fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    chars[chars.len().saturating_sub(n)..].iter().collect()
}

fn has_depth_warning(w: &[EngineWarning]) -> bool {
    w.iter()
        .any(|w| matches!(w, EngineWarning::DepthExceeded { .. }))
}

fn has_token_warning(w: &[EngineWarning]) -> bool {
    w.iter()
        .any(|w| matches!(w, EngineWarning::TokenLimitExceeded { .. }))
}

fn has_limit_warning(w: &[EngineWarning]) -> bool {
    has_depth_warning(w) || has_token_warning(w)
}

/// Run an engine with the given configuration; returns (output, warnings).
fn run_engine(mut engine: Engine, input: &str) -> (String, Vec<EngineWarning>) {
    let out = engine.process(tokenize(input));
    let warnings = engine.take_structured_warnings();
    (detokenize(&out), warnings)
}

/// A letters-only control-word name, unique per index.
fn alpha_name(prefix: &str, mut i: usize) -> String {
    let mut s = String::from(prefix);
    loop {
        s.push((b'a' + (i % 26) as u8) as char);
        i /= 26;
        if i == 0 {
            break;
        }
    }
    s
}

/// `n` uses of `body` separated by spaces.
fn repeat_uses(body: &str, n: usize) -> String {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(body);
        s.push(' ');
    }
    s
}

/// Plain prose of roughly `words` words.
fn plain_text(words: usize) -> String {
    let vocab = [
        "lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
    ];
    let mut s = String::new();
    for i in 0..words {
        s.push_str(vocab[i % vocab.len()]);
        s.push(if i % 17 == 16 { '\n' } else { ' ' });
    }
    s
}

/// A chain `\zqnest<0>` -> `\zqnest<1>` -> (and so on) -> `\zqnest<depth-1>` -> `leaf`.
/// Returns (definitions, name of the outermost macro).
fn nesting_chain(depth: usize, leaf: &str) -> (String, String) {
    let mut defs = String::new();
    for i in 0..depth {
        let name = alpha_name("zqnest", i);
        let body = if i + 1 == depth {
            leaf.to_string()
        } else {
            format!("\\{}", alpha_name("zqnest", i + 1))
        };
        defs.push_str(&format!("\\def\\{}{{{}}}", name, body));
    }
    (defs, alpha_name("zqnest", 0))
}

/// Doubling bomb: level 0 is `xx`, level k is two copies of level k-1.
/// Returns (definitions, name of the top macro).
fn doubling_bomb(levels: usize) -> (String, String) {
    let mut defs = String::from(r"\def\zqbomba{xx}");
    for i in 1..levels {
        defs.push_str(&format!(
            "\\def\\{}{{\\{}\\{}}}",
            alpha_name("zqbomb", i),
            alpha_name("zqbomb", i - 1),
            alpha_name("zqbomb", i - 1)
        ));
    }
    (defs, alpha_name("zqbomb", levels - 1))
}

fn document(body: &str) -> String {
    format!(
        "\\documentclass{{article}}\n\\begin{{document}}\n{}\n\\end{{document}}\n",
        body
    )
}

/// Run the `t2l` CLI (LaTeX -> Typst) in a child process so that a stack
/// overflow cannot take down the test harness.
fn run_cli(input: &str) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_t2l"))
        .args(["-d", "l2t"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn t2l");
    let mut stdin = child.stdin.take().expect("stdin");
    let data = input.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(data.as_bytes());
    });
    let out = child.wait_with_output().expect("wait t2l");
    let _ = writer.join();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Convert with the CLI and require a successful exit. Inputs without a
/// `\documentclass` are wrapped in a minimal document so that the converter
/// treats them as prose.
fn cli_ok(input: &str) -> String {
    let full = if input.contains(r"\documentclass") {
        input.to_string()
    } else {
        document(input)
    };
    let (ok, stdout, stderr) = run_cli(&full);
    assert!(
        ok,
        "t2l did not exit successfully; stderr: {}",
        tail(&stderr, 600)
    );
    stdout
}

/// Alphanumeric runs of the output.
fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

#[test]
fn sequential_zero_arg_uses_beyond_default_limit_all_expand() {
    let input = format!(
        r"\newcommand{{\myR}}{{\mathbb{{R}}}} {}",
        repeat_uses(r"\myR", 250)
    );
    let r = expand_latex_with_warnings(&input, false);
    assert_eq!(count(&r.output, r"\mathbb{R}"), 250);
    assert!(!r.output.contains(r"\myR"));
    assert!(!has_depth_warning(&r.warnings), "{:?}", r.warnings);
}

#[test]
fn sequential_one_arg_uses_keep_their_body() {
    let input = format!(
        r"\newcommand{{\vecnorm}}[1]{{\|#1\|}} {}",
        repeat_uses(r"\vecnorm{qv}", 1000)
    );
    let r = expand_latex_with_warnings(&input, false);
    let out = strip_ws(&r.output);
    assert_eq!(count(&out, r"\|qv\|"), 1000);
    assert_eq!(
        count(&out, "qv"),
        1000,
        "no bare argument leftovers expected"
    );
    assert!(!has_depth_warning(&r.warnings), "{:?}", r.warnings);
}

#[test]
fn definition_after_many_uses_still_defines() {
    let input = format!(
        r"\def\xa{{a}} {} \newcommand{{\Foo}}[1]{{FOO<#1>}} \Foo{{arg}}",
        repeat_uses(r"\xa", 300)
    );
    let out = strip_ws(&expand_latex(&input));
    assert!(
        out.contains("FOO<arg>"),
        "{}",
        &out[out.len().saturating_sub(80)..]
    );
    assert!(!out.contains(r"\Foo"));
    assert!(!out.contains("newcommand"));
}

#[test]
fn iftrue_conditional_after_many_uses_still_works() {
    let input = format!(
        r"\def\xa{{a}}\def\Foo{{YESBRANCH}} {} \iftrue \Foo\else NOBRANCH\fi",
        repeat_uses(r"\xa", 300)
    );
    let out = expand_latex(&input);
    assert!(
        out.contains("YESBRANCH"),
        "{}",
        &out[out.len().saturating_sub(80)..]
    );
    assert!(!out.contains("NOBRANCH"));
    assert!(!out.contains(r"\Foo"));
}

#[test]
fn user_environment_after_many_uses_still_works() {
    let input = format!(
        r"\def\xa{{a}}\def\Foo{{BEGINMARK}}\def\Bar{{ENDMARK}} \newenvironment{{zqbox}}{{\Foo}}{{\Bar}} {} \begin{{zqbox}}inside\end{{zqbox}}",
        repeat_uses(r"\xa", 300)
    );
    let out = strip_ws(&expand_latex(&input));
    assert!(
        out.contains("BEGINMARKinsideENDMARK"),
        "{}",
        &out[out.len().saturating_sub(80)..]
    );
    assert!(!out.contains("zqbox"));
}

#[test]
fn many_uses_without_recursion_report_no_depth_warning() {
    let input = format!(
        r"\newcommand{{\myR}}{{\mathbb{{R}}}}\newcommand{{\vecnorm}}[1]{{\|#1\|}} {}",
        repeat_uses(r"$\vecnorm{x} \in \myR$", 500)
    );
    let r = expand_latex_with_warnings(&input, false);
    assert!(
        !has_depth_warning(&r.warnings),
        "{:?}",
        &r.warnings[..r.warnings.len().min(3)]
    );
    assert_eq!(count(&r.output, r"\mathbb{R}"), 500);
}

#[test]
fn small_depth_limit_allows_thousand_sequential_uses() {
    let input = format!(r"\def\xa{{QZ}} {}", repeat_uses(r"\xa", 1000));
    let (out, warnings) = run_engine(Engine::new().with_max_depth(5), &input);
    assert_eq!(count(&out, "QZ"), 1000);
    assert!(
        !has_depth_warning(&warnings),
        "{:?}",
        &warnings[..warnings.len().min(3)]
    );
}

#[test]
fn nesting_beyond_limit_warns_after_many_sequential_uses() {
    let (defs, top) = nesting_chain(60, "DEEPLEAF");
    let input = format!(
        r"\def\xa{{QZ}} {} {} \{}",
        defs,
        repeat_uses(r"\xa", 300),
        top
    );
    let (out, warnings) = run_engine(Engine::new().with_max_depth(10), &input);
    assert_eq!(count(&out, "QZ"), 300);
    assert!(has_depth_warning(&warnings));
}

#[test]
fn csname_chain_counts_only_macro_nesting() {
    let input = format!(
        r"\def\xq{{QZ}}\def\xd{{LEAF}}\def\xc{{\csname xd\endcsname}}\def\xb{{\csname xc\endcsname}}\def\xa{{\csname xb\endcsname}} {} \xa",
        repeat_uses(r"\xq", 300)
    );
    let (out, warnings) = run_engine(Engine::new().with_max_depth(8), &input);
    assert_eq!(count(&out, "QZ"), 300);
    assert!(
        out.contains("LEAF"),
        "{:?}",
        &out[out.len().saturating_sub(80)..]
    );
    assert!(!has_depth_warning(&warnings), "{:?}", warnings);
}

#[test]
fn direct_recursion_warns_and_later_macro_expands() {
    let input = r"\def\xb{AFTERMARK}\def\xa{\xa} \xa \xb";
    let (out, warnings) = run_engine(Engine::new().with_max_depth(20), input);
    assert!(has_depth_warning(&warnings));
    assert!(out.contains("AFTERMARK"), "{:?}", out);
}

#[test]
fn indirect_recursion_warns_and_later_macro_expands() {
    let input = r"\def\Foo{AFTERMARK}\def\xa{\xb}\def\xb{\xa} \xa \Foo";
    let (out, warnings) = run_engine(Engine::new().with_max_depth(20), input);
    assert!(has_depth_warning(&warnings));
    assert!(out.contains("AFTERMARK"), "{:?}", out);
}

#[test]
fn expandafter_tail_recursion_warns_and_later_macro_expands() {
    let input = r"\def\xb{AFTERMARK}\def\zqe{}\def\xa{\expandafter\xa\zqe} \xa \xb";
    let (out, warnings) = run_engine(Engine::new().with_max_depth(20), input);
    assert!(has_depth_warning(&warnings), "{:?}", warnings);
    assert!(out.contains("AFTERMARK"), "{:?}", out);
}

#[test]
fn document_definitions_conditionals_environments_after_250_uses() {
    let body = format!(
        "\\newcommand{{\\myR}}{{\\mathbb{{R}}}}\n{}\n\\newcommand{{\\Foo}}{{FOOBODY}}\n\\Foo\n\\newenvironment{{zqbox}}{{BEGINMARK}}{{ENDMARK}}\n\\begin{{zqbox}}inside\\end{{zqbox}}\n\\iftrue YESBRANCH\\else NOBRANCH\\fi\n",
        repeat_uses(r"$\myR$", 250)
    );
    let out = strip_ws(&tylax::latex_document_to_typst(&document(&body)));
    assert!(
        out.contains("FOOBODY"),
        "{}",
        &out[out.len().saturating_sub(120)..]
    );
    assert!(out.contains("BEGINMARKinsideENDMARK"));
    assert!(out.contains("YESBRANCH"));
    assert!(!out.contains("NOBRANCH"));
}

#[test]
fn large_plain_document_then_macro_expands_without_token_warning() {
    let input = format!(
        r"\newcommand{{\vecnorm}}[1]{{\|#1\|}} {} \vecnorm{{lastarg}}",
        plain_text(30_000)
    );
    let r = expand_latex_with_warnings(&input, false);
    assert!(strip_ws(&r.output).contains(r"\|lastarg\|"));
    assert!(!has_token_warning(&r.warnings), "{:?}", r.warnings);
}

#[test]
fn small_token_limit_still_stops_doubling_bomb_after_long_text() {
    let (defs, top) = doubling_bomb(20);
    let input = format!(
        r"{} \def\xa{{BEFOREMARK}} {} \xa \{} \xa",
        defs,
        plain_text(3_000),
        top
    );
    let (out, warnings) = run_engine(Engine::new().with_max_tokens(1_000), &input);
    assert!(has_limit_warning(&warnings));
    assert!(out.contains("BEFOREMARK"));
}

#[test]
fn twenty_thousand_newcommands_convert_and_work() {
    let mut input = String::new();
    for i in 0..ALOT {
        input.push_str(&format!(
            "\\newcommand{{\\{}}}{{V{}}}",
            alpha_name("zq", i),
            i
        ));
    }
    let sample: Vec<usize> = (0..ALOT).step_by(97).chain([ALOT - 1]).collect();
    for &i in &sample {
        input.push_str(&format!("\\{}; ", alpha_name("zq", i)));
    }
    let out = words(&cli_ok(&input));
    for &i in &sample {
        assert!(out.contains(&format!("V{}", i)), "missing V{}", i);
    }
}

#[test]
fn thousands_of_expandafter_uses_in_macro_bodies() {
    let input = format!(
        r"\def\xb{{B}}\def\zqw{{\expandafter Q\xb}} {} end",
        repeat_uses(r"\zqw,", MANY_USES)
    );
    let out = cli_ok(&input);
    assert_eq!(count(&out, "QB"), MANY_USES);
}

#[test]
fn thousands_of_bare_expandafter_uses_do_not_crash() {
    let input = format!(
        r"\def\xb{{B}} {} ENDMARKER",
        repeat_uses(r"\expandafter Q\xb,", MANY_USES)
    );
    let out = cli_ok(&input);
    assert!(out.contains("QB"));
    assert!(out.contains("ENDMARKER"));
}

#[test]
fn thousands_of_csname_uses_in_macro_bodies() {
    let input = format!(
        r"\def\xb{{QB}}\def\zqc{{\csname xb\endcsname}} {} end",
        repeat_uses(r"\zqc,", MANY_USES)
    );
    let out = cli_ok(&input);
    assert_eq!(count(&out, "QB"), MANY_USES);
}

#[test]
fn thousands_of_bare_csname_uses_do_not_crash() {
    let input = format!(
        r"\def\xb{{QB}} {} ENDMARKER",
        repeat_uses(r"\csname xb\endcsname,", MANY_USES)
    );
    let out = cli_ok(&input);
    assert!(out.contains("QB"));
    assert!(out.contains("ENDMARKER"));
}

#[test]
fn at_letter_def_does_not_glue_alpha_and_x() {
    let out = expand_latex(r"\makeatletter \def\my@x{Y} \makeatother $\alpha x$");
    assert!(!out.contains(r"\alphax"), "{:?}", out);
    assert!(out.contains(r"\alpha"));
    assert!(out.contains('x'));
}

#[test]
fn at_letter_def_does_not_glue_item_and_text() {
    let out = expand_latex(
        r"\makeatletter \def\my@x{Y} \makeatother \begin{itemize} \item First \item Second \end{itemize}",
    );
    assert!(!out.contains(r"\itemFirst"), "{:?}", out);
    assert!(!out.contains(r"\itemSecond"));
    assert_eq!(count(&out, r"\item"), 2);
    assert!(out.contains("First") && out.contains("Second"));
}

#[test]
fn makeatother_takes_effect_after_at_letter_def() {
    let out =
        expand_latex(r"\newcommand{\foo}{FOOX}\makeatletter \def\my@x{Y} \makeatother \foo@bar");
    assert!(strip_ws(&out).contains("FOOX@bar"), "{:?}", out);
    assert!(!out.contains(r"\foo"));
}

#[test]
fn leading_at_name_defines_and_expands_without_gluing_later_text() {
    let out = expand_latex(r"\makeatletter\def\@zqfoo{ATFOO}\@zqfoo\makeatother $\beta y$");
    assert!(out.contains("ATFOO"), "{:?}", out);
    assert!(!out.contains(r"\betay"), "{:?}", out);
    assert!(out.contains(r"\beta"));
}

#[test]
fn cli_definitions_conditionals_environments_after_thousand_uses() {
    let input = format!(
        "\\def\\xa{{a}}\n{}\n\\newcommand{{\\Foo}}[1]{{LEFTMARK #1 RIGHTMARK}}\n\\Foo{{middle}}\n\\newenvironment{{zqbox}}{{BEGINMARK}}{{ENDMARK}}\n\\begin{{zqbox}}inside\\end{{zqbox}}\n\\iftrue YESBRANCH\\else NOBRANCH\\fi\n",
        repeat_uses(r"\xa", 1_000)
    );
    let out = strip_ws(&cli_ok(&input));
    assert!(
        out.contains("LEFTMARKmiddleRIGHTMARK"),
        "{}",
        &out[out.len().saturating_sub(160)..]
    );
    assert!(out.contains("BEGINMARKinsideENDMARK"));
    assert!(out.contains("YESBRANCH"));
    assert!(!out.contains("NOBRANCH"));
}
