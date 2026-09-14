//! Golden regression tests curated from the OCR math corpus (the "错题本").
//!
//! Each case pins tylax's OWN current, verified-correct output — not parity with
//! any other converter. tylax normalizes differently on purpose (`x^(2)` not
//! `x^2`, `F_(1)` not `F_1`, `\left(..\right)` kept as `lr(..)`, a space after a
//! unary minus, `frac(a, b)` for stacked fractions). These 21 cases were hand
//! reviewed: every expected string both compiles under the real Typst compiler
//! and is a faithful rendering of the LaTeX input.
//!
//! `//` is also deliberate, not a gap: in LaTeX it IS two slashes, and
//! `slash slash` is the faithful conversion. Reading it as `∥` is a geometry
//! OCR convention, not a property of the source, so it is out of scope for the
//! converter and its cases are simply not goldens here.
//!
//! Cases exercising still-unfixed normalizations were deliberately excluded so
//! this file never cements a known defect: half-open `\left|..\right.` and
//! OCR-garbled inputs. Those remain tracked separately.

use tylax::latex_to_typst;

/// `(id, latex, expected typst)` — expected is tylax's verified output.
const GOLDEN: &[(&str, &str, &str)] = &[
    // Multi-digit numbers and decimals stay single literals: splitting the run
    // rendered `120` as three separate numerals.
    (
        "num-degrees",
        r"\angle APB = 120^{\circ}",
        "angle A P B = 120^(circle.small)",
    ),
    (
        "num-fraction",
        r"\left|MN\right| = x_{1} + x_{2} + p = 3 + \frac{1}{3} + 2 = \frac{16}{3}",
        "abs(M N) = x_(1) + x_(2) + p = 3 + 1/3 + 2 = 16/3",
    ),
    // #35 vectors / directional arrows + nested subscripts.
    (
        "f278",
        r"\overrightarrow { F _ { 1 } A } \perp \overrightarrow { F _ { 1 } B } , \overrightarrow { F _ { 2 } A } = - \frac { 2 } { 3 } \overrightarrow { F _ { 2 } B }",
        "arrow(F_(1) A) perp arrow(F_(1) B), arrow(F_(2) A) = - 2/3 arrow(F_(2) B)",
    ),
    (
        "f299",
        r"\overrightarrow { F _ { 2 } A } = - \frac { 2 } { 3 } \overrightarrow { F _ { 2 } B }",
        "arrow(F_(2) A) = - 2/3 arrow(F_(2) B)",
    ),
    (
        "f302",
        r"\overrightarrow { F _ { 1 } A } \perp \overrightarrow { F _ { 1 } B }",
        "arrow(F_(1) A) perp arrow(F_(1) B)",
    ),
    (
        "f303",
        r"{ \overrightarrow { F _ { 1 } A } } \cdot { \overrightarrow { F _ { 1 } B } } = \left( { \frac { 8 } { 3 } } c , - { \frac { 2 } { 3 } } t \right) \left( c , t \right) = { \frac { 8 } { 3 } } c ^ { 2 } - { \frac { 2 } { 3 } } t ^ { 2 } = 0",
        "arrow(F_(1) A) dot arrow(F_(1) B) = (8/3 c, - 2/3 t) (c, t) = 8/3 c^(2) - 2/3 t^(2) = 0",
    ),
    (
        "f347",
        r"\therefore \overrightarrow { B _ { 2 } C _ { 2 } } = ( 0 , - 2 , 1 ) , \overrightarrow { A _ { 2 } D _ { 2 } } = ( 0 , - 2 , 1 )",
        "therefore arrow(B_(2) C_(2)) = (0, - 2, 1), arrow(A_(2) D_(2)) = (0, - 2, 1)",
    ),
    (
        "f352",
        r"\overrightarrow { A _ { 2 } C _ { 2 } } = ( - 2 , - 2 , 2 ) , \overrightarrow { P C _ { 2 } } = ( 0 , - 2 , 3 - \lambda ) , \overrightarrow { D _ { 2 } C _ { 2 } } = ( - 2 , 0 , 1 )",
        "arrow(A_(2) C_(2)) = (- 2, - 2, 2), arrow(P C_(2)) = (0, - 2, 3 - lambda), arrow(D_(2) C_(2)) = (- 2, 0, 1)",
    ),
    (
        "f364",
        r"\therefore \overrightarrow {m} = (1, 1, 2)",
        "therefore arrow(m) = (1, 1, 2)",
    ),
    // Root cause ①: `\left\{ ... array ... \right.` becomes `cases(...)` with
    // `&` column splits, not a leftover `lr(mat(...))`.
    (
        "f355",
        r"\left\{ \begin{array} { l l } { { \vec { n } \cdot \overrightarrow { A _ { 2 } C _ { 2 } } = - 2 x - 2 y + 2 z = 0 } } \\ { { \vec { n } \cdot \overrightarrow { P C _ { 2 } } = - 2 y + ( 3 - \lambda ) z = 0 } } \end{array} \right. ,",
        "cases(arrow(n) dot arrow(A_(2) C_(2)) = - 2 x - 2 y + 2 z = 0, arrow(n) dot arrow(P C_(2)) = - 2 y + (3 - lambda) z = 0),",
    ),
    (
        "f361",
        r"\left\{ \begin{array} { l l } { { \vec { m } \cdot \overrightarrow { A _ { 2 } C _ { 2 } } = - 2 a - 2 b + 2 c = 0 } } \\ { { \vec { m } \cdot \overrightarrow { D _ { 2 } C _ { 2 } } = - 2 a + c = 0 } } \end{array} \right.",
        "cases(arrow(m) dot arrow(A_(2) C_(2)) = - 2 a - 2 b + 2 c = 0, arrow(m) dot arrow(D_(2) C_(2)) = - 2 a + c = 0)",
    ),
    // #42: `\displaystyle` inside a cases row is preserved as `display(...)`.
    (
        "f566",
        r"\left\{ \begin{array} { l } { { \displaystyle y = x ^ { 2 } + { \frac { 1 } { 4 } } } } \\ { { \displaystyle y = k ( x - a ) + a ^ { 2 } + { \frac { 1 } { 4 } } } } \end{array} \right.",
        "cases(display(y = x^(2) + 1/4), display(y = k (x - a) + a^(2) + 1/4))",
    ),
    // `{\cal X}` grouping and abs of a cal product.
    (
        "f503",
        r"P \left( { \cal A } _ { i } \right) = p _ { i }",
        "P (cal(A)_(i)) = p_(i)",
    ),
    (
        "f531",
        r"\left| { \cal A } { \cal B } \right| + \left| { \cal A } { \cal D } \right| \geq \sqrt { \frac { \left( 1 + k ^ { 2 } \right) ^ { 3 } } { k ^ { 2 } } }",
        "abs(cal(A) cal(B)) + abs(cal(A) cal(D)) >= sqrt(frac((1 + k^(2))^(3), k^(2)))",
    ),
    // Nested subscript `k_{_{AB}}` flattens to `k_(A B)`.
    (
        "f526",
        r"k _ { _ { A B } } = a + b = m < 0 , k _ { _ { B C } } = b + c = n > 0",
        "k_(A B) = a + b = m < 0, k_(B C) = b + c = n > 0",
    ),
    (
        "f538",
        r"k _ { _ { A B } } \cdot k _ { _ { B C } } = - 1 , a + b < b + c",
        "k_(A B) dot k_(B C) = - 1, a + b < b + c",
    ),
    (
        "f539",
        r"k _ { _ { A B } } = { \frac { b ^ { 2 } + { \frac { 1 } { 4 } } - \left( a ^ { 2 } + { \frac { 1 } { 4 } } \right) } { b - a } } = a + b = m < 0  \quad",
        "k_(A B) = frac(b^(2) + 1/4 - (a^(2) + 1/4), b - a) = a + b = m < 0 quad",
    ),
    (
        "f540",
        r"k _ { _ { B C } } = b + c = n > 0",
        "k_(B C) = b + c = n > 0",
    ),
    (
        "f543",
        r"k _ { _ { B C } } - k _ { _ { A B } } = c - a = n - m = n + \frac { 1 } { n }",
        "k_(B C) - k_(A B) = c - a = n - m = n + 1/n",
    ),
    (
        "f597",
        r"k _ { \scriptscriptstyle  { A B } ^ { ' } } = t _ { 1 } + t _ { 0 } , k _ { \scriptscriptstyle  { B C } ^ { ' } } = t _ { 2 } + t _ { 0 }",
        "k_(A B^(')) = t_(1) + t_(0), k_(B C^(')) = t_(2) + t_(0)",
    ),
    // `\scriptscriptstyle` has no Typst equivalent: content kept, marker dropped.
    (
        "f457",
        r"a _ { \scriptscriptstyle 1 } = d",
        "a_(1) = d",
    ),
];

#[test]
fn corpus_golden_outputs_are_stable() {
    let mut mismatches = Vec::new();
    for (id, latex, expected) in GOLDEN {
        let got = latex_to_typst(latex);
        let got = got.trim();
        if got != *expected {
            mismatches.push(format!(
                "  {id}:\n    expected: {expected}\n    got:      {got}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "corpus golden output drifted:\n{}",
        mismatches.join("\n")
    );
}
