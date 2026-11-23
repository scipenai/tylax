//! Round-trip fixpoint tests.
//!
//! For LaTeX math `L`, convert `T1 = latex_to_typst(L)`, then `L2 =
//! typst_to_latex(T1)` in math-only mode, then `T2 = latex_to_typst(L2)`. Once
//! the first round has normalised the input, the second round must change
//! nothing: `T1 == T2`. A difference means one direction loses or invents
//! information, without a human reading either output. Second and third checks
//! run `check_latex` on `L2` and `typst_syntax::parse_math` on `T1`/`T2`: an
//! error there means T → L wrote invalid LaTeX or L → T wrote invalid Typst,
//! even when the round trip repeats identically.
//!
//! The corpus test pins which entries of `corpus_merged.json` are unstable
//! today along with their diagnosed `Cause`, so it fails on a new instability,
//! when a known one is fixed, or when a known entry changes cause. The
//! generator test verifies clean LaTeX/Typst parses and compares the parsed
//! Typst math AST modulo `SyntaxKind::Space`.

use std::collections::HashSet;
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
/// environments, parser syntax errors, and structural math errors (bad scripts,
/// unpaired `\left`/`\right`, invalid delimiters, missing command arguments).
/// Its warnings and notes are about support for a command or package, not
/// validity, so they are left out.
fn latex_errors(latex: &str) -> Vec<String> {
    check_latex(latex)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.level == DiagnosticLevel::Error)
        .map(|diagnostic| diagnostic.message)
        .collect()
}

fn typst_errors(node: &SyntaxNode) -> Vec<String> {
    node.errors()
        .into_iter()
        .map(|err| err.message.to_string())
        .collect()
}

/// Structured Typst math syntax tree with `SyntaxKind::Space` trivia stripped.
///
/// Preserving node kinds keeps semantically distinct whitespace changes apart
/// (e.g., `sqrt(x)` is a `FuncCall` while `sqrt (x)` is `MathIdent` followed by
/// `MathDelimited`), while harmless inter-token spacing (`F (x)= x` vs
/// `F (x) = x`) produces identical trees.
#[derive(Debug, PartialEq, Eq)]
enum MathTree {
    Leaf(SyntaxKind, String),
    Branch(SyntaxKind, Vec<MathTree>),
}

fn math_tree(node: &SyntaxNode) -> MathTree {
    if node.children().len() == 0 {
        MathTree::Leaf(node.kind(), node.text().to_string())
    } else {
        let children = node
            .children()
            .filter(|child| child.kind() != SyntaxKind::Space)
            .map(math_tree)
            .collect();
        MathTree::Branch(node.kind(), children)
    }
}

/// Why a corpus entry is not a fixpoint, with a minimal input for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Stable,
    /// Not a strict string fixpoint, tagged with the verified cause (`None`
    /// when the diff does not match any known cause).
    Unstable(Option<Cause>),
    /// `check_latex` reports an error in `L2`, whatever `T2` is.
    InvalidLatex,
    /// `typst_syntax::parse_math` reports syntax errors in `T1` or `T2`.
    InvalidTypst,
}

fn parenthesize_stacked_scripts(tree: &MathTree) -> (MathTree, bool) {
    match tree {
        MathTree::Leaf(kind, text) => (MathTree::Leaf(*kind, text.clone()), false),
        MathTree::Branch(kind, children) => {
            let mut changed = false;
            let mut transformed = Vec::with_capacity(children.len());
            for child in children {
                let (next, child_changed) = parenthesize_stacked_scripts(child);
                changed |= child_changed;
                transformed.push(next);
            }
            if *kind == SyntaxKind::MathAttach {
                for i in 1..transformed.len() {
                    let op = match &transformed[i - 1] {
                        MathTree::Leaf(op @ (SyntaxKind::Underscore | SyntaxKind::Hat), _) => *op,
                        _ => continue,
                    };
                    let has_same_nested_op = matches!(
                        &transformed[i],
                        MathTree::Branch(SyntaxKind::MathAttach, inner_children)
                            if inner_children
                                .iter()
                                .any(|child| matches!(child, MathTree::Leaf(inner_op, _) if *inner_op == op))
                    );
                    if has_same_nested_op {
                        let inner = std::mem::replace(
                            &mut transformed[i],
                            MathTree::Branch(SyntaxKind::Math, Vec::new()),
                        );
                        transformed[i] = MathTree::Branch(
                            SyntaxKind::Math,
                            vec![
                                MathTree::Leaf(SyntaxKind::LeftParen, "(".to_string()),
                                MathTree::Branch(SyntaxKind::Math, vec![inner]),
                                MathTree::Leaf(SyntaxKind::RightParen, ")".to_string()),
                            ],
                        );
                        changed = true;
                    }
                }
            }
            (MathTree::Branch(*kind, transformed), changed)
        }
    }
}

