# tylax

High-performance bidirectional LaTeX <-> Typst converter. Python bindings for [tylax](https://github.com/scipenai/tylax).

## Installation

```bash
pip install tylax
```

Requires Python 3.10+. Pre-built wheels available for Linux, macOS, and Windows.

## Usage

```python
import tylax

# Math conversion
typst = tylax.latex_to_typst(r"\frac{1}{2}")
latex = tylax.typst_to_latex("frac(1, 2)")

# Document conversion
typst_doc = tylax.latex_to_typst(r"\section{Hello}", document=True)
latex_doc = tylax.typst_to_latex("= Hello\nWorld", document=True)

# Auto-detect format
output, fmt = tylax.convert_auto(r"\frac{1}{2}")

# Format detection
tylax.detect_format(r"\documentclass{article}")  # "latex"

# With options
opts = tylax.L2TOptions(frac_to_slash=False)
tylax.latex_to_typst(r"\frac{a}{b}", options=opts)

# Diagnostics
result = tylax.typst_to_latex_diagnostics("frac(1, 2)")
print(result.output)
for w in result.warnings:
    print(f"[{w.kind}] {w.message}")
```

## API

- `latex_to_typst(text, *, document=False, options=None) -> str`
- `typst_to_latex(text, *, document=False, options=None) -> str`
- `latex_to_typst_diagnostics(...) -> ConversionResult`
- `typst_to_latex_diagnostics(...) -> ConversionResult`
- `detect_format(text) -> str`
- `convert_auto(text, *, document=False) -> tuple[str, str]`

## License

Apache-2.0
