//! Data layer - Static mappings and constants
//!
//! This module contains all static data used for LaTeX ↔ Typst conversion:
//! - Symbol mappings
//! - Command specifications
//! - Language/theorem type constants
//! - Shorthand symbol mappings

pub mod colors;
pub mod constants;
pub mod extended_symbols;
pub mod maps;
pub mod shorthands;
pub mod siunitx;
pub mod symbols;
pub mod typst_compat;

// Re-export commonly used items
pub use colors::{convert_color_commands, parse_color_expression, NAMED_COLORS};
pub use constants::{
    AcronymDef, CodeBlockOptions, GlossaryDef, TheoremInfo, TheoremStyle, LANGUAGE_MAP,
    THEOREM_TYPES,
};
pub use extended_symbols::{lookup_extended_symbol, EXTENDED_SYMBOLS};
pub use maps::{TEX_COMMAND_SPEC, TYPST_TO_TEX};
pub use shorthands::{
    apply_shorthand, get_shorthand, has_shorthand, SHORTHAND_TO_TYPST, TYPST_SHORTHANDS,
};
pub use symbols::{
    ACCENT_COMMANDS, CHAR_COMMANDS, GREEK_LETTERS, LETTER_COMMANDS, MISC_SYMBOLS,
    TEXT_FORMAT_COMMANDS,
};
pub use typst_compat::{
    get_heading_command, is_math_func_in_markup, MarkupHandler, MathHandler, TYPST_MARKUP_HANDLERS,
    TYPST_MATH_HANDLERS,
};