fn is_stacked_scripts_cause(t1_tree: &MathTree, t2_tree: &MathTree) -> bool {
    let (normalized, changed) = parenthesize_stacked_scripts(t1_tree);
    changed && normalized == *t2_tree
}

fn unescape_delimiters(node: &SyntaxNode, out: &mut String, changed: &mut bool) {
    if node.children().len() == 0 {
        let text = node.text().as_str();
        if node.kind() == SyntaxKind::Escape
            && matches!(text, r"\{" | r"\}" | r"\(" | r"\)" | r"\[" | r"\]")
        {
            out.push_str(&text[1..]);
            *changed = true;
        } else {
            out.push_str(text);
        }
        return;
    }
    for child in node.children() {
        unescape_delimiters(child, out, changed);
    }
}

fn is_cross_group_delimiter_cause(latex: &str, p1: &SyntaxNode, t2_tree: &MathTree) -> bool {
    let has_lr_error = latex_errors(latex)
        .iter()
        .any(|msg| msg.contains(r"\left") || msg.contains(r"\right"));
    if !has_lr_error {
        return false;
    }
    let mut unescaped = String::new();
    let mut changed = false;
    unescape_delimiters(p1, &mut unescaped, &mut changed);
    if !changed {
        return false;
    }
    let p1_unescaped = typst_syntax::parse_math(&unescaped);
    !p1_unescaped.erroneous() && math_tree(&p1_unescaped) == *t2_tree
}

