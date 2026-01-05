//! Constants and mapping tables for LaTeX to Typst conversion
//!
//! This module contains comprehensive mappings migrated from legacy modules:
//! - Language mappings for code environments (166+ languages)
//! - Theorem type mappings (57+ theorem types)
//! - Theorem styles

use lazy_static::lazy_static;
use std::collections::HashMap;

// ============================================================================
// Language Mappings for Code Environments
// ============================================================================

lazy_static! {
    /// Language name mappings from LaTeX packages (lstlisting, minted) to Typst
    /// Covers common variations in case and abbreviations
    pub static ref LANGUAGE_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();

        // ---- Common programming languages ----
        m.insert("python", "python");
        m.insert("Python", "python");
        m.insert("py", "python");
        m.insert("python3", "python");
        m.insert("Python3", "python");

        m.insert("rust", "rust");
        m.insert("Rust", "rust");
        m.insert("rs", "rust");

        m.insert("javascript", "javascript");
        m.insert("JavaScript", "javascript");
        m.insert("js", "javascript");
        m.insert("JS", "javascript");
        m.insert("ecmascript", "javascript");

        m.insert("typescript", "typescript");
        m.insert("TypeScript", "typescript");
        m.insert("ts", "typescript");
        m.insert("TS", "typescript");

        m.insert("java", "java");
        m.insert("Java", "java");

        m.insert("c", "c");
        m.insert("C", "c");

        m.insert("cpp", "cpp");
        m.insert("c++", "cpp");
        m.insert("C++", "cpp");
        m.insert("Cpp", "cpp");
        m.insert("CPP", "cpp");
        m.insert("cxx", "cpp");

        m.insert("csharp", "csharp");
        m.insert("cs", "csharp");
        m.insert("C#", "csharp");
        m.insert("CSharp", "csharp");

        m.insert("go", "go");
        m.insert("Go", "go");
        m.insert("golang", "go");
        m.insert("Golang", "go");

        m.insert("ruby", "ruby");
        m.insert("Ruby", "ruby");
        m.insert("rb", "ruby");

        m.insert("php", "php");
        m.insert("PHP", "php");
        m.insert("php7", "php");
        m.insert("php8", "php");

        m.insert("swift", "swift");
        m.insert("Swift", "swift");

        m.insert("kotlin", "kotlin");
        m.insert("Kotlin", "kotlin");
        m.insert("kt", "kotlin");

        m.insert("scala", "scala");
        m.insert("Scala", "scala");

        m.insert("r", "r");
        m.insert("R", "r");
        m.insert("rlang", "r");

        m.insert("julia", "julia");
        m.insert("Julia", "julia");
        m.insert("jl", "julia");

        m.insert("matlab", "matlab");
        m.insert("MATLAB", "matlab");
        m.insert("Matlab", "matlab");
        m.insert("octave", "matlab");
        m.insert("Octave", "matlab");

        m.insert("haskell", "haskell");
        m.insert("Haskell", "haskell");
        m.insert("hs", "haskell");

        m.insert("ocaml", "ocaml");
        m.insert("OCaml", "ocaml");
        m.insert("ml", "ocaml");

        m.insert("fsharp", "fsharp");
        m.insert("F#", "fsharp");
        m.insert("FSharp", "fsharp");

        m.insert("erlang", "erlang");
        m.insert("Erlang", "erlang");

        m.insert("elixir", "elixir");
        m.insert("Elixir", "elixir");
        m.insert("ex", "elixir");

        m.insert("clojure", "clojure");
        m.insert("Clojure", "clojure");
        m.insert("clj", "clojure");

        m.insert("lisp", "lisp");
        m.insert("Lisp", "lisp");
        m.insert("commonlisp", "lisp");
        m.insert("CommonLisp", "lisp");

        m.insert("scheme", "scheme");
        m.insert("Scheme", "scheme");

        m.insert("racket", "racket");
        m.insert("Racket", "racket");

        m.insert("lua", "lua");
        m.insert("Lua", "lua");

        m.insert("perl", "perl");
        m.insert("Perl", "perl");
        m.insert("pl", "perl");

        m.insert("awk", "awk");
        m.insert("AWK", "awk");
        m.insert("gawk", "awk");

        m.insert("dart", "dart");
        m.insert("Dart", "dart");

        m.insert("groovy", "groovy");
        m.insert("Groovy", "groovy");

        m.insert("objectivec", "objc");
        m.insert("ObjectiveC", "objc");
        m.insert("objc", "objc");
        m.insert("ObjC", "objc");

        // ---- Shell/Scripting ----
        m.insert("bash", "bash");
        m.insert("Bash", "bash");
        m.insert("sh", "bash");
        m.insert("shell", "bash");
        m.insert("Shell", "bash");
        m.insert("zsh", "bash");
        m.insert("fish", "bash");
        m.insert("ksh", "bash");

        m.insert("powershell", "powershell");
        m.insert("PowerShell", "powershell");
        m.insert("ps1", "powershell");
        m.insert("pwsh", "powershell");

        m.insert("batch", "batch");
        m.insert("Batch", "batch");
        m.insert("bat", "batch");
        m.insert("cmd", "batch");

        // ---- Markup and Data ----
        m.insert("html", "html");
        m.insert("HTML", "html");
        m.insert("html5", "html");
        m.insert("HTML5", "html");
        m.insert("xhtml", "html");

        m.insert("xml", "xml");
        m.insert("XML", "xml");
        m.insert("xsl", "xml");
        m.insert("xslt", "xml");

        m.insert("css", "css");
        m.insert("CSS", "css");
        m.insert("css3", "css");

        m.insert("scss", "scss");
        m.insert("SCSS", "scss");
        m.insert("sass", "sass");
        m.insert("SASS", "sass");
        m.insert("less", "less");
        m.insert("LESS", "less");

        m.insert("json", "json");
        m.insert("JSON", "json");
        m.insert("jsonc", "json");

        m.insert("yaml", "yaml");
        m.insert("YAML", "yaml");
        m.insert("yml", "yaml");

        m.insert("toml", "toml");
        m.insert("TOML", "toml");

        m.insert("ini", "ini");
        m.insert("INI", "ini");
        m.insert("cfg", "ini");

        m.insert("markdown", "markdown");
        m.insert("Markdown", "markdown");
        m.insert("md", "markdown");
        m.insert("MD", "markdown");

        m.insert("tex", "latex");
        m.insert("TeX", "latex");
        m.insert("latex", "latex");
        m.insert("LaTeX", "latex");

        m.insert("typst", "typst");
        m.insert("Typst", "typst");
        m.insert("typ", "typst");

        m.insert("graphql", "graphql");
        m.insert("GraphQL", "graphql");
        m.insert("gql", "graphql");

        // ---- Database ----
        m.insert("sql", "sql");
        m.insert("SQL", "sql");
        m.insert("mysql", "sql");
        m.insert("MySQL", "sql");
        m.insert("postgresql", "sql");
        m.insert("PostgreSQL", "sql");
        m.insert("postgres", "sql");
        m.insert("sqlite", "sql");
        m.insert("SQLite", "sql");
        m.insert("plsql", "sql");
        m.insert("PLSQL", "sql");
        m.insert("tsql", "sql");
        m.insert("TSQL", "sql");

        // ---- Build/Config ----
        m.insert("make", "makefile");
        m.insert("Makefile", "makefile");
        m.insert("makefile", "makefile");
        m.insert("gnumake", "makefile");

        m.insert("cmake", "cmake");
        m.insert("CMake", "cmake");

        m.insert("docker", "docker");
        m.insert("Docker", "docker");
        m.insert("dockerfile", "docker");
        m.insert("Dockerfile", "docker");

        m.insert("nginx", "nginx");
        m.insert("Nginx", "nginx");
        m.insert("apache", "apache");
        m.insert("Apache", "apache");

        m.insert("terraform", "terraform");
        m.insert("Terraform", "terraform");
        m.insert("tf", "terraform");

        // ---- Low-level/Systems ----
        m.insert("asm", "asm");
        m.insert("assembly", "asm");
        m.insert("Assembly", "asm");
        m.insert("nasm", "asm");
        m.insert("NASM", "asm");
        m.insert("masm", "asm");
        m.insert("x86", "asm");
        m.insert("x86asm", "asm");
        m.insert("arm", "asm");
        m.insert("ARM", "asm");
        m.insert("aarch64", "asm");

        m.insert("llvm", "llvm");
        m.insert("LLVM", "llvm");
        m.insert("llvmir", "llvm");

        m.insert("wasm", "wasm");
        m.insert("wat", "wasm");
        m.insert("WebAssembly", "wasm");

        // ---- GPU/Graphics ----
        m.insert("glsl", "glsl");
        m.insert("GLSL", "glsl");
        m.insert("hlsl", "hlsl");
        m.insert("HLSL", "hlsl");
        m.insert("cuda", "cuda");
        m.insert("CUDA", "cuda");
        m.insert("opencl", "opencl");
        m.insert("OpenCL", "opencl");

        // ---- Hardware Description ----
        m.insert("verilog", "verilog");
        m.insert("Verilog", "verilog");
        m.insert("vhdl", "vhdl");
        m.insert("VHDL", "vhdl");
        m.insert("systemverilog", "systemverilog");
        m.insert("SystemVerilog", "systemverilog");
        m.insert("sv", "systemverilog");

        // ---- Editor/Tools ----
        m.insert("vim", "vim");
        m.insert("vimscript", "vim");
        m.insert("VimL", "vim");
        m.insert("emacs", "elisp");
        m.insert("elisp", "elisp");
        m.insert("EmacsLisp", "elisp");

        // ---- Others ----
        m.insert("diff", "diff");
        m.insert("patch", "diff");
        m.insert("console", "console");
        m.insert("text", "text");
        m.insert("plain", "text");
        m.insert("plaintext", "text");
        m.insert("none", "text");
        m.insert("output", "text");

        m.insert("regex", "regex");
        m.insert("regexp", "regex");

        m.insert("protobuf", "protobuf");
        m.insert("proto", "protobuf");
        m.insert("proto3", "protobuf");

        m.insert("solidity", "solidity");
        m.insert("Solidity", "solidity");
        m.insert("sol", "solidity");

        m.insert("zig", "zig");
        m.insert("Zig", "zig");

        m.insert("nim", "nim");
        m.insert("Nim", "nim");

        m.insert("crystal", "crystal");
        m.insert("Crystal", "crystal");

        m.insert("v", "v");
        m.insert("vlang", "v");

        m
    };
}

