from __future__ import annotations

class Span:
    start: int
    end: int
    def __repr__(self) -> str: ...

class ConversionWarning:
    kind: str
    message: str
    span: Span | None
    def __repr__(self) -> str: ...

class ConversionResult:
    output: str
    warnings: list[ConversionWarning]
    def __repr__(self) -> str: ...

class L2TOptions:
    prefer_shorthands: bool
    frac_to_slash: bool
    infty_to_oo: bool
    keep_spaces: bool
    non_strict: bool
    optimize: bool
    expand_macros: bool
    def __init__(
        self,
        *,
        prefer_shorthands: bool = True,
        frac_to_slash: bool = True,
        infty_to_oo: bool = False,
        keep_spaces: bool = False,
        non_strict: bool = True,
        optimize: bool = True,
        expand_macros: bool = True,
    ) -> None: ...

class T2LOptions:
    document_class: str
    title: str | None
    author: str | None
    block_math_mode: bool
    def __init__(
        self,
        *,
        document_class: str = "article",
        title: str | None = None,
        author: str | None = None,
        block_math_mode: bool = True,
    ) -> None: ...

def latex_to_typst(
    text: str, *, document: bool = False, options: L2TOptions | None = None
) -> str: ...
def typst_to_latex(
    text: str, *, document: bool = False, options: T2LOptions | None = None
) -> str: ...
def latex_to_typst_diagnostics(
    text: str, *, document: bool = False, options: L2TOptions | None = None
) -> ConversionResult: ...
def typst_to_latex_diagnostics(
    text: str, *, document: bool = False, options: T2LOptions | None = None
) -> ConversionResult: ...
def detect_format(text: str) -> str: ...
def convert_auto(text: str, *, document: bool = False) -> tuple[str, str]: ...
def tikz_to_cetz(text: str) -> str: ...
def cetz_to_tikz(text: str) -> str: ...
def is_cetz_code(text: str) -> bool: ...

__version__: str
