# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.4] - 2026-03-12

This release unifies citation, reference, and label semantics across both LaTeX→Typst and Typst→LaTeX conversion. Citations now stay explicit as `#cite(...)` on the Typst side, bare `@key` consistently remains reference-first on the T2L path, and the shared refs layer now drives more of the conversion logic in both directions.

MiniEval also preserves `cite`, `ref`, `label`, and `bibliography` as semantic content nodes instead of reconstructing them from generic function calls. That makes dynamic cases like `#let k = <knuth>; #cite(k)` and loop-expanded citations round-trip cleanly and fixes a number of malformed citation/reference outputs.

## [0.3.3] - 2026-03-08

### Fixed
- **LaTeX-to-Typst Matrix Delimiters**: Preserve matrix semantics for `\left...\right` wrapped `array` and matrix environments. Delimiter-less environments now inherit the outer delimiter as `mat(delim: ...)`, while environments with intrinsic delimiters keep their nested structure instead of collapsing to `abs()` or `norm()`.

## [0.3.2] - 2026-03-08

### Fixed
- **Typst-to-LaTeX Math**: Fixed escaped punctuation, `accent()` mapping, grouped scripts, `mat(delim: ...)`, `attach()` script output, and stray math whitespace.

### Changed
- **T2L Math Pipeline**: Moved Typst-to-LaTeX math conversion onto an `AST -> MathIr -> emit` pipeline and removed the legacy runtime path.

## [0.3.1] - 2026-03-07

### Fixed
- **Typst Named Arguments**: Refactored `typst2latex` to parse named arguments through a shared AST-aware `FuncArgs` layer instead of per-call string splitting.
  - Fixed `lr(..., size: ...)` so `size: #200%` no longer leaks into math output and now maps cleanly to fixed-size LaTeX delimiters.
  - Fixed tuple-valued and function-valued named arguments such as `grid(columns: (auto, auto, auto))` and `fill: rgb(255, 0, 0)` so values are preserved intact.
  - Fixed `rotate(angle: 90deg)` and `bibliography(..., style: plain)` to preserve named argument values instead of silently degrading.

### Changed
- **Color Conversion**: Unified Typst color handling for text, rectangles, and table cells through a shared normalization and LaTeX color-spec pipeline.
  - Added support for `rgb(...)`, `cmyk(...)`, and `luma(...)` color values in Typst-to-LaTeX conversion.
  - Kept named colors and `lighten(...)` / `darken(...)` method chains on the same conversion path to reduce duplicated logic.

## [0.3.0] - 2026-03-07

### Added
- **Physics Package Support**: Added initial support for common `physics` package commands in LaTeX to Typst conversion.
  - Automatic bracing helpers such as `\pqty`, `\bqty`, `\Bqty`, `\vqty`, `\abs`, and `\norm`
  - Derivative helpers such as `\dd`, `\dv`, `\pdv`, and `\fdv`
  - Dirac notation such as `\bra`, `\ket`, `\braket`, `\dyad`, `\expval`, `\mel`, and `\vev`
  - Matrix helpers such as `\mqty`, `\pmqty`, `\bmqty`, `\vmqty`, `\dmat`, and `\zmat`
  - Quick-quad text helpers such as `\qq`, `\qif`, `\qthen`, and related variants

### Fixed
- **Mixed Partial Derivatives**: Preserved argument order for mixed partial derivative forms such as `\pdv{f}{x}{y}`.

## [0.2.2] - 2026-02-23

### Fixed
- **Math Spacing**: `\,`, `\;`, `\quad`, `\qquad` now convert to Typst math spacing keywords (`thin`, `thick`, `quad`, `wide`) instead of plain spaces when in math mode. Reverse mapping (`wide` → `\qquad`) also added for T2L.
- **Array Environment**: `\begin{array}` now converts to `mat(delim: #none, ...)` instead of being incorrectly treated as a `table()`.

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

[Unreleased]: https://github.com/scipenai/tylax/compare/v0.3.4...HEAD
[0.3.4]: https://github.com/scipenai/tylax/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/scipenai/tylax/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/scipenai/tylax/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/scipenai/tylax/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/scipenai/tylax/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/scipenai/tylax/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/scipenai/tylax/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/scipenai/tylax/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/scipenai/tylax/releases/tag/v0.1.0