// ============================================================================
// Theorem Environment Mappings
// ============================================================================

/// Theorem style enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TheoremStyle {
    /// Bold title, italic body (theorem, lemma, proposition, corollary)
    #[default]
    Plain,
    /// Bold title, normal body (definition, example, exercise)
    Definition,
    /// Italic title, normal body (remark, note, observation)
    Remark,
}

/// Theorem type information
#[derive(Debug, Clone)]
pub struct TheoremInfo {
    /// Display name in output
    pub display_name: &'static str,
    /// Style to use
    pub style: TheoremStyle,
}

lazy_static! {
    /// Comprehensive theorem environment mappings
    /// Maps LaTeX environment names to display names and styles
    pub static ref THEOREM_TYPES: HashMap<&'static str, TheoremInfo> = {
        let mut m = HashMap::new();

        // ---- Plain style (bold title, italic body) ----
        // Theorems
        m.insert("theorem", TheoremInfo { display_name: "Theorem", style: TheoremStyle::Plain });
        m.insert("thm", TheoremInfo { display_name: "Theorem", style: TheoremStyle::Plain });
        m.insert("Theorem", TheoremInfo { display_name: "Theorem", style: TheoremStyle::Plain });

        // Lemmas
        m.insert("lemma", TheoremInfo { display_name: "Lemma", style: TheoremStyle::Plain });
        m.insert("lem", TheoremInfo { display_name: "Lemma", style: TheoremStyle::Plain });
        m.insert("Lemma", TheoremInfo { display_name: "Lemma", style: TheoremStyle::Plain });

        // Propositions
        m.insert("proposition", TheoremInfo { display_name: "Proposition", style: TheoremStyle::Plain });
        m.insert("prop", TheoremInfo { display_name: "Proposition", style: TheoremStyle::Plain });
        m.insert("Proposition", TheoremInfo { display_name: "Proposition", style: TheoremStyle::Plain });

        // Corollaries
        m.insert("corollary", TheoremInfo { display_name: "Corollary", style: TheoremStyle::Plain });
        m.insert("cor", TheoremInfo { display_name: "Corollary", style: TheoremStyle::Plain });
        m.insert("Corollary", TheoremInfo { display_name: "Corollary", style: TheoremStyle::Plain });

        // Conjectures
        m.insert("conjecture", TheoremInfo { display_name: "Conjecture", style: TheoremStyle::Plain });
        m.insert("conj", TheoremInfo { display_name: "Conjecture", style: TheoremStyle::Plain });

        // Claims
        m.insert("claim", TheoremInfo { display_name: "Claim", style: TheoremStyle::Plain });

        // Facts
        m.insert("fact", TheoremInfo { display_name: "Fact", style: TheoremStyle::Plain });

        // Assumptions
        m.insert("assumption", TheoremInfo { display_name: "Assumption", style: TheoremStyle::Plain });

        // Hypotheses
        m.insert("hypothesis", TheoremInfo { display_name: "Hypothesis", style: TheoremStyle::Plain });

        // Axioms
        m.insert("axiom", TheoremInfo { display_name: "Axiom", style: TheoremStyle::Plain });
        m.insert("postulate", TheoremInfo { display_name: "Postulate", style: TheoremStyle::Plain });

        // Properties
        m.insert("property", TheoremInfo { display_name: "Property", style: TheoremStyle::Plain });

        // Criteria
        m.insert("criterion", TheoremInfo { display_name: "Criterion", style: TheoremStyle::Plain });

        // ---- Definition style (bold title, normal body) ----
        // Definitions
        m.insert("definition", TheoremInfo { display_name: "Definition", style: TheoremStyle::Definition });
        m.insert("defn", TheoremInfo { display_name: "Definition", style: TheoremStyle::Definition });
        m.insert("def", TheoremInfo { display_name: "Definition", style: TheoremStyle::Definition });
        m.insert("Definition", TheoremInfo { display_name: "Definition", style: TheoremStyle::Definition });

        // Examples
        m.insert("example", TheoremInfo { display_name: "Example", style: TheoremStyle::Definition });
        m.insert("ex", TheoremInfo { display_name: "Example", style: TheoremStyle::Definition });
        m.insert("examples", TheoremInfo { display_name: "Examples", style: TheoremStyle::Definition });
        m.insert("Example", TheoremInfo { display_name: "Example", style: TheoremStyle::Definition });

        // Exercises
        m.insert("exercise", TheoremInfo { display_name: "Exercise", style: TheoremStyle::Definition });
        m.insert("exer", TheoremInfo { display_name: "Exercise", style: TheoremStyle::Definition });

        // Problems
        m.insert("problem", TheoremInfo { display_name: "Problem", style: TheoremStyle::Definition });
        m.insert("prob", TheoremInfo { display_name: "Problem", style: TheoremStyle::Definition });

        // Questions
        m.insert("question", TheoremInfo { display_name: "Question", style: TheoremStyle::Definition });
        m.insert("ques", TheoremInfo { display_name: "Question", style: TheoremStyle::Definition });

        // Solutions
        m.insert("solution", TheoremInfo { display_name: "Solution", style: TheoremStyle::Definition });
        m.insert("sol", TheoremInfo { display_name: "Solution", style: TheoremStyle::Definition });

        // Algorithms
        m.insert("algorithm", TheoremInfo { display_name: "Algorithm", style: TheoremStyle::Definition });
        m.insert("alg", TheoremInfo { display_name: "Algorithm", style: TheoremStyle::Definition });

        // Notation
        m.insert("notation", TheoremInfo { display_name: "Notation", style: TheoremStyle::Definition });

        // ---- Remark style (italic title, normal body) ----
        // Remarks
        m.insert("remark", TheoremInfo { display_name: "Remark", style: TheoremStyle::Remark });
        m.insert("rem", TheoremInfo { display_name: "Remark", style: TheoremStyle::Remark });
        m.insert("remarks", TheoremInfo { display_name: "Remarks", style: TheoremStyle::Remark });
        m.insert("Remark", TheoremInfo { display_name: "Remark", style: TheoremStyle::Remark });

        // Notes
        m.insert("note", TheoremInfo { display_name: "Note", style: TheoremStyle::Remark });
        m.insert("notes", TheoremInfo { display_name: "Notes", style: TheoremStyle::Remark });

        // Observations
        m.insert("observation", TheoremInfo { display_name: "Observation", style: TheoremStyle::Remark });
        m.insert("obs", TheoremInfo { display_name: "Observation", style: TheoremStyle::Remark });

        // Cases
        m.insert("case", TheoremInfo { display_name: "Case", style: TheoremStyle::Remark });

        // Summary
        m.insert("summary", TheoremInfo { display_name: "Summary", style: TheoremStyle::Remark });

        // Conclusion
        m.insert("conclusion", TheoremInfo { display_name: "Conclusion", style: TheoremStyle::Remark });

        // ---- Proof (special handling) ----
        m.insert("proof", TheoremInfo { display_name: "Proof", style: TheoremStyle::Remark });

        m
    };
}

