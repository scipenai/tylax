//! Multi-file support for LaTeX ↔ Typst conversion
//!
//! This module provides WASM-safe abstractions for handling file includes:
//! - `\input{file}` and `\include{file}` in LaTeX
//! - `#import` and `#include` in Typst
//!
//! The key abstraction is the `FileResolver` trait which allows different
//! implementations for CLI (real filesystem) and WASM (no-op or memory-based).

use std::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

/// Trait for resolving and reading files
///
/// Implementations:
/// - `StdFileResolver`: Uses std::fs for real file system access (CLI)
/// - `MemoryFileResolver`: In-memory file storage (testing, WASM with preloaded files)
/// - `NoopFileResolver`: Returns empty/error for all reads (WASM fallback)
pub trait FileResolver: Send + Sync {
    /// Read a file's contents
    fn read_file(&self, path: &str) -> Result<String, FileResolveError>;

    /// Check if a file exists
    fn file_exists(&self, path: &str) -> bool;

    /// Resolve a relative path against a base path
    fn resolve_path(&self, base: &str, relative: &str) -> String;

    /// Get the base directory for includes
    fn base_dir(&self) -> Option<&str>;
}

/// Error type for file resolution
#[derive(Debug, Clone)]
pub enum FileResolveError {
    NotFound(String),
    ReadError(String),
    NotSupported(String),
}

impl std::fmt::Display for FileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileResolveError::NotFound(path) => write!(f, "File not found: {}", path),
            FileResolveError::ReadError(msg) => write!(f, "Read error: {}", msg),
            FileResolveError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
        }
    }
}

impl std::error::Error for FileResolveError {}

/// Standard filesystem resolver (for CLI usage)
#[cfg(not(target_arch = "wasm32"))]
pub struct StdFileResolver {
    base_directory: Option<PathBuf>,
    /// Search paths for includes (like TEXINPUTS)
    search_paths: Vec<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
impl StdFileResolver {
    pub fn new() -> Self {
        Self {
            base_directory: None,
            search_paths: vec![],
        }
    }

    pub fn with_base_dir(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_directory: Some(base_dir.as_ref().to_path_buf()),
            search_paths: vec![base_dir.as_ref().to_path_buf()],
        }
    }

    pub fn add_search_path(&mut self, path: impl AsRef<Path>) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }

    /// Try to find a file in search paths
    fn find_file(&self, filename: &str) -> Option<PathBuf> {
        // Try exact path first
        let path = Path::new(filename);
        if path.exists() {
            return Some(path.to_path_buf());
        }

        // Try with .tex extension
        let with_ext = format!("{}.tex", filename);
        let path_with_ext = Path::new(&with_ext);
        if path_with_ext.exists() {
            return Some(path_with_ext.to_path_buf());
        }

        // Search in search paths
        for search_path in &self.search_paths {
            let full_path = search_path.join(filename);
            if full_path.exists() {
                return Some(full_path);
            }

            let full_path_ext = search_path.join(&with_ext);
            if full_path_ext.exists() {
                return Some(full_path_ext);
            }
        }

        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for StdFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl FileResolver for StdFileResolver {
    fn read_file(&self, path: &str) -> Result<String, FileResolveError> {
        if let Some(full_path) = self.find_file(path) {
            std::fs::read_to_string(&full_path)
                .map_err(|e| FileResolveError::ReadError(e.to_string()))
        } else {
            Err(FileResolveError::NotFound(path.to_string()))
        }
    }

    fn file_exists(&self, path: &str) -> bool {
        self.find_file(path).is_some()
    }

    fn resolve_path(&self, base: &str, relative: &str) -> String {
        let base_path = Path::new(base);
        if let Some(parent) = base_path.parent() {
            parent.join(relative).to_string_lossy().to_string()
        } else {
            relative.to_string()
        }
    }

    fn base_dir(&self) -> Option<&str> {
        self.base_directory.as_ref().and_then(|p| p.to_str())
    }
}

/// Memory-based file resolver (for testing and WASM with preloaded files)
pub struct MemoryFileResolver {
    files: HashMap<String, String>,
    base_directory: Option<String>,
}

impl MemoryFileResolver {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            base_directory: None,
        }
    }

    pub fn with_base_dir(base_dir: &str) -> Self {
        Self {
            files: HashMap::new(),
            base_directory: Some(base_dir.to_string()),
        }
    }

    /// Add a file to the in-memory storage
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.files.insert(path.to_string(), content.to_string());
    }

    /// Add multiple files
    pub fn add_files(&mut self, files: impl IntoIterator<Item = (String, String)>) {
        for (path, content) in files {
            self.files.insert(path, content);
        }
    }
}

