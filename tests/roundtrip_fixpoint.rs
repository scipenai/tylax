//! Round-trip fixpoint tests.
//!
//! For LaTeX math `L`, convert `T1 = latex_to_typst(L)`, then `L2 =
//! typst_to_latex(T1)` in math-only mode, then `T2 = latex_to_typst(L2)`. Once
//! the first round has normalised the input, the second round must change
//! nothing: `T1 == T2`. A difference means one direction loses or invents
//! information, without a human reading either output. A second check runs
//! `check_latex` on `L2`: an error there means T → L wrote invalid LaTeX, even
//! when `T2` still matches.
//!
//! The corpus test pins which entries of `corpus_merged.json` are unstable
//! today and why, so it fails both on a new instability and when a known one
//! is fixed. The generator test compares tokens, since Typst ignores
//! differences in spacing.

use tylax::diagnostics::{check_latex, DiagnosticLevel};
use tylax::{latex_to_typst, typst_to_latex_with_options, T2LOptions};
use typst_syntax::{SyntaxKind, SyntaxNode};

struct RoundTrip {
    t1: String,
    l2: String,
    t2: String,
}

fn round_trip(latex: &str) -> RoundTrip {
    let t1 = latex_to_typst(latex).trim().to_string();
    let l2 = typst_to_latex_with_options(&t1, &T2LOptions::math_only())
        .trim()
        .to_string();
    let t2 = latex_to_typst(&l2).trim().to_string();
    RoundTrip { t1, l2, t2 }
}