// ============================================================================
// Acronym/Glossary Support
// ============================================================================

/// An acronym definition
#[derive(Debug, Clone)]
pub struct AcronymDef {
    /// Short form (e.g., "API")
    pub short: String,
    /// Long form (e.g., "Application Programming Interface")
    pub long: String,
    /// Optional plural short form
    pub plural_short: Option<String>,
    /// Optional plural long form
    pub plural_long: Option<String>,
}

impl AcronymDef {
    pub fn new(short: &str, long: &str) -> Self {
        Self {
            short: short.to_string(),
            long: long.to_string(),
            plural_short: None,
            plural_long: None,
        }
    }

    /// Get plural form of short
    pub fn short_plural(&self) -> String {
        self.plural_short
            .clone()
            .unwrap_or_else(|| format!("{}s", self.short))
    }

    /// Get plural form of long
    pub fn long_plural(&self) -> String {
        self.plural_long
            .clone()
            .unwrap_or_else(|| format!("{}s", self.long))
    }

    /// Get full form: "Long Form (SF)"
    pub fn full(&self) -> String {
        format!("{} ({})", self.long, self.short)
    }

    /// Get plural full form
    pub fn full_plural(&self) -> String {
        format!("{} ({})", self.long_plural(), self.short_plural())
    }
}

