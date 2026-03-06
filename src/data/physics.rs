//! Physics package command support
//!
//! Provides LaTeX `physics` package command-to-Typst mappings.
//! The physics package (by Sergio C. de la Barrera) defines ~134 commands
//! for vector notation, derivatives, Dirac bra-ket notation, operators,
//! automatic bracing, matrix macros, and quick quad text.
//!
//! ## Design
//!
//! Commands are split into two groups:
//! - **Zero-argument symbols** → mapped in `EXTENDED_SYMBOLS` (looked up automatically)
//! - **Parametric commands** → handled via match arms in `markup.rs`

/// Quick-quad text words used by \qif, \qthen, etc.
/// Maps physics command name → the English word it produces.
pub const QQ_COMMANDS: &[(&str, &str)] = &[
    ("qif", "if"),
    ("qthen", "then"),
    ("qelse", "else"),
    ("qotherwise", "otherwise"),
    ("qunless", "unless"),
    ("qgiven", "given"),
    ("qusing", "using"),
    ("qassume", "assume"),
    ("qsince", "since"),
    ("qlet", "let"),
    ("qfor", "for"),
    ("qall", "for all"),
    ("qeven", "even"),
    ("qodd", "odd"),
    ("qinteger", "integer"),
    ("qand", "and"),
    ("qor", "or"),
    ("qas", "as"),
    ("qin", "in"),
];

/// Check if a command name is a physics quick-quad command
pub fn get_qq_text(name: &str) -> Option<&'static str> {
    QQ_COMMANDS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}

/// Check if a command is a physics package command that needs special handling
/// in markup.rs (i.e., has arguments that can't be expressed as a simple symbol).
pub fn is_physics_command(name: &str) -> bool {
    matches!(
        name,
        // Automatic bracing
        "pqty" | "bqty" | "Bqty" | "vqty"
        | "abs" | "norm" | "eval" | "order"
        | "comm" | "acomm" | "acommutator" | "pb" | "poissonbracket"
        // Vector notation (with arguments)
        | "vb" | "va" | "vu"
        | "vectorbold" | "vectorarrow" | "vectorunit"
        // Derivatives
        | "dd" | "differential"
        | "dv" | "derivative"
        | "pdv" | "pderivative" | "partialderivative"
        | "fdv" | "fderivative" | "functionalderivative"
        // Dirac notation
        | "bra" | "ket"
        | "braket" | "innerproduct" | "ip"
        | "dyad" | "outerproduct" | "ketbra" | "op"
        | "expval" | "expectationvalue" | "ev"
        | "mel" | "matrixelement" | "matrixel"
        | "vev"
        // Quick quad text
        | "qq" | "qqtext" | "qc" | "qcomma" | "qcc"
        | "qif" | "qthen" | "qelse" | "qotherwise" | "qunless"
        | "qgiven" | "qusing" | "qassume" | "qsince" | "qlet"
        | "qfor" | "qall" | "qeven" | "qodd" | "qinteger"
        | "qand" | "qor" | "qas" | "qin"
        // Matrix macros
        | "mqty" | "matrixquantity"
        | "pmqty" | "bmqty" | "vmqty" | "Pmqty"
        | "smqty" | "smallmatrixquantity"
        | "spmqty" | "sPmqty" | "sbmqty" | "svmqty"
        | "mdet" | "matrixdeterminant"
        | "smdet" | "smallmatrixdeterminant"
        // Operators with bracing
        | "Res" | "Residue"
        | "pv" | "principalvalue" | "PV"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qq_text_lookup() {
        assert_eq!(get_qq_text("qif"), Some("if"));
        assert_eq!(get_qq_text("qand"), Some("and"));
        assert_eq!(get_qq_text("qall"), Some("for all"));
        assert_eq!(get_qq_text("nonexistent"), None);
    }

    #[test]
    fn test_is_physics_command() {
        assert!(is_physics_command("dv"));
        assert!(is_physics_command("braket"));
        assert!(is_physics_command("abs"));
        assert!(is_physics_command("mqty"));
        assert!(!is_physics_command("frac"));
        assert!(!is_physics_command("section"));
    }
}
