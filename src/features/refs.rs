//! Citation and Cross-Reference Module
//!
//! This module handles parsing and conversion of citations, labels, and
//! cross-references between LaTeX and Typst, inspired by Pandoc's
//! citation handling.

use std::collections::HashMap;

/// Citation mode (how the citation is displayed)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CitationMode {
    /// Normal citation: (Author, Year) or \[1\]
    #[default]
    Normal,
    /// Author in text: Author (Year)
    AuthorInText,
    /// Suppress author: (Year)
    SuppressAuthor,
    /// No parentheses
    NoParen,
}

/// A single citation
#[derive(Debug, Clone)]
pub struct Citation {
    /// Citation key (the reference ID)
    pub key: String,
    /// Text before the citation
    pub prefix: Option<String>,
    /// Text after the citation
    pub suffix: Option<String>,
    /// Citation mode
    pub mode: CitationMode,
    /// Page number or locator
    pub locator: Option<String>,
}

impl Citation {
    pub fn new(key: String) -> Self {
        Self {
            key,
            prefix: None,
            suffix: None,
            mode: CitationMode::Normal,
            locator: None,
        }
    }

    pub fn with_mode(key: String, mode: CitationMode) -> Self {
        Self {
            key,
            prefix: None,
            suffix: None,
            mode,
            locator: None,
        }
    }
}

/// A group of citations (for multiple citations in one reference)
#[derive(Debug, Clone, Default)]
pub struct CiteGroup {
    pub citations: Vec<Citation>,
    /// Common prefix for all citations
    pub prefix: Option<String>,
    /// Common suffix for all citations
    pub suffix: Option<String>,
}

impl CiteGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single(citation: Citation) -> Self {
        Self {
            citations: vec![citation],
            prefix: None,
            suffix: None,
        }
    }

    pub fn push(&mut self, citation: Citation) {
        self.citations.push(citation);
    }
}

/// A label for cross-referencing
#[derive(Debug, Clone)]
pub struct Label {
    /// Label identifier
    pub id: String,
    /// Type of labeled element
    pub label_type: LabelType,
    /// Display number (if assigned)
    pub number: Option<String>,
}

/// Type of labeled element
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelType {
    /// Section/chapter
    Section,
    /// Figure
    Figure,
    /// Table
    Table,
    /// Equation
    Equation,
    /// Theorem/lemma/etc.
    Theorem,
    /// Generic item
    Item,
}

impl Label {
    pub fn new(id: String, label_type: LabelType) -> Self {
        Self {
            id,
            label_type,
            number: None,
        }
    }
}

/// Reference to a label
#[derive(Debug, Clone)]
pub struct Reference {
    /// Target label ID
    pub target: String,
    /// Reference type
    pub ref_type: ReferenceType,
}

/// Type of reference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceType {
    /// Basic reference: "1.2"
    Basic,
    /// Reference with name: "Section 1.2"
    Named,
    /// Page reference: "page 5"
    Page,
    /// Equation reference: "(1)"
    Equation,
}

impl Reference {
    pub fn new(target: String) -> Self {
        Self {
            target,
            ref_type: ReferenceType::Basic,
        }
    }

    pub fn named(target: String) -> Self {
        Self {
            target,
            ref_type: ReferenceType::Named,
        }
    }
}

/// Reference database for tracking labels
#[derive(Debug, Default)]
pub struct RefDatabase {
    /// All defined labels
    labels: HashMap<String, Label>,
    /// Label counters by type
    counters: HashMap<LabelType, u32>,
}

impl RefDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new label
    pub fn register_label(&mut self, id: String, label_type: LabelType) -> &Label {
        let counter = self.counters.entry(label_type).or_insert(0);
        *counter += 1;

        let label = Label {
            id: id.clone(),
            label_type,
            number: Some(counter.to_string()),
        };

        self.labels.insert(id.clone(), label);
        self.labels.get(&id).unwrap()
    }

    /// Look up a label
    pub fn get_label(&self, id: &str) -> Option<&Label> {
        self.labels.get(id)
    }

    /// Check if a label exists
    pub fn has_label(&self, id: &str) -> bool {
        self.labels.contains_key(id)
    }
}

// ============================================================================
// LaTeX Citation Parsing
// ============================================================================