fn outcome(latex: &str, round: &RoundTrip) -> Outcome {
    if !latex_errors(&round.l2).is_empty() {
        return Outcome::InvalidLatex;
    }
    let p1 = typst_syntax::parse_math(&round.t1);
    let p2 = typst_syntax::parse_math(&round.t2);
    if p1.erroneous() || p2.erroneous() {
        return Outcome::InvalidTypst;
    }
    if round.t1 == round.t2 {
        return Outcome::Stable;
    }
    let t1_tree = math_tree(&p1);
    let t2_tree = math_tree(&p2);
    if t1_tree == t2_tree {
        Outcome::Unstable(Some(Cause::Spacing))
    } else if is_stacked_scripts_cause(&t1_tree, &t2_tree) {
        Outcome::Unstable(Some(Cause::StackedScripts))
    } else if is_cross_group_delimiter_cause(latex, &p1, &t2_tree) {
        Outcome::Unstable(Some(Cause::CrossGroupDelimiter))
    } else {
        Outcome::Unstable(None)
    }
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

fn expected_outcome(id: &str) -> Outcome {
    for (cause, ids) in KNOWN_UNSTABLE {
        if ids.contains(&id) {
            return Outcome::Unstable(Some(*cause));
        }
    }
    Outcome::Stable
}

#[test]
fn merged_corpus_round_trip_is_a_fixpoint() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("corpus_merged.json")).expect("valid merged corpus JSON");
    let ids: Vec<&str> = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    let mut seen = HashSet::new();
    for (_, known) in KNOWN_UNSTABLE {
        for id in *known {
            assert!(
                ids.contains(id),
                "KNOWN_UNSTABLE lists `{id}`, which is not in the corpus"
            );
            assert!(
                seen.insert(*id),
                "KNOWN_UNSTABLE lists `{id}` more than once"
            );
        }
    }

    let mut mismatches = Vec::new();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let latex = case["latex"].as_str().unwrap();
        let round = round_trip(latex);
        let got = outcome(latex, &round);
        let expected = expected_outcome(id);
        if got != expected {
            let mut extra = String::new();
            match got {
                Outcome::InvalidLatex => {
                    extra.push_str(&format!("\n    L2 errors: {:?}", latex_errors(&round.l2)));
                }
                Outcome::InvalidTypst => {
                    let t1_errs = typst_errors(&typst_syntax::parse_math(&round.t1));
                    let t2_errs = typst_errors(&typst_syntax::parse_math(&round.t2));
                    extra.push_str(&format!(
                        "\n    T1 errors: {t1_errs:?}\n    T2 errors: {t2_errs:?}"
                    ));
                }
                _ => {}
            }
            mismatches.push(format!(
                "  {id}: expected {expected:?}, got {got:?}{extra}\n    L1: {latex}\n    T1: {}\n    L2: {}\n    T2: {}",
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

/// Ignores `SyntaxKind::Space` trivia via `math_tree`, so that harmless
/// spacing differences do not hide structural changes while parse-changing
/// spacing (like `sqrt(x)` vs `sqrt (x)`) and Typst syntax errors still fail.
/// Runs `ROUNDTRIP_CASES` cases, else 1,000. A failure prints its input; its
/// fix adds a test to integration_tests.rs.
#[test]
fn generated_math_round_trip_is_a_fixpoint_up_to_spacing() {
    let cases: u64 = match std::env::var("ROUNDTRIP_CASES") {
        Ok(val) => val
            .parse()
            .unwrap_or_else(|err| panic!("invalid ROUNDTRIP_CASES={val:?}: {err}")),
        Err(std::env::VarError::NotPresent) => 1000,
        Err(err) => panic!("failed to read ROUNDTRIP_CASES: {err}"),
    };
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
        let p1 = typst_syntax::parse_math(&round.t1);
        let p2 = typst_syntax::parse_math(&round.t2);
        assert_eq!(
            typst_errors(&p1),
            Vec::<String>::new(),
            "T1 Typst syntax error\nL1: {latex}\nT1: {}\nL2: {}\nT2: {}",
            round.t1,
            round.l2,
            round.t2
        );
        assert_eq!(
            typst_errors(&p2),
            Vec::<String>::new(),
            "T2 Typst syntax error\nL1: {latex}\nT1: {}\nL2: {}\nT2: {}",
            round.t1,
            round.l2,
            round.t2
        );
        assert_eq!(
            math_tree(&p1),
            math_tree(&p2),
            "L1: {latex}\nT1: {}\nL2: {}\nT2: {}",
            round.t1,
            round.l2,
            round.t2
        );
    }
}

#[test]
fn math_tree_distinguishes_func_call_from_spaced_delimiters() {
    assert_ne!(
        math_tree(&typst_syntax::parse_math("sqrt(x)")),
        math_tree(&typst_syntax::parse_math("sqrt (x)"))
    );
    assert_ne!(
        math_tree(&typst_syntax::parse_math("frac(a, b)")),
        math_tree(&typst_syntax::parse_math("frac (a, b)"))
    );
    assert_eq!(
        math_tree(&typst_syntax::parse_math("F (x)= x")),
        math_tree(&typst_syntax::parse_math("F (x) = x"))
    );
}

#[test]
fn cause_classifiers_verify_specific_instability_only() {
    let stacked_latex = r"{x^{a}}^{b}";
    let stacked_round = round_trip(stacked_latex);
    assert_eq!(
        outcome(stacked_latex, &stacked_round),
        Outcome::Unstable(Some(Cause::StackedScripts))
    );
    let stacked_unrelated = RoundTrip {
        t2: "y^((a)^(b))".to_string(),
        ..stacked_round
    };
    assert_eq!(
        outcome(stacked_latex, &stacked_unrelated),
        Outcome::Unstable(None)
    );

    let cross_latex = r"\left\{ x { \right\} }";
    let cross_round = round_trip(cross_latex);
    assert_eq!(
        outcome(cross_latex, &cross_round),
        Outcome::Unstable(Some(Cause::CrossGroupDelimiter))
    );
    let cross_unrelated = RoundTrip {
        t2: "{y }".to_string(),
        ..cross_round
    };
    assert_eq!(
        outcome(cross_latex, &cross_unrelated),
        Outcome::Unstable(None)
    );

    let invalid_typst = RoundTrip {
        t1: "frac(a,".to_string(),
        l2: "x".to_string(),
        t2: "x".to_string(),
    };
    assert_eq!(outcome("x", &invalid_typst), Outcome::InvalidTypst);
}
