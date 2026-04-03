import tylax


def test_version():
    assert isinstance(tylax.__version__, str)
    assert "." in tylax.__version__


# ---- Math conversion ----


def test_latex_to_typst_math():
    result = tylax.latex_to_typst(r"\frac{1}{2}")
    assert "frac" in result or "/" in result


def test_typst_to_latex_math():
    result = tylax.typst_to_latex("frac(1, 2)")
    assert r"\frac" in result


def test_latex_to_typst_with_options():
    opts = tylax.L2TOptions(frac_to_slash=False)
    result = tylax.latex_to_typst(r"\frac{a}{b}", options=opts)
    assert "frac(" in result


def test_typst_to_latex_with_options():
    opts = tylax.T2LOptions(document_class="report")
    result = tylax.typst_to_latex("= Hello\nWorld", document=True, options=opts)
    assert "Hello" in result


# ---- Document conversion ----


def test_latex_to_typst_document():
    src = r"\documentclass{article}\begin{document}\section{Hello}\end{document}"
    result = tylax.latex_to_typst(src, document=True)
    assert "Hello" in result


def test_typst_to_latex_document():
    result = tylax.typst_to_latex("= Hello\nWorld", document=True)
    assert "section" in result or "Hello" in result


# ---- Auto detection ----


def test_detect_format_latex():
    assert tylax.detect_format(r"\documentclass{article}") == "latex"


def test_detect_format_typst():
    assert tylax.detect_format('#set page(paper: "a4")') == "typst"


def test_convert_auto():
    output, fmt = tylax.convert_auto(r"\frac{1}{2}")
    assert fmt == "typst"
    assert len(output) > 0


def test_convert_auto_document():
    output, fmt = tylax.convert_auto("= Hello\nWorld", document=True)
    assert fmt in ("latex", "typst")
    assert len(output) > 0


# ---- Diagnostics ----


def test_l2t_diagnostics():
    result = tylax.latex_to_typst_diagnostics(r"\frac{1}{2}")
    assert isinstance(result, tylax.ConversionResult)
    assert isinstance(result.output, str)
    assert isinstance(result.warnings, list)


def test_t2l_diagnostics():
    result = tylax.typst_to_latex_diagnostics("frac(1, 2)")
    assert isinstance(result, tylax.ConversionResult)
    assert isinstance(result.output, str)
    assert isinstance(result.warnings, list)


# ---- TikZ / CeTZ ----


def test_tikz_to_cetz():
    tikz = r"\draw (0,0) -- (1,1);"
    result = tylax.tikz_to_cetz(tikz)
    assert len(result) > 0


def test_cetz_to_tikz():
    cetz = 'line((0, 0), (1, 1))'
    result = tylax.cetz_to_tikz(cetz)
    assert len(result) > 0


def test_is_cetz_code():
    assert tylax.is_cetz_code('import "@preview/cetz:0.1.0"')
    assert tylax.is_cetz_code('canvas({ line((0,0), (1,1)) })')
    assert not tylax.is_cetz_code(r'\documentclass{article}')


# ---- Diagnostics ----


def test_diagnostics_warning_structure():
    result = tylax.typst_to_latex_diagnostics("frac(1, 2)")
    for w in result.warnings:
        assert isinstance(w, tylax.ConversionWarning)
        assert isinstance(w.kind, str)
        assert isinstance(w.message, str)
        assert w.span is None or isinstance(w.span, tylax.Span)
