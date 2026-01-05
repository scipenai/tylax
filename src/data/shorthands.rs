//! Typst shorthand mappings
//!
//! This module provides shorthand symbol mappings for Typst output.
//! When enabled, symbols like `arrow.r` will be output as `->` for better readability.

use phf::phf_map;

/// Typst symbol to shorthand mapping
/// Maps verbose Typst symbols to their shorthand equivalents
pub static TYPST_SHORTHANDS: phf::Map<&'static str, &'static str> = phf_map! {
    // Long arrows
    "arrow.l.r.double.long" => "<==>",
    "arrow.l.r.long" => "<-->",
    "arrow.r.double.long" => "==>",
    "arrow.r.long" => "-->",
    "arrow.r.long.squiggly" => "~~>",
    "arrow.l.double.long" => "<==",
    "arrow.l.long" => "<--",
    "arrow.l.long.squiggly" => "<~~",

    // Medium arrows
    "arrow.r.bar" => "|->",
    "arrow.r.double.bar" => "|=>",
    "arrow.r.tail" => ">->",
    "arrow.r.twohead" => "->>",
    "arrow.l.tail" => "<-<",
    "arrow.l.twohead" => "<<-",
    "arrow.l.r.double" => "<=>",
    "arrow.l.r" => "<->",

    // Short arrows
    "arrow.r" => "->",
    "arrow.r.double" => "=>",
    "arrow.r.squiggly" => "~>",
    "arrow.l" => "<-",
    "arrow.l.squiggly" => "<~",

    // Comparison operators
    "gt.double" => ">>",
    "gt.eq" => ">=",
    "gt.triple" => ">>>",
    "lt.double" => "<<",
    "lt.eq" => "<=",
    "lt.triple" => "<<<",
    "eq.not" => "!=",

    // Other symbols
    "bar.v.double" => "||",
    "bracket.l.stroked" => "[|",
    "bracket.r.stroked" => "|]",
    "colon.eq" => ":=",
    "eq.colon" => "=:",
    "colon.double.eq" => "::=",
    "dots.h" => "...",
    "ast.op" => "*",
    "minus" => "-",
    "tilde.op" => "~",
};

/// Reverse mapping: shorthand to full symbol
/// Used for Typst → LaTeX conversion when we need to recognize shorthands
pub static SHORTHAND_TO_TYPST: phf::Map<&'static str, &'static str> = phf_map! {
    "<==>" => "arrow.l.r.double.long",
    "<-->" => "arrow.l.r.long",
    "==>" => "arrow.r.double.long",
    "-->" => "arrow.r.long",
    "~~>" => "arrow.r.long.squiggly",
    "<==" => "arrow.l.double.long",
    "<--" => "arrow.l.long",
    "<~~" => "arrow.l.long.squiggly",
    "|->" => "arrow.r.bar",
    "|=>" => "arrow.r.double.bar",
    ">->" => "arrow.r.tail",
    "->>" => "arrow.r.twohead",
    "<-<" => "arrow.l.tail",
    "<<-" => "arrow.l.twohead",
    "<=>" => "arrow.l.r.double",
    "<->" => "arrow.l.r",
    "->" => "arrow.r",
    "=>" => "arrow.r.double",
    "~>" => "arrow.r.squiggly",
    "<-" => "arrow.l",
    "<~" => "arrow.l.squiggly",
    ">>" => "gt.double",
    ">=" => "gt.eq",
    ">>>" => "gt.triple",
    "<<" => "lt.double",
    "<=" => "lt.eq",
    "<<<" => "lt.triple",
    "!=" => "eq.not",
    "||" => "bar.v.double",
    "[|" => "bracket.l.stroked",
    "|]" => "bracket.r.stroked",
    ":=" => "colon.eq",
    "=:" => "eq.colon",
    "::=" => "colon.double.eq",
    "..." => "dots.h",
};

/// Check if a Typst symbol has a shorthand equivalent
#[inline]
pub fn has_shorthand(symbol: &str) -> bool {
    TYPST_SHORTHANDS.contains_key(symbol)
}

/// Get the shorthand for a Typst symbol, if available
#[inline]
pub fn get_shorthand(symbol: &str) -> Option<&'static str> {
    TYPST_SHORTHANDS.get(symbol).copied()
}

/// Apply shorthand conversion to a Typst symbol if enabled
#[inline]
pub fn apply_shorthand(symbol: &str, prefer_shorthands: bool) -> &str {
    if prefer_shorthands {
        TYPST_SHORTHANDS.get(symbol).copied().unwrap_or(symbol)
    } else {
        symbol
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arrow_shorthands() {
        assert_eq!(get_shorthand("arrow.r"), Some("->"));
        assert_eq!(get_shorthand("arrow.l.r.double"), Some("<=>"));
        assert_eq!(get_shorthand("arrow.r.long"), Some("-->"));
    }

    #[test]
    fn test_comparison_shorthands() {
        assert_eq!(get_shorthand("gt.eq"), Some(">="));
        assert_eq!(get_shorthand("lt.eq"), Some("<="));
        assert_eq!(get_shorthand("eq.not"), Some("!="));
    }

    #[test]
    fn test_apply_shorthand() {
        assert_eq!(apply_shorthand("arrow.r", true), "->");
        assert_eq!(apply_shorthand("arrow.r", false), "arrow.r");
        assert_eq!(apply_shorthand("alpha", true), "alpha"); // No shorthand
    }

    #[test]
    fn test_reverse_mapping() {
        assert_eq!(SHORTHAND_TO_TYPST.get("->"), Some(&"arrow.r"));
        assert_eq!(SHORTHAND_TO_TYPST.get("<=>"), Some(&"arrow.l.r.double"));
    }
}