impl Default for MemoryFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl FileResolver for MemoryFileResolver {
    fn read_file(&self, path: &str) -> Result<String, FileResolveError> {
        self.files
            .get(path)
            .cloned()
            .or_else(|| {
                // Try with .tex extension
                self.files.get(&format!("{}.tex", path)).cloned()
            })
            .ok_or_else(|| FileResolveError::NotFound(path.to_string()))
    }

    fn file_exists(&self, path: &str) -> bool {
        self.files.contains_key(path) || self.files.contains_key(&format!("{}.tex", path))
    }

    fn resolve_path(&self, base: &str, relative: &str) -> String {
        // Simple path joining for memory resolver
        if relative.starts_with('/') || relative.starts_with('\\') {
            relative.to_string()
        } else if let Some(last_sep) = base.rfind(['/', '\\']) {
            format!("{}/{}", &base[..last_sep], relative)
        } else {
            relative.to_string()
        }
    }

    fn base_dir(&self) -> Option<&str> {
        self.base_directory.as_deref()
    }
}

/// No-op file resolver (for WASM when no files are available)
pub struct NoopFileResolver;

impl FileResolver for NoopFileResolver {
    fn read_file(&self, path: &str) -> Result<String, FileResolveError> {
        Err(FileResolveError::NotSupported(format!(
            "File reading not supported in this environment: {}",
            path
        )))
    }

    fn file_exists(&self, _path: &str) -> bool {
        false
    }

    fn resolve_path(&self, _base: &str, relative: &str) -> String {
        relative.to_string()
    }

    fn base_dir(&self) -> Option<&str> {
        None
    }
}

/// Represents a parsed include/input command
#[derive(Debug, Clone)]
pub enum IncludeCommand {
    /// LaTeX \input{file} - inserts content directly
    Input(String),
    /// LaTeX \include{file} - inserts with \clearpage before and after
    Include(String),
    /// LaTeX \subfile{file} - from subfiles package
    Subfile(String),
    /// Typst #import "file"
    Import(String),
    /// Typst #include "file"
    TypstInclude(String),
}

impl IncludeCommand {
    /// Get the file path
    pub fn path(&self) -> &str {
        match self {
            IncludeCommand::Input(p)
            | IncludeCommand::Include(p)
            | IncludeCommand::Subfile(p)
            | IncludeCommand::Import(p)
            | IncludeCommand::TypstInclude(p) => p,
        }
    }
}

/// Parse LaTeX content for include commands
pub fn find_latex_includes(content: &str) -> Vec<(usize, usize, IncludeCommand)> {
    let mut includes = Vec::new();

    // Helper to find includes for a specific command
    fn find_cmd_includes(
        content: &str,
        cmd: &str,
        includes: &mut Vec<(usize, usize, IncludeCommand)>,
        cmd_type: &str,
    ) {
        let mut search_start = 0;
        while let Some(pos) = content[search_start..].find(cmd) {
            let abs_pos = search_start + pos;
            let after_cmd = &content[abs_pos + cmd.len()..];

            if let Some(end) = after_cmd.find('}') {
                let path = after_cmd[..end].to_string();
                let full_end = abs_pos + cmd.len() + end + 1;
                let inc_cmd = match cmd_type {
                    "input" => IncludeCommand::Input(path),
                    "include" => IncludeCommand::Include(path),
                    "subfile" => IncludeCommand::Subfile(path),
                    _ => IncludeCommand::Input(path),
                };
                includes.push((abs_pos, full_end, inc_cmd));
            }

            search_start = abs_pos + cmd.len();
        }
    }

    find_cmd_includes(content, "\\input{", &mut includes, "input");
    find_cmd_includes(content, "\\include{", &mut includes, "include");
    find_cmd_includes(content, "\\subfile{", &mut includes, "subfile");

    // Sort by position
    includes.sort_by_key(|(pos, _, _)| *pos);

    includes
}