/// Parse LaTeX citation commands
pub fn parse_latex_citation(input: &str) -> Option<CiteGroup> {
    let input = input.trim();

    // Identify citation type using strip_prefix
    let (mode, rest) = if let Some(rest) = input.strip_prefix("\\cite{") {
        (CitationMode::Normal, rest)
    } else if let Some(rest) = input.strip_prefix("\\citep{") {
        (CitationMode::Normal, rest)
    } else if let Some(rest) = input.strip_prefix("\\citet{") {
        (CitationMode::AuthorInText, rest)
    } else if let Some(rest) = input.strip_prefix("\\citeyear{") {
        (CitationMode::SuppressAuthor, rest)
    } else if let Some(rest) = input.strip_prefix("\\citeauthor{") {
        (CitationMode::NoParen, rest)
    } else if let Some(rest) = input.strip_prefix("\\parencite{") {
        (CitationMode::Normal, rest)
    } else if let Some(rest) = input.strip_prefix("\\textcite{") {
        (CitationMode::AuthorInText, rest)
    } else if let Some(rest) = input.strip_prefix("\\autocite{") {
        (CitationMode::Normal, rest)
    } else {
        return None;
    };

    // Find closing brace
    let end = rest.find('}')?;
    let keys_str = &rest[..end];

    // Parse keys (comma-separated)
    let mut group = CiteGroup::new();
    for key in keys_str.split(',') {
        let key = key.trim();
        if !key.is_empty() {
            let mut citation = Citation::new(key.to_string());
            citation.mode = mode;
            group.push(citation);
        }
    }

    // Check for optional arguments (prefix/suffix)

    Some(group)
}

/// Parse LaTeX citation with optional arguments
pub fn parse_latex_citation_full(input: &str) -> Option<CiteGroup> {
    let input = input.trim();

    // Check for command
    let cmd_end = input.find('{')?;
    let cmd = &input[..cmd_end];
    let rest = &input[cmd_end..];

    // Determine mode from command
    let mode = match cmd {
        "\\cite" | "\\citep" | "\\parencite" | "\\autocite" => CitationMode::Normal,
        "\\citet" | "\\textcite" => CitationMode::AuthorInText,
        "\\citeyear" | "\\citeyearpar" => CitationMode::SuppressAuthor,
        "\\citeauthor" => CitationMode::NoParen,
        _ => CitationMode::Normal,
    };

    // Parse optional arguments before the main argument
    let (prefix, suffix, rest) = parse_optional_args(rest);

    // Parse main argument {keys}
    if !rest.starts_with('{') {
        return None;
    }

    let end = find_matching_brace(rest)?;
    let keys_str = &rest[1..end];

    // Parse keys
    let mut group = CiteGroup::new();
    group.prefix = prefix;
    group.suffix = suffix;

    for key in keys_str.split(',') {
        let key = key.trim();
        if !key.is_empty() {
            let mut citation = Citation::new(key.to_string());
            citation.mode = mode;
            group.push(citation);
        }
    }

    Some(group)
}

/// Parse optional [prefix][suffix] arguments
fn parse_optional_args(input: &str) -> (Option<String>, Option<String>, &str) {
    let mut rest = input;
    let mut first_opt = None;
    let mut second_opt = None;

    // First optional argument
    if rest.starts_with('[') {
        if let Some(end) = rest.find(']') {
            first_opt = Some(rest[1..end].to_string());
            rest = &rest[end + 1..];
        }
    }

    // Second optional argument
    if rest.starts_with('[') {
        if let Some(end) = rest.find(']') {
            second_opt = Some(rest[1..end].to_string());
            rest = &rest[end + 1..];
        }
    }

    // In natbib/biblatex, if two optional args:
    // first is prenote, second is postnote
    // If only one optional arg, it's postnote
    let (prefix, suffix) = match (first_opt, second_opt) {
        (Some(a), Some(b)) => (Some(a), Some(b)),
        (Some(a), None) => (None, Some(a)),
        _ => (None, None),
    };

    (prefix, suffix, rest)
}

/// Find matching brace
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

// ============================================================================
// LaTeX Reference Parsing
// ============================================================================

/// Parse LaTeX reference command
pub fn parse_latex_ref(input: &str) -> Option<Reference> {
    let input = input.trim();

    let (ref_type, rest) = if let Some(rest) = input.strip_prefix("\\ref{") {
        (ReferenceType::Basic, rest)
    } else if let Some(rest) = input.strip_prefix("\\eqref{") {
        (ReferenceType::Equation, rest)
    } else if let Some(rest) = input.strip_prefix("\\pageref{") {
        (ReferenceType::Page, rest)
    } else if let Some(rest) = input.strip_prefix("\\autoref{") {
        (ReferenceType::Named, rest)
    } else if let Some(rest) = input.strip_prefix("\\cref{") {
        (ReferenceType::Named, rest)
    } else if let Some(rest) = input.strip_prefix("\\Cref{") {
        (ReferenceType::Named, rest)
    } else {
        return None;
    };

    let end = rest.find('}')?;
    let target = rest[..end].trim().to_string();

    Some(Reference { target, ref_type })
}