/// A glossary entry definition
#[derive(Debug, Clone)]
pub struct GlossaryDef {
    /// Display name
    pub name: String,
    /// Description/definition
    pub description: String,
    /// Optional plural form
    pub plural: Option<String>,
}

impl GlossaryDef {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            plural: None,
        }
    }
}

// ============================================================================
// Code Block Options
// ============================================================================

/// Options for code blocks (extracted from lstlisting/minted options)
#[derive(Debug, Clone, Default)]
pub struct CodeBlockOptions {
    /// Programming language
    pub language: Option<String>,
    /// Caption text
    pub caption: Option<String>,
    /// Label for referencing
    pub label: Option<String>,
    /// Show line numbers
    pub line_numbers: bool,
    /// First line number
    pub first_line: Option<usize>,
    /// Last line number
    pub last_line: Option<usize>,
    /// Highlight specific lines
    pub highlight_lines: Vec<usize>,
}

impl CodeBlockOptions {
    /// Parse options from lstlisting/minted option string
    /// e.g., "language=Python, numbers=left, caption={My Code}"
    pub fn parse(options: &str) -> Self {
        let mut result = Self::default();

        for opt in options.split(',') {
            let opt = opt.trim();
            if let Some((key, value)) = opt.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_start_matches('{').trim_end_matches('}');

                match key {
                    "language" => {
                        // Map to Typst language name
                        if let Some(lang) = LANGUAGE_MAP.get(value) {
                            result.language = Some(lang.to_string());
                        } else {
                            result.language = Some(value.to_lowercase());
                        }
                    }
                    "caption" => result.caption = Some(value.to_string()),
                    "label" => result.label = Some(value.to_string()),
                    "numbers" => result.line_numbers = value == "left" || value == "right",
                    "linenos" | "showlines" => {
                        result.line_numbers = value == "true" || value.is_empty()
                    }
                    "firstnumber" | "firstline" => {
                        result.first_line = value.parse().ok();
                    }
                    "lastline" => {
                        result.last_line = value.parse().ok();
                    }
                    _ => {}
                }
            } else {
                // Handle flag-style options
                match opt {
                    "numbers" | "linenos" => result.line_numbers = true,
                    _ => {}
                }
            }
        }

