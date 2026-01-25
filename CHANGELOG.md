# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-01-25

### Fixed
- **Infinite Recursion**: Fixed a nasty browser freeze when macros call each other infinitely (like `\foo` ↔ `\bar`).
  - Added a hard step limit to kill the loop before it kills the browser.
  - If it hits the limit, we now just dump the text (e.g., `x`) instead of choking.
  - Fixed the real culprit: raw `\newcommand` definitions were leaking into the parser when things went wrong.

## [0.2.0] - 2026-01-16

This release is a major overhaul of the core conversion logic, introducing proper macro expansion engines for both directions.

### Core Engines
- **L2T Engine**: Replaced the old regex hacks with a proper token-based macro expander (`latex2typst/engine`).
  - Now supports `\def`, `\newcommand`, `\let`, `\newif`, and delimited parameters (e.g., `#1...#2`).
  - Implemented correct scoping (`\begingroup`) and recursion limits.
- **T2L Engine**: Introduced "MiniEval", a lightweight interpreter (`typst2latex/engine`).
  - Handles `#let` bindings, loops (`#for`/`#while`), conditionals, and custom function calls before conversion.
  - Added VFS abstraction to support `#import` resolution in both CLI and WASM.

### Fixed
- **Math Mode**: Implemented dynamic `$` tracking for `\ifmmode`. Macros now properly detect if they are inside inline/display math during expansion.
- **Escaping**: Patched missing escapes for special chars (`_`, `$`, `#`) in text mode. The logic is now shared with `convert_command_sym` to prevent regressions.
- **Typst Math**: Added missing mappings for keywords like `plus`, `minus`, `eq`. They now convert to operators (`+`, `-`, `=`) instead of raw identifiers.
- **Operators**: Fixed `\DeclareMathOperator*` ignoring the star; it now correctly adds `limits()` to the Typst operator.
- **Diagnostics**: `\let` now warns when the target macro doesn't exist (previously failed silently).

### Added
- **WASM**: New `expand_macros` option to toggle MiniEval in document mode.

### Removed
- Dropped the legacy `features/macros.rs` module in favor of the new engines.

## [0.1.0] - 2026-01-05

### Added
- Bidirectional LaTeX ↔ Typst conversion
- Full document conversion support (headings, lists, tables, figures)
- 700+ symbol mappings (376 LaTeX→Typst + 341 Typst→LaTeX)
- TikZ ↔ CeTZ graphics conversion
- Advanced table engine with multirow/multicolumn support
- Macro expansion (`\def`, `\newcommand`, `#let`)
- siunitx and mhchem support
- WebAssembly support
- CLI tool (`t2l`)
- Structured error handling with warnings

[Unreleased]: https://github.com/scipenai/tylax/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/scipenai/tylax/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/scipenai/tylax/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/scipenai/tylax/releases/tag/v0.1.0