/// Parse LaTeX label
pub fn parse_latex_label(input: &str) -> Option<String> {
    let input = input.trim();

    if !input.starts_with("\\label{") {
        return None;
    }

    let rest = &input["\\label{".len()..];
    let end = rest.find('}')?;

    Some(rest[..end].trim().to_string())
}

// ============================================================================
// Typst Citation Parsing
// ============================================================================

/// Parse Typst citation (`@key` or `#cite(<key>)`)
pub fn parse_typst_citation(input: &str) -> Option<CiteGroup> {
    let input = input.trim();

    if let Some(rest) = input.strip_prefix('@') {
        // Simple @key syntax
        let key: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ':')
            .collect();

        if key.is_empty() {
            return None;
        }

        let citation = Citation::new(key);
        return Some(CiteGroup::single(citation));
    }

    if input.starts_with("#cite(") || input.starts_with("cite(") {
        // #cite(<key>) or #cite(<key>, form: "prose")
        let start = input.find('<')? + 1;
        let end = input.find('>')?;
        let key = input[start..end].to_string();

        // Check for form argument
        let mode = if input.contains("form: \"prose\"") || input.contains("form: 'prose'") {
            CitationMode::AuthorInText
        } else if input.contains("form: \"year\"") || input.contains("form: 'year'") {
            CitationMode::SuppressAuthor
        } else {
            CitationMode::Normal
        };

        let citation = Citation::with_mode(key, mode);
        return Some(CiteGroup::single(citation));
    }

    None
}

/// Parse Typst reference (@label)
pub fn parse_typst_ref(input: &str) -> Option<Reference> {
    let input = input.trim();

    if !input.starts_with('@') {
        return None;
    }

    let target: String = input[1..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ':' || *c == '.')
        .collect();

    if target.is_empty() {
        return None;
    }

    Some(Reference::new(target))
}

/// Parse Typst label (`<label>`)
pub fn parse_typst_label(input: &str) -> Option<String> {
    let input = input.trim();

    if !input.starts_with('<') {
        return None;
    }

    let end = input.find('>')?;
    let label = input[1..end].to_string();

    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}

// ============================================================================
// Citation/Reference to LaTeX Conversion
// ============================================================================

/// Convert citation group to LaTeX
pub fn citation_to_latex(group: &CiteGroup) -> String {
    if group.citations.is_empty() {
        return String::new();
    }

    // Use first citation's mode for command selection
    let mode = group.citations[0].mode;

    let cmd = match mode {
        CitationMode::Normal => "\\cite",
        CitationMode::AuthorInText => "\\citet",
        CitationMode::SuppressAuthor => "\\citeyear",
        CitationMode::NoParen => "\\citeauthor",
    };

    let keys: Vec<&str> = group.citations.iter().map(|c| c.key.as_str()).collect();

    let mut result = String::new();
    result.push_str(cmd);

    // Add optional arguments
    if let Some(ref prefix) = group.prefix {
        if let Some(ref suffix) = group.suffix {
            result.push_str(&format!("[{}][{}]", prefix, suffix));
        } else {
            result.push_str(&format!("[{}]", prefix));
        }
    } else if let Some(ref suffix) = group.suffix {
        result.push_str(&format!("[{}]", suffix));
    }

    result.push('{');
    result.push_str(&keys.join(", "));
    result.push('}');

    result
}

/// Convert reference to LaTeX
pub fn reference_to_latex(reference: &Reference) -> String {
    let cmd = match reference.ref_type {
        ReferenceType::Basic => "\\ref",
        ReferenceType::Named => "\\autoref",
        ReferenceType::Page => "\\pageref",
        ReferenceType::Equation => "\\eqref",
    };

    format!("{}{{{}}}", cmd, reference.target)
}

/// Convert label to LaTeX
pub fn label_to_latex(id: &str) -> String {
    format!("\\label{{{}}}", id)
}

// ============================================================================
// Citation/Reference to Typst Conversion
// ============================================================================