/// The error messages `check_latex` reports for `latex`: unbalanced braces or
/// environments, and parser syntax errors. Its warnings and notes are about
/// support for a command or package, not validity, so they are left out.
fn latex_errors(latex: &str) -> Vec<String> {
    check_latex(latex)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.level == DiagnosticLevel::Error)
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// The leaf tokens of `typst` parsed as math, without whitespace. Two outputs
/// with the same tokens differ only in spacing, which Typst ignores between
/// tokens (`f (x)= x` and `f (x) = x`), while `ab` and `a b` stay distinct.
fn tokens(typst: &str) -> Vec<String> {
    fn walk(node: &SyntaxNode, out: &mut Vec<String>) {
        if node.children().len() == 0 && node.kind() != SyntaxKind::Space {
            out.push(node.text().to_string());
        }
        for child in node.children() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(&typst_syntax::parse_math(typst), &mut out);
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Stable,
    /// `T1` and `T2` have the same tokens but different whitespace.
    Spacing,
    Unstable,
    /// `check_latex` reports an error in `L2`, whatever `T2` is.
    InvalidLatex,
}

fn outcome(round: &RoundTrip) -> Outcome {
    if !latex_errors(&round.l2).is_empty() {
        Outcome::InvalidLatex
    } else if round.t1 == round.t2 {
        Outcome::Stable
    } else if tokens(&round.t1) == tokens(&round.t2) {
        Outcome::Spacing
    } else {
        Outcome::Unstable
    }
}

/// Why a corpus entry is not a fixpoint, with a minimal input for each.
#[derive(Clone, Copy, Debug)]
enum Cause {
    /// L → T copies source whitespace: `F(x)=x` → `F (x)= x`, then `F (x) = x`.
    Spacing,
    /// L → T writes two scripts of one kind in a row, which Typst nests:
    /// `{x^{a}}^{b}` → `x^(a)^(b)` → `x^{\left(a\right)^b}`.
    StackedScripts,
    /// L → T escapes a delimiter that pairs only across a TeX group, and the
    /// group is gone on the way back: `\left\{ x { \right\} }` → `{ x \}`
    /// → `\{x \}` → `{x }`.
    CrossGroupDelimiter,
}

/// Corpus entries that are not a fixpoint today, by cause. Where an entry has
/// more than one cause, it is listed under the first one the diff shows.
const KNOWN_UNSTABLE: &[(Cause, &[&str])] = &[
    (
        Cause::Spacing,
        &[
            "m0", "m1", "m2", "m3", "m7", "m9", "m11", "m33", "m34", "m35", "m36", "m51", "m68",
            "m69", "m79", "m82", "m83", "m107", "m116", "m117", "m119", "m121", "m122", "m125",
            "m127", "m128", "m129", "m130", "m131", "m132", "m136", "m165", "m177", "m189", "m190",
            "m191", "m192", "m193", "m217", "m231", "m232", "m234", "m235", "m239", "m241", "m243",
            "m246",
        ],
    ),
    // m305 is a LaTeX double subscript, `_{a}_{b}`.
    (Cause::StackedScripts, &["m305"]),
    (Cause::CrossGroupDelimiter, &["m310"]),
];

fn expected_outcome(id: &str) -> (Outcome, Option<Cause>) {
    for (cause, ids) in KNOWN_UNSTABLE {
        if ids.contains(&id) {
            let outcome = match cause {
                Cause::Spacing => Outcome::Spacing,
                _ => Outcome::Unstable,
            };
            return (outcome, Some(*cause));
        }
    }
    (Outcome::Stable, None)
}

#[test]
fn merged_corpus_round_trip_is_a_fixpoint() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("corpus_merged.json")).expect("valid merged corpus JSON");
    let ids: Vec<&str> = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    for (_, known) in KNOWN_UNSTABLE {
        for id in *known {
            assert!(
                ids.contains(id),
                "KNOWN_UNSTABLE lists `{id}`, which is not in the corpus"
            );
        }
    }

    let mut mismatches = Vec::new();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let latex = case["latex"].as_str().unwrap();
        let round = round_trip(latex);
        let got = outcome(&round);
        let (expected, cause) = expected_outcome(id);
        if got != expected {
            mismatches.push(format!(
                "  {id}: expected {expected:?} (cause {cause:?}), got {got:?}\n    L1: {latex}\n    T1: {}\n    L2: {}\n    T2: {}",
                round.t1, round.l2, round.t2
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "round-trip outcomes changed; update KNOWN_UNSTABLE:\n{}",
        mismatches.join("\n")
    );
}

// Grammar of valid LaTeX math for the generator tests.

const GREEK: &[&str] = &[
    r"\alpha",
    r"\beta",
    r"\gamma",
    r"\delta",
    r"\epsilon",
    r"\varepsilon",
    r"\zeta",
    r"\eta",
    r"\theta",
    r"\vartheta",
    r"\iota",
    r"\kappa",
    r"\lambda",
    r"\mu",
    r"\nu",
    r"\xi",
    r"\pi",
    r"\rho",
    r"\sigma",
    r"\tau",
    r"\upsilon",
    r"\phi",
    r"\varphi",
    r"\chi",
    r"\psi",
    r"\omega",
    r"\Gamma",
    r"\Delta",
    r"\Theta",
    r"\Lambda",
    r"\Xi",
    r"\Pi",
    r"\Sigma",
    r"\Phi",
    r"\Psi",
    r"\Omega",
];
const SYMBOLS: &[&str] = &[
    r"\infty",
    r"\partial",
    r"\nabla",
    r"\emptyset",
    r"\forall",
    r"\exists",
    r"\neg",
    r"\ell",
    r"\hbar",
    r"\angle",
    r"\triangle",
    r"\cdots",
    r"\ldots",
    r"\prime",
];
const BINARY: &[&str] = &[
    "+",
    "-",
    "/",
    r"\pm",
    r"\mp",
    r"\times",
    r"\div",
    r"\cdot",
    r"\cup",
    r"\cap",
    r"\setminus",
    r"\circ",
    r"\oplus",
    r"\otimes",
    r"\wedge",
    r"\vee",
];
const RELATIONS: &[&str] = &[
    "=",
    "<",
    ">",
    ",",
    r"\leq",
    r"\geq",
    r"\neq",
    r"\approx",
    r"\equiv",
    r"\sim",
    r"\simeq",
    r"\cong",
    r"\propto",
    r"\in",
    r"\notin",
    r"\subset",
    r"\subseteq",
    r"\supset",
    r"\to",
    r"\rightarrow",
    r"\leftarrow",
    r"\Rightarrow",
    r"\Leftrightarrow",
    r"\mapsto",
    r"\perp",
    r"\parallel",
    r"\mid",
];
const FUNCTIONS: &[&str] = &[
    r"\sin", r"\cos", r"\tan", r"\log", r"\ln", r"\exp", r"\max", r"\min", r"\det",
];
const LARGE_OPERATORS: &[&str] = &[r"\sum", r"\prod", r"\int", r"\oint", r"\bigcup", r"\bigcap"];
const FONTS: &[&str] = &[
    r"\mathbf",
    r"\mathrm",
    r"\mathit",
    r"\mathsf",
    r"\mathtt",
    r"\boldsymbol",
];
/// Fonts that only take a capital letter.
const LETTER_FONTS: &[&str] = &[r"\mathcal", r"\mathbb", r"\mathfrak"];
const ACCENTS: &[&str] = &[
    r"\hat",
    r"\bar",
    r"\vec",
    r"\dot",
    r"\ddot",
    r"\tilde",
    r"\overline",
    r"\underline",
    r"\widehat",
    r"\widetilde",
];
const DELIMITERS: &[(&str, &str)] = &[
    ("(", ")"),
    ("[", "]"),
    (r"\{", r"\}"),
    ("|", "|"),
    (r"\|", r"\|"),
    (r"\langle", r"\rangle"),
    (r"\lfloor", r"\rfloor"),
    (r"\lceil", r"\rceil"),
    (".", "."),
];
const MATRICES: &[&str] = &["matrix", "pmatrix", "bmatrix", "vmatrix"];

/// SplitMix64, seeded per case so that every run generates the same inputs.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) % n as u64) as usize
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// Between `min` and `max` characters of `set`.
    fn chars(&mut self, set: &str, min: usize, max: usize) -> String {
        let set: Vec<char> = set.chars().collect();
        let len = min + self.below(max - min + 1);
        (0..len).map(|_| self.pick(&set)).collect()
    }
}

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";

