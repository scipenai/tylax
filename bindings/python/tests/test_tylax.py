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


# ---- Preamble / wrapper customization ----


SAMPLE_LATEX_DOC = r"""\documentclass{article}
\title{Sample}
\author{Alice}
\begin{document}
\section{Intro}
Hello.
\end{document}"""


def test_l2t_preamble_default_emits_set_rules():
    out = tylax.latex_to_typst(SAMPLE_LATEX_DOC, document=True)
    assert "#set page(" in out
    assert "#set heading(" in out
    assert "#set math.equation(" in out


def test_l2t_preamble_omit_drops_set_rules_but_keeps_metadata_and_title():
    opts = tylax.L2TOptions(preamble_omit=True)
    out = tylax.latex_to_typst(SAMPLE_LATEX_DOC, document=True, options=opts)
    assert "#set page(" not in out
    assert "#set heading(" not in out
    assert "#set math.equation(" not in out
    # metadata block must remain
    assert "#set document(" in out and "Sample" in out and "Alice" in out
    # title block must remain
    assert "#align(center)[" in out


def test_l2t_preamble_custom_replaces_default():
    opts = tylax.L2TOptions(preamble='#set text(font: "New Roman")')
    out = tylax.latex_to_typst(SAMPLE_LATEX_DOC, document=True, options=opts)
    assert "#set text(font:" in out
    assert "#set page(" not in out


def test_t2l_wrapper_default_emits_full_document():
    out = tylax.typst_to_latex("= Hi\n\nbody", document=True)
    assert "\\documentclass{" in out
    assert "\\usepackage{amsmath}" in out
    assert "\\begin{document}" in out
    assert "\\end{document}" in out


def test_t2l_wrapper_omit_drops_documentclass_and_packages():
    opts = tylax.T2LOptions(wrapper_omit=True)
    out = tylax.typst_to_latex("= Hi\n\nbody", document=True, options=opts)
    assert "\\documentclass" not in out
    assert "\\usepackage" not in out
    assert "\\begin{document}" not in out
    assert "\\end{document}" not in out
    assert "\\section{" in out


def test_t2l_wrapper_custom_inserts_body_at_placeholder():
    template = "\\documentclass{minimal}\n\\begin{document}\n{body}\n\\end{document}\n"
    opts = tylax.T2LOptions(wrapper=template)
    out = tylax.typst_to_latex("= Hi", document=True, options=opts)
    assert out.startswith("\\documentclass{minimal}")
    assert "\\section{" in out
    assert out.rstrip().endswith("\\end{document}")
    assert "\\usepackage{amsmath}" not in out


def test_t2l_wrapper_missing_body_placeholder_raises_value_error():
    import pytest

    opts = tylax.T2LOptions(wrapper="\\documentclass{article}\nno placeholder")
    with pytest.raises(ValueError):
        tylax.typst_to_latex("= Hi", document=True, options=opts)


def test_l2t_preamble_omit_and_preamble_conflict_raises_value_error():
    import pytest

    opts = tylax.L2TOptions(preamble="X", preamble_omit=True)
    with pytest.raises(ValueError):
        tylax.latex_to_typst(SAMPLE_LATEX_DOC, document=True, options=opts)


def test_t2l_wrapper_omit_and_wrapper_conflict_raises_value_error():
    import pytest

    opts = tylax.T2LOptions(wrapper="x{body}y", wrapper_omit=True)
    with pytest.raises(ValueError):
        tylax.typst_to_latex("= Hi", document=True, options=opts)