/// Convert citation group to Typst
pub fn citation_to_typst(group: &CiteGroup) -> String {
    if group.citations.is_empty() {
        return String::new();
    }

    // For single citation with normal mode, use simple @key syntax
    if group.citations.len() == 1 {
        let citation = &group.citations[0];

        if citation.mode == CitationMode::Normal
            && citation.prefix.is_none()
            && citation.suffix.is_none()
            && is_simple_key(&citation.key)
        {
            let mut result = format!("@{}", citation.key);

            // Add suffix in brackets if group has suffix
            if let Some(ref suffix) = group.suffix {
                result.push_str(&format!("[{}]", suffix));
            }

            return result;
        }
    }

    // Otherwise use #cite() syntax
    let mut result = String::from("#cite(");

    // Add keys
    let keys: Vec<String> = group
        .citations
        .iter()
        .map(|c| format!("<{}>", c.key))
        .collect();
    result.push_str(&keys.join(", "));

    // Add form for non-normal modes
    let mode = group.citations[0].mode;
    match mode {
        CitationMode::AuthorInText => {
            result.push_str(", form: \"prose\"");
        }
        CitationMode::SuppressAuthor => {
            result.push_str(", form: \"year\"");
        }
        _ => {}
    }

    // Add supplement for suffix
    if let Some(ref suffix) = group.suffix {
        result.push_str(&format!(", supplement: [{}]", suffix));
    }

    result.push(')');
    result
}

