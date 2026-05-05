use pyo3::prelude::*;

// ---------------------------------------------------------------------------
// Result / Warning types
// ---------------------------------------------------------------------------

#[pyclass(frozen, get_all)]
#[derive(Clone)]
struct Span {
    start: usize,
    end: usize,
}

#[pymethods]
impl Span {
    fn __repr__(&self) -> String {
        format!("Span(start={}, end={})", self.start, self.end)
    }
}

#[pyclass(frozen, get_all)]
#[derive(Clone)]
struct ConversionWarning {
    kind: String,
    message: String,
    span: Option<Span>,
}

#[pymethods]
impl ConversionWarning {
    fn __repr__(&self) -> String {
        format!(
            "ConversionWarning(kind={:?}, message={:?})",
            self.kind, self.message
        )
    }
}

#[pyclass(frozen, get_all)]
struct ConversionResult {
    output: String,
    warnings: Vec<ConversionWarning>,
}

#[pymethods]
impl ConversionResult {
    fn __repr__(&self) -> String {
        format!(
            "ConversionResult(output=..., warnings=[{} item(s)])",
            self.warnings.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Option types
// ---------------------------------------------------------------------------

#[pyclass(get_all, set_all)]
#[derive(Clone)]
struct L2TOptions {
    prefer_shorthands: bool,
    frac_to_slash: bool,
    infty_to_oo: bool,
    keep_spaces: bool,
    non_strict: bool,
    optimize: bool,
    expand_macros: bool,
    /// If set, replaces the default `#set page/heading/math.equation`
    /// block with this string. Ignored when `preamble_omit` is `True`.
    preamble: Option<String>,
    /// `True` ⇒ emit no style preamble at all. Takes precedence over `preamble`.
    preamble_omit: bool,
}

#[pymethods]
impl L2TOptions {
    #[new]
    #[pyo3(signature = (
        *,
        prefer_shorthands = true,
        frac_to_slash = true,
        infty_to_oo = false,
        keep_spaces = false,
        non_strict = true,
        optimize = true,
        expand_macros = true,
        preamble = None,
        preamble_omit = false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        prefer_shorthands: bool,
        frac_to_slash: bool,
        infty_to_oo: bool,
        keep_spaces: bool,
        non_strict: bool,
        optimize: bool,
        expand_macros: bool,
        preamble: Option<String>,
        preamble_omit: bool,
    ) -> Self {
        Self {
            prefer_shorthands,
            frac_to_slash,
            infty_to_oo,
            keep_spaces,
            non_strict,
            optimize,
            expand_macros,
            preamble,
            preamble_omit,
        }
    }
}

impl TryFrom<&L2TOptions> for tylax::L2TOptions {
    type Error = pyo3::PyErr;

    fn try_from(py: &L2TOptions) -> Result<Self, Self::Error> {
        let preamble = match (py.preamble_omit, py.preamble.as_deref()) {
            (true, Some(_)) => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "preamble_omit=True conflicts with preamble=...; pick one",
                ));
            }
            (true, None) => tylax::PreambleMode::None,
            (false, Some(text)) => tylax::PreambleMode::Custom(text.to_string()),
            (false, None) => tylax::PreambleMode::Default,
        };
        Ok(tylax::L2TOptions {
            prefer_shorthands: py.prefer_shorthands,
            frac_to_slash: py.frac_to_slash,
            infty_to_oo: py.infty_to_oo,
            keep_spaces: py.keep_spaces,
            non_strict: py.non_strict,
            optimize: py.optimize,
            expand_macros: py.expand_macros,
            preamble,
        })
    }
}

#[pyclass(get_all, set_all)]
#[derive(Clone)]
struct T2LOptions {
    document_class: String,
    title: Option<String>,
    author: Option<String>,
    block_math_mode: bool,
    /// LaTeX wrapper template. Must contain the literal `{body}` placeholder.
    /// When set, replaces the default `\documentclass`/packages/`\begin{document}`
    /// wrapper.
    wrapper: Option<String>,
    /// True ⇒ emit only the converted body. Overrides `wrapper`.
    wrapper_omit: bool,
}

#[pymethods]
impl T2LOptions {
    #[new]
    #[pyo3(signature = (
        *,
        document_class = String::from("article"),
        title = None,
        author = None,
        block_math_mode = true,
        wrapper = None,
        wrapper_omit = false,
    ))]
    fn new(
        document_class: String,
        title: Option<String>,
        author: Option<String>,
        block_math_mode: bool,
        wrapper: Option<String>,
        wrapper_omit: bool,
    ) -> Self {
        Self {
            document_class,
            title,
            author,
            block_math_mode,
            wrapper,
            wrapper_omit,
        }
    }
}

fn build_l2t_options(py: Option<&L2TOptions>) -> PyResult<tylax::L2TOptions> {
    match py {
        Some(o) => tylax::L2TOptions::try_from(o),
        None => Ok(tylax::L2TOptions::default()),
    }
}