/// Parse Typst content for include commands
pub fn find_typst_includes(content: &str) -> Vec<(usize, usize, IncludeCommand)> {
    let mut includes = Vec::new();

    // #import "file"
    let mut search_start = 0;
    while let Some(pos) = content[search_start..].find("#import") {
        let abs_pos = search_start + pos;
        let after = &content[abs_pos + "#import".len()..];
        let after = after.trim_start();

        if after.starts_with('"') {
            if let Some((path, end_pos)) = parse_quoted_string_with_pos(after) {
                let full_end = abs_pos
                    + "#import".len()
                    + (content[abs_pos + "#import".len()..].len() - after.len())
                    + end_pos;
                includes.push((abs_pos, full_end, IncludeCommand::Import(path)));
            }
        }

        search_start = abs_pos + "#import".len();
    }

    // #include "file"
    search_start = 0;
    while let Some(pos) = content[search_start..].find("#include") {
        let abs_pos = search_start + pos;
        let after = &content[abs_pos + "#include".len()..];
        let after = after.trim_start();

        if after.starts_with('"') {
            if let Some((path, end_pos)) = parse_quoted_string_with_pos(after) {
                let full_end = abs_pos
                    + "#include".len()
                    + (content[abs_pos + "#include".len()..].len() - after.len())
                    + end_pos;
                includes.push((abs_pos, full_end, IncludeCommand::TypstInclude(path)));
            }
        }

        search_start = abs_pos + "#include".len();
    }

    includes.sort_by_key(|(pos, _, _)| *pos);

    includes
}

/// Parse quoted string and return content and end position
fn parse_quoted_string_with_pos(s: &str) -> Option<(String, usize)> {
    if !s.starts_with('"') {
        return None;
    }

    let mut escaped = false;
    let mut content = String::new();

    for (i, c) in s[1..].char_indices() {
        if escaped {
            content.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            return Some((content, i + 2)); // +1 for starting quote, +1 for current char
        }
        content.push(c);
    }

    None
}

/// Process a document with includes, recursively resolving them
pub fn process_includes<R: FileResolver>(
    content: &str,
    current_file: &str,
    resolver: &R,
    max_depth: usize,
) -> Result<String, FileResolveError> {
    if max_depth == 0 {
        return Ok(content.to_string());
    }

    let includes = find_latex_includes(content);

    if includes.is_empty() {
        return Ok(content.to_string());
    }

    let mut result = String::new();
    let mut last_end = 0;

    for (start, end, cmd) in includes {
        // Add content before this include
        result.push_str(&content[last_end..start]);

        // Resolve the included file
        let include_path = resolver.resolve_path(current_file, cmd.path());

        match resolver.read_file(&include_path) {
            Ok(included_content) => {
                // Add clearpage for \include
                if matches!(cmd, IncludeCommand::Include(_)) {
                    result.push_str("\\clearpage\n");
                }

                // Recursively process includes
                let processed =
                    process_includes(&included_content, &include_path, resolver, max_depth - 1)?;
                result.push_str(&processed);

                if matches!(cmd, IncludeCommand::Include(_)) {
                    result.push_str("\n\\clearpage");
                }
            }
            Err(_) => {
                // Leave a comment for unresolved includes
                result.push_str(&format!("% Could not resolve: {}\n", cmd.path()));
            }
        }

        last_end = end;
    }

    // Add remaining content
    result.push_str(&content[last_end..]);

    Ok(result)
}