/// Check if a citation key is simple (can use @key syntax)
fn is_simple_key(key: &str) -> bool {
    key.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Convert reference to Typst
pub fn reference_to_typst(reference: &Reference) -> String {
    if is_simple_key(&reference.target) {
        format!("@{}", reference.target)
    } else {
        format!("#ref(<{}>)", reference.target)
    }
}

/// Convert label to Typst
pub fn label_to_typst(id: &str) -> String {
    format!("<{}>", id)
}

// ============================================================================
// Bibliography Handling
// ============================================================================

/// Bibliography style
#[derive(Debug, Clone, Default)]
pub enum BibStyle {
    /// Numeric style: \[1\], \[2\]
    #[default]
    Numeric,
    /// Author-year style: (Author, 2020)
    AuthorYear,
    /// Alpha style: \[ABC20\]
    Alpha,
    /// Custom CSL style
    Custom(String),
}

/// Bibliography configuration
#[derive(Debug, Clone, Default)]
pub struct BibConfig {
    /// Bibliography file(s)
    pub files: Vec<String>,
    /// Citation style
    pub style: BibStyle,
    /// Title for bibliography section
    pub title: Option<String>,
}

/// Parse LaTeX bibliography commands
pub fn parse_latex_bibliography(input: &str) -> Option<BibConfig> {
    let mut config = BibConfig::default();

    // Look for \bibliography{file1,file2}
    if let Some(start) = input.find("\\bibliography{") {
        let rest = &input[start + "\\bibliography{".len()..];
        if let Some(end) = rest.find('}') {
            for file in rest[..end].split(',') {
                config.files.push(file.trim().to_string());
            }
        }
    }

    // Look for \bibliographystyle{style}
    if let Some(start) = input.find("\\bibliographystyle{") {
        let rest = &input[start + "\\bibliographystyle{".len()..];
        if let Some(end) = rest.find('}') {
            let style_name = rest[..end].trim();
            config.style = match style_name {
                "plain" | "unsrt" | "abbrv" => BibStyle::Numeric,
                "alpha" => BibStyle::Alpha,
                "apalike" | "natbib" | "chicago" => BibStyle::AuthorYear,
                other => BibStyle::Custom(other.to_string()),
            };
        }
    }

    if config.files.is_empty() {
        None
    } else {
        Some(config)
    }
}

/// Convert bibliography config to Typst
pub fn bibliography_to_typst(config: &BibConfig) -> String {
    let mut result = String::new();

    // Add bibliography directive
    if !config.files.is_empty() {
        let files: Vec<String> = config
            .files
            .iter()
            .map(|f| {
                let f = if f.ends_with(".bib") {
                    f.clone()
                } else {
                    format!("{}.bib", f)
                };
                format!("\"{}\"", f)
            })
            .collect();

        result.push_str("#bibliography(");
        result.push_str(&files.join(", "));

        // Add style if not default
        match &config.style {
            BibStyle::AuthorYear => {
                result.push_str(", style: \"apa\"");
            }
            BibStyle::Alpha => {
                result.push_str(", style: \"alphanumeric\"");
            }
            BibStyle::Custom(style) => {
                result.push_str(&format!(", style: \"{}\"", style));
            }
            _ => {}
        }

        // Add title if specified
        if let Some(ref title) = config.title {
            result.push_str(&format!(", title: \"{}\"", title));
        }

        result.push_str(")\n");
    }

    result
}

/// Convert bibliography config to LaTeX
pub fn bibliography_to_latex(config: &BibConfig) -> String {
    let mut result = String::new();

    // Add style
    let style = match &config.style {
        BibStyle::Numeric => "plain",
        BibStyle::AuthorYear => "apalike",
        BibStyle::Alpha => "alpha",
        BibStyle::Custom(s) => s.as_str(),
    };
    result.push_str(&format!("\\bibliographystyle{{{}}}\n", style));

    // Add bibliography
    if !config.files.is_empty() {
        let files: Vec<&str> = config
            .files
            .iter()
            .map(|f| f.trim_end_matches(".bib"))
            .collect();
        result.push_str(&format!("\\bibliography{{{}}}\n", files.join(",")));
    }

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_cite() {
        let group = parse_latex_citation("\\cite{key1}").unwrap();
        assert_eq!(group.citations.len(), 1);
        assert_eq!(group.citations[0].key, "key1");
    }

    #[test]
    fn test_parse_multiple_cite() {
        let group = parse_latex_citation("\\cite{key1, key2, key3}").unwrap();
        assert_eq!(group.citations.len(), 3);
    }

    #[test]
    fn test_parse_citet() {
        let group = parse_latex_citation("\\citet{author2020}").unwrap();
        assert_eq!(group.citations[0].mode, CitationMode::AuthorInText);
    }

    #[test]
    fn test_parse_typst_citation() {
        let group = parse_typst_citation("@author2020").unwrap();
        assert_eq!(group.citations[0].key, "author2020");
    }

    #[test]
    fn test_parse_typst_cite_func() {
        let group = parse_typst_citation("#cite(<key>, form: \"prose\")").unwrap();
        assert_eq!(group.citations[0].key, "key");
        assert_eq!(group.citations[0].mode, CitationMode::AuthorInText);
    }

    #[test]
    fn test_citation_to_latex() {
        let citation = Citation::new("test2020".to_string());
        let group = CiteGroup::single(citation);
        let latex = citation_to_latex(&group);
        assert_eq!(latex, "\\cite{test2020}");
    }

    #[test]
    fn test_citation_to_typst() {
        let citation = Citation::new("test2020".to_string());
        let group = CiteGroup::single(citation);
        let typst = citation_to_typst(&group);
        assert_eq!(typst, "@test2020");
    }

    #[test]
    fn test_parse_ref() {
        let reference = parse_latex_ref("\\ref{fig:example}").unwrap();
        assert_eq!(reference.target, "fig:example");
        assert_eq!(reference.ref_type, ReferenceType::Basic);
    }

    #[test]
    fn test_parse_eqref() {
        let reference = parse_latex_ref("\\eqref{eq:main}").unwrap();
        assert_eq!(reference.ref_type, ReferenceType::Equation);
    }

    #[test]
    fn test_reference_to_latex() {
        let reference = Reference::new("fig:1".to_string());
        assert_eq!(reference_to_latex(&reference), "\\ref{fig:1}");
    }

    #[test]
    fn test_reference_to_typst() {
        let reference = Reference::new("fig-1".to_string());
        assert_eq!(reference_to_typst(&reference), "@fig-1");
    }

    #[test]
    fn test_label_conversions() {
        assert_eq!(label_to_latex("sec:intro"), "\\label{sec:intro}");
        assert_eq!(label_to_typst("sec-intro"), "<sec-intro>");
    }

    #[test]
    fn test_parse_latex_label() {
        let label = parse_latex_label("\\label{fig:example}").unwrap();
        assert_eq!(label, "fig:example");
    }

    #[test]
    fn test_parse_typst_label() {
        let label = parse_typst_label("<fig-example>").unwrap();
        assert_eq!(label, "fig-example");
    }

    #[test]
    fn test_bibliography_parsing() {
        let input = r#"
\bibliographystyle{apalike}
\bibliography{refs,more_refs}
"#;
        let config = parse_latex_bibliography(input).unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(matches!(config.style, BibStyle::AuthorYear));
    }

    #[test]
    fn test_bibliography_to_typst() {
        let config = BibConfig {
            files: vec!["refs.bib".to_string()],
            style: BibStyle::AuthorYear,
            title: None,
        };
        let typst = bibliography_to_typst(&config);
        assert!(typst.contains("#bibliography"));
        assert!(typst.contains("refs.bib"));
    }

    #[test]
    fn test_ref_database() {
        let mut db = RefDatabase::new();
        db.register_label("fig:1".to_string(), LabelType::Figure);
        db.register_label("fig:2".to_string(), LabelType::Figure);

        assert!(db.has_label("fig:1"));
        assert_eq!(db.get_label("fig:1").unwrap().number, Some("1".to_string()));
        assert_eq!(db.get_label("fig:2").unwrap().number, Some("2".to_string()));
    }
}