/// A single symbol: a valid script base on its own.
fn atom(rng: &mut Rng) -> String {
    match rng.below(9) {
        0..=2 => rng.chars(&format!("{LOWER}{UPPER}"), 1, 1),
        3..=4 if rng.below(2) == 0 => rng.chars(DIGITS, 1, 3),
        3..=4 => format!("{}.{}", rng.chars(DIGITS, 1, 3), rng.chars(DIGITS, 1, 2)),
        5..=6 => rng.pick(GREEK).to_string(),
        7 => rng.pick(SYMBOLS).to_string(),
        _ => format!("{}{{{}}}", rng.pick(LETTER_FONTS), rng.chars(UPPER, 1, 1)),
    }
}

/// Whether a braced group of `latex` may be a script base. L → T drops the
/// braces, so a script or fraction inside joins the outer script:
/// `{x^{a}}^{b}` → `x^(a)^(b)` and `{\frac{a}{b}}_{c}` → `a/b_(c)`. Typst has no
/// invisible grouping to keep them apart, so this cause is won't-fix, and the
/// generator leaves it out so that the tests find others.
fn braced_base_allowed(latex: &str) -> bool {
    !latex.contains(['^', '_']) && !latex.contains(r"\frac")
}

fn leaf(rng: &mut Rng) -> String {
    match rng.below(8) {
        0..=4 => atom(rng),
        5 => format!("{} {}", rng.pick(FUNCTIONS), atom(rng)),
        6 => format!("{}{}", rng.chars(LOWER, 1, 1), rng.chars("'", 1, 2)),
        _ if rng.below(2) == 0 => format!(r"\text{{{}}}", rng.chars(LOWER, 1, 6)),
        _ => format!(
            r"\text{{{} {}}}",
            rng.chars(LOWER, 1, 6),
            rng.chars(LOWER, 1, 6)
        ),
    }
}

fn script_base(rng: &mut Rng, depth: u32) -> String {
    let atom = atom(rng);
    if rng.below(2) == 0 {
        return atom;
    }
    let group = latex_math(rng, depth);
    if braced_base_allowed(&group) {
        format!("{{{group}}}")
    } else {
        atom
    }
}