/// Generate a fallback comment for WASM when includes are detected
pub fn generate_include_fallback(content: &str) -> String {
    let includes = find_latex_includes(content);

    if includes.is_empty() {
        return content.to_string();
    }

    let mut result = String::new();
    let mut last_end = 0;

    for (start, end, cmd) in includes {
        result.push_str(&content[last_end..start]);

        // Generate appropriate fallback
        match cmd {
            IncludeCommand::Input(path)
            | IncludeCommand::Include(path)
            | IncludeCommand::Subfile(path) => {
                result.push_str(&format!(
                    "% [Include: {}] (not available in web mode)\n",
                    path
                ));
            }
            _ => {}
        }

        last_end = end;
    }

    result.push_str(&content[last_end..]);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_resolver() {
        let mut resolver = MemoryFileResolver::new();
        resolver.add_file("test.tex", "Hello, world!");

        assert!(resolver.file_exists("test.tex"));
        assert_eq!(resolver.read_file("test.tex").unwrap(), "Hello, world!");
    }

    #[test]
    fn test_memory_resolver_tex_extension() {
        let mut resolver = MemoryFileResolver::new();
        resolver.add_file("chapter1.tex", "Chapter 1 content");

        // Should find with or without extension
        assert!(resolver.file_exists("chapter1.tex"));
        assert_eq!(resolver.read_file("chapter1").unwrap(), "Chapter 1 content");
    }

    #[test]
    fn test_find_latex_includes() {
        let content = r#"
\documentclass{article}
\begin{document}
\input{chapter1}
\include{chapter2}
\end{document}
"#;
        let includes = find_latex_includes(content);

        assert_eq!(includes.len(), 2);
        assert!(matches!(includes[0].2, IncludeCommand::Input(ref p) if p == "chapter1"));
        assert!(matches!(includes[1].2, IncludeCommand::Include(ref p) if p == "chapter2"));
    }

    #[test]
    fn test_process_includes() {
        let mut resolver = MemoryFileResolver::new();
        resolver.add_file("main.tex", r#"\input{sub}"#);
        resolver.add_file("sub.tex", "Included content");

        let result = process_includes(
            resolver.read_file("main.tex").unwrap().as_str(),
            "main.tex",
            &resolver,
            5,
        )
        .unwrap();

        assert!(result.contains("Included content"));
    }

    #[test]
    fn test_nested_includes() {
        let mut resolver = MemoryFileResolver::new();
        resolver.add_file("main.tex", r#"\input{level1}"#);
        resolver.add_file("level1.tex", r#"Level 1 \input{level2}"#);
        resolver.add_file("level2.tex", "Level 2");

        let result = process_includes(
            resolver.read_file("main.tex").unwrap().as_str(),
            "main.tex",
            &resolver,
            5,
        )
        .unwrap();

        assert!(result.contains("Level 1"));
        assert!(result.contains("Level 2"));
    }

    #[test]
    fn test_noop_resolver() {
        let resolver = NoopFileResolver;

        assert!(!resolver.file_exists("any.tex"));
        assert!(resolver.read_file("any.tex").is_err());
    }

    #[test]
    fn test_generate_include_fallback() {
        let content = r#"\input{chapter1}
Some text
\include{chapter2}"#;

        let result = generate_include_fallback(content);

        assert!(result.contains("[Include: chapter1]"));
        assert!(result.contains("[Include: chapter2]"));
        assert!(result.contains("Some text"));
    }

    #[test]
    fn test_find_typst_includes() {
        let content = r#"
#import "utils.typ"
Some content
#include "chapter.typ"
"#;
        let includes = find_typst_includes(content);

        assert_eq!(includes.len(), 2);
        assert!(matches!(includes[0].2, IncludeCommand::Import(ref p) if p == "utils.typ"));
        assert!(matches!(includes[1].2, IncludeCommand::TypstInclude(ref p) if p == "chapter.typ"));
    }
}