        result
    }

    /// Get the Typst language identifier
    pub fn get_typst_language(&self) -> &str {
        self.language.as_deref().unwrap_or("")
    }
}

// ============================================================================
// Native Math Operators
// ============================================================================

lazy_static! {
    /// Math operators that are handled natively by the converter
    /// These should NOT be expanded via user macro definitions
    /// to avoid issues like \argmin -> \mathop{\rm argmin} -> problems
    pub static ref NATIVE_MATH_OPERATORS: std::collections::HashSet<&'static str> = {
        let mut s = std::collections::HashSet::new();
        // Argmin/Argmax variants
        s.insert("argmin");
        s.insert("argmax");
        s.insert("Argmin");
        s.insert("Argmax");
        // Add more as needed
        s
    };
}

/// Check if an operator is handled natively
pub fn is_native_math_operator(name: &str) -> bool {
    NATIVE_MATH_OPERATORS.contains(name)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_mapping() {
        assert_eq!(LANGUAGE_MAP.get("Python"), Some(&"python"));
        assert_eq!(LANGUAGE_MAP.get("py"), Some(&"python"));
        assert_eq!(LANGUAGE_MAP.get("C++"), Some(&"cpp"));
        assert_eq!(LANGUAGE_MAP.get("JavaScript"), Some(&"javascript"));
        assert_eq!(LANGUAGE_MAP.get("typescript"), Some(&"typescript"));
    }

    #[test]
    fn test_theorem_types() {
        let thm = THEOREM_TYPES.get("theorem").unwrap();
        assert_eq!(thm.display_name, "Theorem");
        assert_eq!(thm.style, TheoremStyle::Plain);

        let defn = THEOREM_TYPES.get("definition").unwrap();
        assert_eq!(defn.display_name, "Definition");
        assert_eq!(defn.style, TheoremStyle::Definition);

        let rem = THEOREM_TYPES.get("remark").unwrap();
        assert_eq!(rem.display_name, "Remark");
        assert_eq!(rem.style, TheoremStyle::Remark);
    }

    #[test]
    fn test_acronym_def() {
        let acr = AcronymDef::new("API", "Application Programming Interface");
        assert_eq!(acr.short, "API");
        assert_eq!(acr.long, "Application Programming Interface");
        assert_eq!(acr.short_plural(), "APIs");
        assert_eq!(acr.full(), "Application Programming Interface (API)");
    }

    #[test]
    fn test_native_operators() {
        assert!(NATIVE_MATH_OPERATORS.contains("argmin"));
        assert!(NATIVE_MATH_OPERATORS.contains("argmax"));
        assert!(!NATIVE_MATH_OPERATORS.contains("foobar"));
    }

    #[test]
    fn test_code_block_options_parse() {
        let opts = CodeBlockOptions::parse("language=Python, numbers=left, caption={Hello World}");
        assert_eq!(opts.language, Some("python".to_string()));
        assert!(opts.line_numbers);
        assert_eq!(opts.caption, Some("Hello World".to_string()));
    }
}