/// A LaTeX math expression at most `depth` levels deep. Script bases are atoms
/// or braced groups, so no output has a double script, and `\text` holds only
/// plain words.
fn latex_math(rng: &mut Rng, depth: u32) -> String {
    if depth == 0 || rng.below(3) == 0 {
        return leaf(rng);
    }
    let d = depth - 1;
    match rng.below(16) {
        0 => {
            let n = 2 + rng.below(2);
            (0..n)
                .map(|_| latex_math(rng, d))
                .collect::<Vec<_>>()
                .join(" ")
        }
        1 => format!(
            "{} {} {}",
            latex_math(rng, d),
            rng.pick(BINARY),
            latex_math(rng, d)
        ),
        2 => format!(
            "{} {} {}",
            latex_math(rng, d),
            rng.pick(RELATIONS),
            latex_math(rng, d)
        ),
        3 => format!("-{}", latex_math(rng, d)),
        4 => format!(r"\frac{{{}}}{{{}}}", latex_math(rng, d), latex_math(rng, d)),
        5 => format!(
            r"\binom{{{}}}{{{}}}",
            latex_math(rng, d),
            latex_math(rng, d)
        ),
        6 => format!(r"\sqrt{{{}}}", latex_math(rng, d)),
        7 => format!(
            r"\sqrt[{}]{{{}}}",
            rng.chars("23456789", 1, 1),
            latex_math(rng, d)
        ),
        8 => format!("{}^{{{}}}", script_base(rng, d), latex_math(rng, d)),
        9 => format!("{}_{{{}}}", script_base(rng, d), latex_math(rng, d)),
        10 => format!(
            "{}_{{{}}}^{{{}}}",
            script_base(rng, d),
            latex_math(rng, d),
            latex_math(rng, d)
        ),
        11 => format!(
            "{}_{{{}}}^{{{}}} {}",
            rng.pick(LARGE_OPERATORS),
            latex_math(rng, d),
            latex_math(rng, d),
            latex_math(rng, d)
        ),
        12 => {
            let (l, r) = rng.pick(DELIMITERS);
            format!(r"\left{l} {} \right{r}", latex_math(rng, d))
        }
        13 => format!("{}{{{}}}", rng.pick(FONTS), latex_math(rng, d)),
        14 => format!("{}{{{}}}", rng.pick(ACCENTS), latex_math(rng, d)),
        _ if rng.below(2) == 0 => {
            let env = rng.pick(MATRICES);
            let (rows, cols) = (1 + rng.below(3), 1 + rng.below(3));
            let rows: Vec<String> = (0..rows)
                .map(|_| {
                    (0..cols)
                        .map(|_| latex_math(rng, d))
                        .collect::<Vec<_>>()
                        .join(" & ")
                })
                .collect();
            format!(r"\begin{{{env}}} {} \end{{{env}}}", rows.join(r" \\ "))
        }
        _ => {
            let n = 1 + rng.below(3);
            let rows: Vec<String> = (0..n)
                .map(|_| format!("{} & {}", latex_math(rng, d), latex_math(rng, d)))
                .collect();
            format!(r"\begin{{cases}} {} \end{{cases}}", rows.join(r" \\ "))
        }
    }
}

/// Ignores whitespace, so that spacing does not hide the other causes. Runs
/// `ROUNDTRIP_CASES` cases, else 1,000. A failure prints its input; its fix
/// adds a test to integration_tests.rs.
#[test]
fn generated_math_round_trip_is_a_fixpoint_up_to_spacing() {
    let cases = std::env::var("ROUNDTRIP_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(1000);
    for seed in 0..cases {
        let latex = latex_math(&mut Rng(seed), 4);
        let round = round_trip(&latex);
        assert_eq!(
            latex_errors(&round.l2),
            Vec::<String>::new(),
            "L1: {latex}\nT1: {}\nL2: {}",
            round.t1,
            round.l2
        );
        assert_eq!(
            tokens(&round.t1),
            tokens(&round.t2),
            "L1: {latex}\nT1: {}\nL2: {}\nT2: {}",
            round.t1,
            round.l2,
            round.t2
        );
    }
}