fn build_t2l_options(document: bool, py: Option<&T2LOptions>) -> PyResult<tylax::T2LOptions> {
    let wrapper_omit = py.is_some_and(|o| o.wrapper_omit);
    let wrapper_template = py.and_then(|o| o.wrapper.clone());
    let wrapper = match (wrapper_omit, wrapper_template) {
        (true, Some(_)) => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "wrapper_omit=True conflicts with wrapper=...; pick one",
            ));
        }
        (true, None) => tylax::DocumentWrapperMode::BodyOnly,
        (false, Some(template)) => {
            tylax::DocumentWrapperMode::from_template(&template).map_err(|msg| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid wrapper: {}", msg))
            })?
        }
        (false, None) => tylax::DocumentWrapperMode::Default,
    };
    Ok(tylax::T2LOptions {
        full_document: document,
        math_only: !document,
        document_class: py
            .map(|o| o.document_class.clone())
            .unwrap_or_else(|| "article".into()),
        title: py.and_then(|o| o.title.clone()),
        author: py.and_then(|o| o.author.clone()),
        block_math_mode: py.map(|o| o.block_math_mode).unwrap_or(true),
        wrapper,
    })
}

// ---------------------------------------------------------------------------
// Warning mapping helpers
// ---------------------------------------------------------------------------

fn map_l2t_warning(w: &tylax::core::latex2typst::ConversionWarning) -> ConversionWarning {
    let message = match &w.location {
        Some(loc) => format!("{} at {}", w.message, loc),
        None => w.message.clone(),
    };
    ConversionWarning {
        kind: format!("{:?}", w.kind),
        message,
        span: None,
    }
}

fn map_t2l_warning(w: &tylax::core::typst2latex::ConversionWarning) -> ConversionWarning {
    ConversionWarning {
        kind: format!("{:?}", w.kind),
        message: w.message.clone(),
        span: w.span.map(|s| Span {
            start: s.start,
            end: s.end,
        }),
    }
}

// ---------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (text, *, document = false, options = None))]
fn latex_to_typst(text: &str, document: bool, options: Option<&L2TOptions>) -> PyResult<String> {
    let opts = build_l2t_options(options)?;
    Ok(if document {
        tylax::latex_document_to_typst_with_options(text, &opts)
    } else {
        tylax::latex_to_typst_with_options(text, &opts)
    })
}

#[pyfunction]
#[pyo3(signature = (text, *, document = false, options = None))]
fn typst_to_latex(text: &str, document: bool, options: Option<&T2LOptions>) -> PyResult<String> {
    let opts = build_t2l_options(document, options)?;
    Ok(if document {
        tylax::typst_to_latex_with_eval(text, &opts)
    } else {
        tylax::typst_to_latex_with_options(text, &opts)
    })
}

#[pyfunction]
#[pyo3(signature = (text, *, document = false, options = None))]
fn latex_to_typst_diagnostics(
    text: &str,
    document: bool,
    options: Option<&L2TOptions>,
) -> PyResult<ConversionResult> {
    let opts = build_l2t_options(options)?;
    let mut converter = tylax::LatexConverter::with_options(opts);
    let result = if document {
        converter.convert_document_with_diagnostics(text)
    } else {
        converter.convert_math_with_diagnostics(text)
    };
    Ok(ConversionResult {
        output: result.output,
        warnings: result.warnings.iter().map(map_l2t_warning).collect(),
    })
}

#[pyfunction]
#[pyo3(signature = (text, *, document = false, options = None))]
fn typst_to_latex_diagnostics(
    text: &str,
    document: bool,
    options: Option<&T2LOptions>,
) -> PyResult<ConversionResult> {
    let opts = build_t2l_options(document, options)?;
    let result = tylax::typst_to_latex_with_diagnostics(text, &opts);
    Ok(ConversionResult {
        output: result.output,
        warnings: result.warnings.iter().map(map_t2l_warning).collect(),
    })
}

#[pyfunction]
fn detect_format(text: &str) -> &'static str {
    tylax::detect_format(text)
}

#[pyfunction]
#[pyo3(signature = (text, *, document = false))]
fn convert_auto(text: &str, document: bool) -> (String, &'static str) {
    if document {
        tylax::convert_auto_document(text)
    } else {
        tylax::convert_auto(text)
    }
}

// ---------------------------------------------------------------------------
// TikZ / CeTZ
// ---------------------------------------------------------------------------

#[pyfunction]
fn tikz_to_cetz(text: &str) -> String {
    tylax::tikz::convert_tikz_to_cetz(text)
}

#[pyfunction]
fn cetz_to_tikz(text: &str) -> String {
    tylax::tikz::convert_cetz_to_tikz(text)
}

#[pyfunction]
fn is_cetz_code(text: &str) -> bool {
    tylax::tikz::is_cetz_code(text)
}

// ---------------------------------------------------------------------------
// Module definition
// ---------------------------------------------------------------------------

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    m.add_class::<Span>()?;
    m.add_class::<ConversionWarning>()?;
    m.add_class::<ConversionResult>()?;
    m.add_class::<L2TOptions>()?;
    m.add_class::<T2LOptions>()?;

    m.add_function(wrap_pyfunction!(latex_to_typst, m)?)?;
    m.add_function(wrap_pyfunction!(typst_to_latex, m)?)?;
    m.add_function(wrap_pyfunction!(latex_to_typst_diagnostics, m)?)?;
    m.add_function(wrap_pyfunction!(typst_to_latex_diagnostics, m)?)?;
    m.add_function(wrap_pyfunction!(detect_format, m)?)?;
    m.add_function(wrap_pyfunction!(convert_auto, m)?)?;
    m.add_function(wrap_pyfunction!(tikz_to_cetz, m)?)?;
    m.add_function(wrap_pyfunction!(cetz_to_tikz, m)?)?;
    m.add_function(wrap_pyfunction!(is_cetz_code, m)?)?;

    Ok(())
}
