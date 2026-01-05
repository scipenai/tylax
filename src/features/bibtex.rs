//! BibTeX file parser and converter
//!
//! This module parses BibTeX (.bib) files and converts them to Typst-compatible
//! bibliography format. It handles:
//!
//! - Standard BibTeX entry types (@article, @book, @inproceedings, etc.)
//! - LaTeX special character encoding ({\"o} -> ö)
//! - Field value parsing with proper brace/quote handling
//! - Cross-reference resolution
//! - Conversion to Typst's bibliography format (YAML or native)
//!
//! ## Example
//!
//! ```rust
//! use tylax::bibtex::{parse_bibtex, BibEntry};
//!
//! let bib = r#"
//! @article{einstein1905,
//!   author = {Albert Einstein},
//!   title = {On the Electrodynamics of Moving Bodies},
//!   journal = {Annalen der Physik},
//!   year = {1905}
//! }
//! "#;
//!
//! let entries = parse_bibtex(bib);
//! assert_eq!(entries.len(), 1);
//! assert_eq!(entries[0].entry_type, "article");
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Write;

lazy_static! {
    // Entry pattern: @type{key, ...}
    static ref ENTRY_PATTERN: Regex = Regex::new(
        r"@(\w+)\s*\{([^,]+),"
    ).unwrap();

    // Field pattern: name = value
    static ref FIELD_PATTERN: Regex = Regex::new(
        r"(\w+)\s*=\s*"
    ).unwrap();

    // LaTeX accent patterns
    static ref ACCENT_PATTERNS: Vec<(Regex, &'static str)> = vec![
        // Umlaut: {\"o} or \"o or \"{}o
        (Regex::new(r#"\{\\"([aeiouAEIOU])\}"#).unwrap(), ""),
        (Regex::new(r#"\\"([aeiouAEIOU])"#).unwrap(), ""),
        // Acute: {\'o} or \'o
        (Regex::new(r#"\{\\'([aeiouAEIOU])\}"#).unwrap(), ""),
        (Regex::new(r#"\\'([aeiouAEIOU])"#).unwrap(), ""),
        // Grave: {\`o} or \`o
        (Regex::new(r#"\{\\`([aeiouAEIOU])\}"#).unwrap(), ""),
        (Regex::new(r#"\\`([aeiouAEIOU])"#).unwrap(), ""),
        // Circumflex: {\^o} or \^o
        (Regex::new(r#"\{\\\^([aeiouAEIOU])\}"#).unwrap(), ""),
        (Regex::new(r#"\\\^([aeiouAEIOU])"#).unwrap(), ""),
        // Tilde: {\~n} or \~n
        (Regex::new(r#"\{\\~([nNaAoO])\}"#).unwrap(), ""),
        (Regex::new(r#"\\~([nNaAoO])"#).unwrap(), ""),
    ];
}

/// A single BibTeX entry
#[derive(Debug, Clone, Default)]
pub struct BibEntry {
    /// Entry type (article, book, inproceedings, etc.)
    pub entry_type: String,
    /// Citation key
    pub key: String,
    /// Fields and their values
    pub fields: HashMap<String, String>,
}

impl BibEntry {
    /// Create a new empty entry
    pub fn new(entry_type: &str, key: &str) -> Self {
        Self {
            entry_type: entry_type.to_lowercase(),
            key: key.to_string(),
            fields: HashMap::new(),
        }
    }

    /// Get a field value
    pub fn get(&self, field: &str) -> Option<&str> {
        self.fields.get(field).map(|s| s.as_str())
    }

    /// Set a field value
    pub fn set(&mut self, field: &str, value: &str) {
        self.fields.insert(field.to_lowercase(), value.to_string());
    }

    /// Get author field
    pub fn author(&self) -> Option<&str> {
        self.get("author")
    }

    /// Get title field
    pub fn title(&self) -> Option<&str> {
        self.get("title")
    }

    /// Get year field
    pub fn year(&self) -> Option<&str> {
        self.get("year").or_else(|| self.get("date"))
    }

    /// Get journal/booktitle
    pub fn venue(&self) -> Option<&str> {
        self.get("journal")
            .or_else(|| self.get("booktitle"))
            .or_else(|| self.get("publisher"))
    }

    /// Convert to Typst bibliography YAML format
    pub fn to_yaml(&self) -> String {
        let mut yaml = String::new();
        let _ = writeln!(yaml, "{}:", self.key);

        // Type mapping
        let typst_type = match self.entry_type.as_str() {
            "article" => "article",
            "book" => "book",
            "inproceedings" | "conference" => "article", // Typst doesn't have conference
            "incollection" => "chapter",
            "phdthesis" | "mastersthesis" => "thesis",
            "techreport" => "report",
            "misc" | "online" => "web",
            _ => "article",
        };
        let _ = writeln!(yaml, "  type: {}", typst_type);

        // Title
        if let Some(title) = self.title() {
            let _ = writeln!(yaml, "  title: \"{}\"", escape_yaml_string(title));
        }

        // Authors - split by "and"
        if let Some(author) = self.author() {
            let authors: Vec<&str> = author.split(" and ").collect();
            if authors.len() == 1 {
                let _ = writeln!(
                    yaml,
                    "  author: \"{}\"",
                    escape_yaml_string(authors[0].trim())
                );
            } else {
                yaml.push_str("  author:\n");
                for auth in authors {
                    let _ = writeln!(yaml, "    - \"{}\"", escape_yaml_string(auth.trim()));
                }
            }
        }

        // Year/Date
        if let Some(year) = self.year() {
            let _ = writeln!(yaml, "  date: {}", year);
        }

        // Venue (journal, booktitle, etc.)
        if let Some(venue) = self.venue() {
            let field_name = if self.entry_type == "book" {
                "publisher"
            } else {
                "parent"
            };
            let _ = writeln!(yaml, "  {}: \"{}\"", field_name, escape_yaml_string(venue));
        }

        // Volume, number, pages
        if let Some(volume) = self.get("volume") {
            let _ = writeln!(yaml, "  volume: {}", volume);
        }
        if let Some(number) = self.get("number") {
            let _ = writeln!(yaml, "  issue: {}", number);
        }
        if let Some(pages) = self.get("pages") {
            let _ = writeln!(yaml, "  page: \"{}\"", pages.replace("--", "-"));
        }

        // DOI
        if let Some(doi) = self.get("doi") {
            let _ = writeln!(yaml, "  doi: \"{}\"", doi);
        }

        // URL
        if let Some(url) = self.get("url") {
            let _ = writeln!(yaml, "  url: \"{}\"", url);
        }

        // ISBN/ISSN
        if let Some(isbn) = self.get("isbn") {
            let _ = writeln!(yaml, "  isbn: \"{}\"", isbn);
        }

        yaml
    }

    /// Convert to Typst inline citation format
    pub fn to_typst_inline(&self) -> String {
        format!("@{}", self.key)
    }
}

/// Escape string for YAML
fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

/// A collection of BibTeX entries
#[derive(Debug, Clone, Default)]
pub struct Bibliography {
    /// All entries keyed by citation key
    pub entries: HashMap<String, BibEntry>,
    /// String definitions (@string{...})
    pub strings: HashMap<String, String>,
    /// Preamble content
    pub preamble: Vec<String>,
}

impl Bibliography {
    /// Create new empty bibliography
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry
    pub fn add_entry(&mut self, entry: BibEntry) {
        self.entries.insert(entry.key.clone(), entry);
    }

    /// Get an entry by key
    pub fn get(&self, key: &str) -> Option<&BibEntry> {
        self.entries.get(key)
    }

    /// Get all entries
    pub fn all_entries(&self) -> Vec<&BibEntry> {
        self.entries.values().collect()
    }

    /// Convert entire bibliography to YAML
    pub fn to_yaml(&self) -> String {
        let mut yaml = String::new();
        for entry in self.entries.values() {
            yaml.push_str(&entry.to_yaml());
            yaml.push('\n');
        }
        yaml
    }

    /// Generate Typst bibliography command
    pub fn to_typst_bibliography(&self, bib_file: &str) -> String {
        format!("#bibliography(\"{}\")", bib_file)
    }

    /// Generate inline bibliography data (for documents without external .bib file)
    pub fn to_typst_inline_data(&self) -> String {
        let yaml = self.to_yaml();
        format!("#let bibliography-data = yaml(`\n{}`)\n", yaml)
    }
}

/// Parse a complete BibTeX file
pub fn parse_bibtex(input: &str) -> Vec<BibEntry> {
    let mut entries = Vec::new();
    let mut current_pos = 0;

    while current_pos < input.len() {
        let remaining = &input[current_pos..];

        // Find next entry
        if let Some(at_pos) = remaining.find('@') {
            let entry_start = current_pos + at_pos;
            let entry_content = &input[entry_start..];

            // Determine entry type
            if let Some(brace_pos) = entry_content.find('{') {
                let entry_type = entry_content[1..brace_pos].trim().to_lowercase();

                // Handle special entries
                match entry_type.as_str() {
                    "string" | "preamble" | "comment" => {
                        // Skip these for now
                        if let Some(end) = find_matching_brace(&entry_content[brace_pos..]) {
                            current_pos = entry_start + brace_pos + end + 1;
                        } else {
                            current_pos = entry_start + 1;
                        }
                        continue;
                    }
                    _ => {}
                }

                // Parse regular entry
                if let Some(end_brace) = find_matching_brace(&entry_content[brace_pos..]) {
                    let full_entry = &entry_content[..brace_pos + end_brace + 1];
                    if let Some(entry) = parse_single_entry(full_entry) {
                        entries.push(entry);
                    }
                    current_pos = entry_start + brace_pos + end_brace + 1;
                } else {
                    current_pos = entry_start + 1;
                }
            } else {
                current_pos = entry_start + 1;
            }
        } else {
            break;
        }
    }

    entries
}

/// Parse a single BibTeX entry
fn parse_single_entry(input: &str) -> Option<BibEntry> {
    // Extract entry type and key: @type{key,
    let input = input.trim();

    // Find @
    let at_pos = input.find('@')?;
    let after_at = &input[at_pos + 1..];

    // Find opening brace
    let brace_pos = after_at.find('{')?;
    let entry_type = after_at[..brace_pos].trim();

    // Find comma after key
    let after_brace = &after_at[brace_pos + 1..];
    let comma_pos = after_brace.find(',')?;
    let key = after_brace[..comma_pos].trim();

    let mut entry = BibEntry::new(entry_type, key);

    // Parse fields
    let fields_content = &after_brace[comma_pos + 1..];
    parse_fields(fields_content, &mut entry);

    // Clean up field values (remove LaTeX encoding)
    let cleaned_fields: HashMap<String, String> = entry
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), clean_latex_encoding(v)))
        .collect();
    entry.fields = cleaned_fields;

    Some(entry)
}

/// Parse fields from entry content
fn parse_fields(content: &str, entry: &mut BibEntry) {
    let content = content.trim();
    if content.is_empty() || content == "}" {
        return;
    }

    let mut current_pos = 0;
    let chars: Vec<char> = content.chars().collect();

    while current_pos < chars.len() {
        // Skip whitespace
        while current_pos < chars.len() && chars[current_pos].is_whitespace() {
            current_pos += 1;
        }

        if current_pos >= chars.len() || chars[current_pos] == '}' {
            break;
        }

        // Find field name
        let name_start = current_pos;
        while current_pos < chars.len()
            && (chars[current_pos].is_alphanumeric() || chars[current_pos] == '_')
        {
            current_pos += 1;
        }
        let field_name: String = chars[name_start..current_pos].iter().collect();

        if field_name.is_empty() {
            current_pos += 1;
            continue;
        }

        // Skip whitespace and =
        while current_pos < chars.len()
            && (chars[current_pos].is_whitespace() || chars[current_pos] == '=')
        {
            current_pos += 1;
        }

        // Parse value
        if current_pos < chars.len() {
            let (value, end_pos) = parse_field_value(&chars[current_pos..]);
            entry.set(&field_name.to_lowercase(), &value);
            current_pos += end_pos;

            // Skip comma
            while current_pos < chars.len()
                && (chars[current_pos].is_whitespace() || chars[current_pos] == ',')
            {
                current_pos += 1;
            }
        }
    }
}

/// Parse a field value (handles braces, quotes, and concatenation)
fn parse_field_value(chars: &[char]) -> (String, usize) {
    if chars.is_empty() {
        return (String::new(), 0);
    }

    let mut value = String::new();
    let mut pos = 0;

    loop {
        // Skip whitespace
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }

        if pos >= chars.len() {
            break;
        }

        let c = chars[pos];

        if c == '{' {
            // Braced value
            let (braced, end) = extract_braced(&chars[pos..]);
            value.push_str(&braced);
            pos += end;
        } else if c == '"' {
            // Quoted value
            let (quoted, end) = extract_quoted(&chars[pos..]);
            value.push_str(&quoted);
            pos += end;
        } else if c.is_alphanumeric() {
            // Bare word (number or string variable)
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                pos += 1;
            }
            let word: String = chars[start..pos].iter().collect();
            value.push_str(&word);
        } else if c == '#' {
            // Concatenation - continue parsing
            pos += 1;
            continue;
        } else if c == ',' || c == '}' {
            // End of field
            break;
        } else {
            pos += 1;
        }

        // Check for concatenation
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos < chars.len() && chars[pos] == '#' {
            pos += 1;
            continue;
        } else {
            break;
        }
    }

    (value, pos)
}

/// Extract content within braces
fn extract_braced(chars: &[char]) -> (String, usize) {
    if chars.is_empty() || chars[0] != '{' {
        return (String::new(), 0);
    }

    let mut depth = 0;
    let mut end = 0;

    for (i, &c) in chars.iter().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let content: String = chars[1..end].iter().collect();
    (content, end + 1)
}

/// Extract content within quotes
fn extract_quoted(chars: &[char]) -> (String, usize) {
    if chars.is_empty() || chars[0] != '"' {
        return (String::new(), 0);
    }

    let mut pos = 1;
    let mut content = String::new();

    while pos < chars.len() {
        let c = chars[pos];
        if c == '"' {
            // End of quoted string
            pos += 1;
            break;
        } else if c == '\\' && pos + 1 < chars.len() {
            // Escape sequence
            content.push(chars[pos + 1]);
            pos += 2;
        } else if c == '{' {
            // Nested braces
            let (braced, end) = extract_braced(&chars[pos..]);
            content.push_str(&braced);
            pos += end;
        } else {
            content.push(c);
            pos += 1;
        }
    }

    (content, pos)
}

/// Find matching closing brace
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

/// Clean LaTeX encoding from a string
pub fn clean_latex_encoding(input: &str) -> String {
    let mut result = input.to_string();

    // Remove protective braces: {Einstein} -> Einstein
    while result.contains('{') && result.contains('}') {
        let mut new_result = String::new();
        let mut depth = 0;
        let mut last_open = 0;

        for (i, c) in result.char_indices() {
            match c {
                '{' => {
                    if depth == 0 {
                        last_open = i;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if this is a simple protective brace
                        let content = &result[last_open + 1..i];
                        if !content.starts_with('\\') {
                            new_result.push_str(content);
                        } else {
                            new_result.push('{');
                            new_result.push_str(content);
                            new_result.push('}');
                        }
                    }
                }
                _ => {
                    if depth == 0 {
                        new_result.push(c);
                    }
                }
            }
        }

        if new_result == result {
            break;
        }
        result = new_result;
    }

    // Convert LaTeX accents to Unicode
    result = convert_latex_accents(&result);

    // Clean up remaining LaTeX commands
    result = result.replace("\\&", "&");
    result = result.replace("\\%", "%");
    result = result.replace("\\$", "$");
    result = result.replace("\\#", "#");
    result = result.replace("\\_", "_");
    result = result.replace("\\textit", "");
    result = result.replace("\\textbf", "");
    result = result.replace("\\emph", "");
    result = result.replace("~", " ");
    result = result.replace("--", "–");
    result = result.replace("---", "—");

    // Clean up extra whitespace
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }

    result.trim().to_string()
}

/// Convert LaTeX accent commands to Unicode characters
fn convert_latex_accents(input: &str) -> String {
    let mut result = input.to_string();

    // Umlaut/Dieresis: \"
    let umlauts = [
        (r#"\"{a}"#, "ä"),
        (r#"\"a"#, "ä"),
        (r#"\"{A}"#, "Ä"),
        (r#"\"A"#, "Ä"),
        (r#"\"{e}"#, "ë"),
        (r#"\"e"#, "ë"),
        (r#"\"{E}"#, "Ë"),
        (r#"\"E"#, "Ë"),
        (r#"\"{i}"#, "ï"),
        (r#"\"i"#, "ï"),
        (r#"\"{I}"#, "Ï"),
        (r#"\"I"#, "Ï"),
        (r#"\"{o}"#, "ö"),
        (r#"\"o"#, "ö"),
        (r#"\"{O}"#, "Ö"),
        (r#"\"O"#, "Ö"),
        (r#"\"{u}"#, "ü"),
        (r#"\"u"#, "ü"),
        (r#"\"{U}"#, "Ü"),
        (r#"\"U"#, "Ü"),
        (r#"\"{y}"#, "ÿ"),
        (r#"\"y"#, "ÿ"),
    ];
    for (from, to) in umlauts {
        result = result.replace(from, to);
    }

    // Acute: \'
    let acutes = [
        (r#"\'{a}"#, "á"),
        (r#"\'a"#, "á"),
        (r#"\'{A}"#, "Á"),
        (r#"\'A"#, "Á"),
        (r#"\'{e}"#, "é"),
        (r#"\'e"#, "é"),
        (r#"\'{E}"#, "É"),
        (r#"\'E"#, "É"),
        (r#"\'{i}"#, "í"),
        (r#"\'i"#, "í"),
        (r#"\'{I}"#, "Í"),
        (r#"\'I"#, "Í"),
        (r#"\'{o}"#, "ó"),
        (r#"\'o"#, "ó"),
        (r#"\'{O}"#, "Ó"),
        (r#"\'O"#, "Ó"),
        (r#"\'{u}"#, "ú"),
        (r#"\'u"#, "ú"),
        (r#"\'{U}"#, "Ú"),
        (r#"\'U"#, "Ú"),
    ];
    for (from, to) in acutes {
        result = result.replace(from, to);
    }

    // Grave: \`
    let graves = [
        (r#"\`{a}"#, "à"),
        (r#"\`a"#, "à"),
        (r#"\`{A}"#, "À"),
        (r#"\`A"#, "À"),
        (r#"\`{e}"#, "è"),
        (r#"\`e"#, "è"),
        (r#"\`{E}"#, "È"),
        (r#"\`E"#, "È"),
        (r#"\`{i}"#, "ì"),
        (r#"\`i"#, "ì"),
        (r#"\`{I}"#, "Ì"),
        (r#"\`I"#, "Ì"),
        (r#"\`{o}"#, "ò"),
        (r#"\`o"#, "ò"),
        (r#"\`{O}"#, "Ò"),
        (r#"\`O"#, "Ò"),
        (r#"\`{u}"#, "ù"),
        (r#"\`u"#, "ù"),
        (r#"\`{U}"#, "Ù"),
        (r#"\`U"#, "Ù"),
    ];
    for (from, to) in graves {
        result = result.replace(from, to);
    }

    // Circumflex: \^
    let circumflexes = [
        (r#"\^{a}"#, "â"),
        (r#"\^a"#, "â"),
        (r#"\^{A}"#, "Â"),
        (r#"\^A"#, "Â"),
        (r#"\^{e}"#, "ê"),
        (r#"\^e"#, "ê"),
        (r#"\^{E}"#, "Ê"),
        (r#"\^E"#, "Ê"),
        (r#"\^{i}"#, "î"),
        (r#"\^i"#, "î"),
        (r#"\^{I}"#, "Î"),
        (r#"\^I"#, "Î"),
        (r#"\^{o}"#, "ô"),
        (r#"\^o"#, "ô"),
        (r#"\^{O}"#, "Ô"),
        (r#"\^O"#, "Ô"),
        (r#"\^{u}"#, "û"),
        (r#"\^u"#, "û"),
        (r#"\^{U}"#, "Û"),
        (r#"\^U"#, "Û"),
    ];
    for (from, to) in circumflexes {
        result = result.replace(from, to);
    }

    // Tilde: \~
    let tildes = [
        (r#"\~{n}"#, "ñ"),
        (r#"\~n"#, "ñ"),
        (r#"\~{N}"#, "Ñ"),
        (r#"\~N"#, "Ñ"),
        (r#"\~{a}"#, "ã"),
        (r#"\~a"#, "ã"),
        (r#"\~{A}"#, "Ã"),
        (r#"\~A"#, "Ã"),
        (r#"\~{o}"#, "õ"),
        (r#"\~o"#, "õ"),
        (r#"\~{O}"#, "Õ"),
        (r#"\~O"#, "Õ"),
    ];
    for (from, to) in tildes {
        result = result.replace(from, to);
    }

    // Cedilla: \c
    let cedillas = [
        (r#"\c{c}"#, "ç"),
        (r#"\c c"#, "ç"),
        (r#"\c{C}"#, "Ç"),
        (r#"\c C"#, "Ç"),
    ];
    for (from, to) in cedillas {
        result = result.replace(from, to);
    }

    // Special characters
    let specials = [
        (r#"\ss"#, "ß"),
        (r#"\ss{}"#, "ß"),
        (r#"\ae"#, "æ"),
        (r#"\AE"#, "Æ"),
        (r#"\oe"#, "œ"),
        (r#"\OE"#, "Œ"),
        (r#"\o"#, "ø"),
        (r#"\O"#, "Ø"),
        (r#"\aa"#, "å"),
        (r#"\AA"#, "Å"),
        (r#"\l"#, "ł"),
        (r#"\L"#, "Ł"),
    ];
    for (from, to) in specials {
        result = result.replace(from, to);
    }

    result
}

/// Generate Typst bibliography setup for a document
pub fn generate_typst_bibliography_setup(bib_file: &str, style: Option<&str>) -> String {
    let style_str = style.unwrap_or("ieee");
    format!(
        "#bibliography(\"{}\", style: \"{}\")\n",
        bib_file, style_str
    )
}

/// Convert a .bib file path to Typst bibliography command
pub fn convert_bibliography_command(bib_path: &str) -> String {
    // Just generate the bibliography command - Typst can read .bib files directly
    format!("#bibliography(\"{}\")", bib_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_entry() {
        let bib = r#"
@article{einstein1905,
  author = {Albert Einstein},
  title = {On the Electrodynamics of Moving Bodies},
  journal = {Annalen der Physik},
  year = {1905}
}
"#;
        let entries = parse_bibtex(bib);
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.entry_type, "article");
        assert_eq!(entry.key, "einstein1905");
        assert_eq!(entry.author(), Some("Albert Einstein"));
        assert_eq!(
            entry.title(),
            Some("On the Electrodynamics of Moving Bodies")
        );
        assert_eq!(entry.year(), Some("1905"));
    }

    #[test]
    fn test_parse_multiple_entries() {
        let bib = r#"
@article{paper1, author = {A}, title = {B}, year = {2020}}
@book{book1, author = {C}, title = {D}, year = {2021}}
"#;
        let entries = parse_bibtex(bib);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_clean_latex_accents() {
        assert_eq!(clean_latex_encoding(r#"M\"uller"#), "Müller");
        assert_eq!(clean_latex_encoding(r#"Caf\'e"#), "Café");
        assert_eq!(clean_latex_encoding(r#"Stra\ss{}e"#), "Straße");
        assert_eq!(clean_latex_encoding(r#"\ss"#), "ß");
    }

    #[test]
    fn test_clean_protective_braces() {
        assert_eq!(clean_latex_encoding("{Einstein}"), "Einstein");
        assert_eq!(
            clean_latex_encoding("The {DNA} Structure"),
            "The DNA Structure"
        );
    }

    #[test]
    fn test_to_yaml() {
        let mut entry = BibEntry::new("article", "test2023");
        entry.set("author", "John Doe and Jane Smith");
        entry.set("title", "A Test Paper");
        entry.set("year", "2023");
        entry.set("journal", "Test Journal");

        let yaml = entry.to_yaml();
        assert!(yaml.contains("test2023:"));
        assert!(yaml.contains("type: article"));
        assert!(yaml.contains("title: \"A Test Paper\""));
        assert!(yaml.contains("John Doe"));
        assert!(yaml.contains("Jane Smith"));
    }

    #[test]
    fn test_quoted_values() {
        let bib = r#"
@article{test,
  author = "John Doe",
  title = "A \"Quoted\" Title",
  year = "2023"
}
"#;
        let entries = parse_bibtex(bib);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].author(), Some("John Doe"));
    }

    #[test]
    fn test_concatenation() {
        // BibTeX allows: author = first # " and " # second
        let bib = r#"
@article{test,
  author = {John} # { Doe},
  year = {2023}
}
"#;
        let entries = parse_bibtex(bib);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].author(), Some("John Doe"));
    }

    #[test]
    fn test_bibliography_struct() {
        let mut bib = Bibliography::new();

        let mut entry = BibEntry::new("article", "key1");
        entry.set("title", "Test");
        bib.add_entry(entry);

        assert!(bib.get("key1").is_some());
        assert_eq!(bib.all_entries().len(), 1);
    }
}
