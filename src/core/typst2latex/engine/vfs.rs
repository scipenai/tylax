//! Virtual File System trait for module loading.
//!
//! This module provides an abstraction for file system operations,
//! allowing the evaluator to work with different file sources
//! (real files, in-memory content, bundled modules, etc.)

use std::collections::HashMap;
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Component, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_IMPORT_FILE_BYTES: usize = 1024 * 1024;

/// Derive the default module identifier bound by a bare `#import "path"`.
pub(crate) fn default_import_binding_name(source: &str) -> Option<&str> {
    if source.starts_with('@') {
        let name = source.split('/').nth(1)?.split(':').next()?;
        (!name.is_empty()).then_some(name)
    } else {
        let file = source.rsplit(['/', '\\']).next()?;
        let stem = file.strip_suffix(".typ").unwrap_or(file);
        (!stem.is_empty()).then_some(stem)
    }
}

/// Result type for VFS operations.
pub type VfsResult<T> = Result<T, VfsError>;

/// Error type for VFS operations.
#[derive(Debug, Clone)]
pub enum VfsError {
    /// File not found.
    NotFound(String),
    /// Read error.
    ReadError(String),
    /// Invalid path.
    InvalidPath(String),
    /// Permission denied.
    PermissionDenied(String),
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsError::NotFound(path) => write!(f, "file not found: {}", path),
            VfsError::ReadError(msg) => write!(f, "read error: {}", msg),
            VfsError::InvalidPath(path) => write!(f, "invalid path: {}", path),
            VfsError::PermissionDenied(path) => write!(f, "permission denied: {}", path),
        }
    }
}

impl std::error::Error for VfsError {}

/// Virtual File System trait.
///
/// Implementations of this trait provide file system operations
/// for loading modules and other resources.
pub trait VirtualFileSystem: Send + Sync {
    /// Read a file as text.
    fn read_text(&self, path: &str) -> VfsResult<String>;

    /// Read a file as bytes.
    fn read_bytes(&self, path: &str) -> VfsResult<Vec<u8>>;

    /// Check if a file exists.
    fn exists(&self, path: &str) -> bool;

    /// Resolve a relative path against a base path.
    fn resolve(&self, base: &str, relative: &str) -> VfsResult<String>;

    /// Get the current working directory.
    fn cwd(&self) -> VfsResult<String>;
}

/// A no-op VFS that always returns errors.
///
/// Useful for sandboxed environments where file access is not allowed.
pub struct NoopVfs;

impl VirtualFileSystem for NoopVfs {
    fn read_text(&self, path: &str) -> VfsResult<String> {
        Err(VfsError::NotFound(path.to_string()))
    }

    fn read_bytes(&self, path: &str) -> VfsResult<Vec<u8>> {
        Err(VfsError::NotFound(path.to_string()))
    }

    fn exists(&self, _path: &str) -> bool {
        false
    }

    fn resolve(&self, _base: &str, relative: &str) -> VfsResult<String> {
        Ok(relative.to_string())
    }

    fn cwd(&self) -> VfsResult<String> {
        Ok(".".to_string())
    }
}

/// An in-memory VFS for testing and bundled content.
#[derive(Debug, Clone, Default)]
pub struct MemoryVfs {
    files: HashMap<String, Vec<u8>>,
    cwd: String,
}

impl MemoryVfs {
    /// Create a new empty memory VFS.
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            cwd: ".".to_string(),
        }
    }

    /// Add a file to the VFS.
    pub fn add_file(&mut self, path: impl Into<String>, content: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), content.into());
    }

    /// Add a text file to the VFS.
    pub fn add_text_file(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.add_file(path, content.into().into_bytes());
    }

    /// Set the current working directory.
    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = cwd.into();
    }

    fn lookup(&self, path: &str) -> Option<&Vec<u8>> {
        self.files
            .get(path)
            .or_else(|| path.strip_prefix('/').and_then(|rel| self.files.get(rel)))
    }
}

impl VirtualFileSystem for MemoryVfs {
    fn read_text(&self, path: &str) -> VfsResult<String> {
        self.lookup(path)
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
            .ok_or_else(|| VfsError::NotFound(path.to_string()))
    }

    fn read_bytes(&self, path: &str) -> VfsResult<Vec<u8>> {
        self.lookup(path)
            .cloned()
            .ok_or_else(|| VfsError::NotFound(path.to_string()))
    }

    fn exists(&self, path: &str) -> bool {
        self.lookup(path).is_some()
    }

    fn resolve(&self, base: &str, relative: &str) -> VfsResult<String> {
        // Simple path resolution
        if relative.starts_with('/') || relative.starts_with("@") {
            return Ok(relative.to_string());
        }

        let base_path = Path::new(base);
        let parent = base_path.parent().unwrap_or(Path::new(""));
        let resolved = parent.join(relative);

        // Normalize the path
        let mut components: Vec<&str> = Vec::new();
        for component in resolved.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::CurDir => {}
                std::path::Component::Normal(s) => {
                    if let Some(s) = s.to_str() {
                        components.push(s);
                    }
                }
                std::path::Component::RootDir => {
                    components.clear();
                    components.push("");
                }
                std::path::Component::Prefix(_) => {}
            }
        }

        Ok(components.join("/"))
    }

    fn cwd(&self) -> VfsResult<String> {
        Ok(self.cwd.clone())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_relative_components(
    rel: &mut Vec<String>,
    path: &Path,
    allow_root_dir: bool,
    original: &str,
) -> VfsResult<()> {
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let s = part
                    .to_str()
                    .ok_or_else(|| VfsError::InvalidPath(original.to_string()))?;
                rel.push(s.to_string());
            }
            Component::CurDir => {}
            Component::RootDir if allow_root_dir => {}
            Component::RootDir | Component::Prefix(_) => {
                return Err(VfsError::PermissionDenied(original.to_string()));
            }
            Component::ParentDir => {
                if rel.pop().is_none() {
                    return Err(VfsError::PermissionDenied(original.to_string()));
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_virtual_parts(base: Option<&str>, relative: &str) -> VfsResult<Vec<String>> {
    if relative.is_empty() || relative.starts_with('@') {
        return Err(VfsError::NotFound(relative.to_string()));
    }

    let mut parts = Vec::new();
    let (path_to_walk, allow_root_dir) = if let Some(root_rel) = relative.strip_prefix('/') {
        (root_rel, true)
    } else {
        if let Some(base_path) = base.filter(|b| !b.is_empty()) {
            let parent = Path::new(base_path).parent().unwrap_or(Path::new(""));
            push_relative_components(&mut parts, parent, false, relative)?;
        }
        (relative, false)
    };

    push_relative_components(
        &mut parts,
        Path::new(path_to_walk),
        allow_root_dir,
        relative,
    )?;
    if parts.is_empty() {
        return Err(VfsError::InvalidPath(relative.to_string()));
    }
    Ok(parts)
}

/// A real file system VFS.
#[cfg(not(target_arch = "wasm32"))]
pub struct RealVfs {
    root: PathBuf,
    canon_root: Option<PathBuf>,
    max_file_bytes: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl RealVfs {
    /// Create a new real VFS rooted at the given path.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let canon_root = std::fs::canonicalize(&root).ok().filter(|p| p.is_dir());
        Self {
            root,
            canon_root,
            max_file_bytes: MAX_IMPORT_FILE_BYTES,
        }
    }

    /// Build a sandboxed VFS and logical entry virtual path for a source file,
    /// honoring `TYPST_ROOT` and resolving entry directory links cleanly.
    pub(crate) fn for_source_file(source_path: &Path) -> Option<(Self, String)> {
        let source_dir = std::fs::canonicalize(source_path)
            .ok()
            .and_then(|c| c.parent().map(Path::to_path_buf))
            .or_else(|| source_path.parent().map(Path::to_path_buf));
        let effective_source = match source_dir.as_deref() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };

        let (project_root, canon_root, rel_dir) = match std::env::var_os("TYPST_ROOT") {
            Some(typst_root) if !typst_root.is_empty() => {
                let canon_root = std::fs::canonicalize(typst_root).ok()?;
                if !canon_root.is_dir() {
                    return None;
                }
                let canon_source = std::fs::canonicalize(effective_source).ok()?;
                let rel = canon_source.strip_prefix(&canon_root).ok()?.to_path_buf();
                (canon_root.clone(), Some(canon_root), rel)
            }
            _ => {
                let canon = std::fs::canonicalize(effective_source)
                    .ok()
                    .filter(|p| p.is_dir());
                let root = canon
                    .clone()
                    .unwrap_or_else(|| effective_source.to_path_buf());
                (root, canon, PathBuf::new())
            }
        };

        let mut virtual_parts = Vec::new();
        for component in rel_dir.components() {
            if let Component::Normal(part) = component {
                virtual_parts.push(part.to_str()?.to_string());
            }
        }
        let leaf = source_path
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("__entry__.typ");
        virtual_parts.push(leaf.to_string());

        Some((
            Self {
                root: project_root,
                canon_root,
                max_file_bytes: MAX_IMPORT_FILE_BYTES,
            },
            virtual_parts.join("/"),
        ))
    }

    fn resolve_checked_path(&self, path: &str) -> VfsResult<PathBuf> {
        let parts = normalize_virtual_parts(None, path)?;
        let Some(canon_root) = &self.canon_root else {
            return Err(VfsError::NotFound(path.to_string()));
        };
        let rel: PathBuf = parts.iter().map(String::as_str).collect();
        let candidate = canon_root.join(&rel);
        let canon_candidate = std::fs::canonicalize(&candidate).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VfsError::NotFound(path.to_string())
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                VfsError::PermissionDenied(path.to_string())
            } else {
                VfsError::ReadError(e.to_string())
            }
        })?;
        if !canon_candidate.starts_with(canon_root) {
            return Err(VfsError::PermissionDenied(path.to_string()));
        }
        Ok(canon_candidate)
    }

    fn read_bounded_bytes(&self, path: &str) -> VfsResult<Vec<u8>> {
        use std::io::Read;

        let canon_candidate = self.resolve_checked_path(path)?;
        let metadata = std::fs::metadata(&canon_candidate).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VfsError::NotFound(path.to_string())
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                VfsError::PermissionDenied(path.to_string())
            } else {
                VfsError::ReadError(e.to_string())
            }
        })?;
        if !metadata.is_file() {
            return Err(VfsError::ReadError(format!("not a regular file: {}", path)));
        }
        if metadata.len() > self.max_file_bytes as u64 {
            return Err(VfsError::ReadError(format!(
                "file exceeds maximum import size: {}",
                path
            )));
        }

        let file = std::fs::File::open(&canon_candidate).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VfsError::NotFound(path.to_string())
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                VfsError::PermissionDenied(path.to_string())
            } else {
                VfsError::ReadError(e.to_string())
            }
        })?;
        let opened_meta = file
            .metadata()
            .map_err(|e| VfsError::ReadError(e.to_string()))?;
        if !opened_meta.is_file() {
            return Err(VfsError::ReadError(format!("not a regular file: {}", path)));
        }

        let mut buf = Vec::new();
        file.take((self.max_file_bytes as u64).saturating_add(1))
            .read_to_end(&mut buf)
            .map_err(|e| VfsError::ReadError(e.to_string()))?;
        if buf.len() > self.max_file_bytes {
            return Err(VfsError::ReadError(format!(
                "file exceeds maximum import size: {}",
                path
            )));
        }
        Ok(buf)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl VirtualFileSystem for RealVfs {
    fn read_text(&self, path: &str) -> VfsResult<String> {
        let bytes = self.read_bounded_bytes(path)?;
        String::from_utf8(bytes).map_err(|e| VfsError::ReadError(e.to_string()))
    }

    fn read_bytes(&self, path: &str) -> VfsResult<Vec<u8>> {
        self.read_bounded_bytes(path)
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve_checked_path(path).is_ok_and(|p| p.is_file())
    }

    fn resolve(&self, base: &str, relative: &str) -> VfsResult<String> {
        Ok(normalize_virtual_parts(Some(base), relative)?.join("/"))
    }

    fn cwd(&self) -> VfsResult<String> {
        self.root
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| VfsError::InvalidPath("cwd".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_vfs() {
        let mut vfs = MemoryVfs::new();
        vfs.add_text_file("test.typ", "#let x = 1");
        vfs.add_text_file("lib/utils.typ", "#let add(a, b) = a + b");

        assert!(vfs.exists("test.typ"));
        assert!(vfs.exists("lib/utils.typ"));
        assert!(!vfs.exists("nonexistent.typ"));

        assert_eq!(vfs.read_text("test.typ").unwrap(), "#let x = 1");
    }

    #[test]
    fn test_memory_vfs_resolve() {
        let vfs = MemoryVfs::new();

        // Resolve relative to base
        assert_eq!(
            vfs.resolve("src/main.typ", "utils.typ").unwrap(),
            "src/utils.typ"
        );

        // Resolve parent reference
        assert_eq!(
            vfs.resolve("src/lib/main.typ", "../utils.typ").unwrap(),
            "src/utils.typ"
        );

        // Absolute paths are returned as-is
        assert_eq!(
            vfs.resolve("src/main.typ", "/root.typ").unwrap(),
            "/root.typ"
        );
    }

    #[test]
    fn test_noop_vfs() {
        let vfs = NoopVfs;

        assert!(!vfs.exists("anything.typ"));
        assert!(vfs.read_text("anything.typ").is_err());
    }

    #[test]
    fn test_memory_vfs_root_relative_lookup() {
        let mut vfs = MemoryVfs::new();
        vfs.add_text_file("provider.typ", "#let x = 1");

        assert!(vfs.exists("/provider.typ"));
        assert_eq!(vfs.read_text("/provider.typ").unwrap(), "#let x = 1");
        assert_eq!(
            default_import_binding_name("../dir/terms.typ"),
            Some("terms")
        );
        assert_eq!(
            default_import_binding_name("@preview/cetz:0.3.1"),
            Some("cetz")
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_real_vfs_resolves_root_relative_and_blocks_escapes() {
        let outer =
            std::env::temp_dir().join(format!("tylax_real_vfs_unit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outer);
        let project = outer.join("project");
        let sub = project.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(outer.join("outside.typ"), "#let leak = 1\n").unwrap();
        std::fs::write(project.join("root.typ"), "#let ok = 1\n").unwrap();
        std::fs::write(sub.join("mod.typ"), "#let m = 1\n").unwrap();

        let vfs = RealVfs::new(&project);
        assert_eq!(vfs.resolve("sub/mod.typ", "/root.typ").unwrap(), "root.typ");
        assert_eq!(
            vfs.resolve("sub/mod.typ", "../root.typ").unwrap(),
            "root.typ"
        );
        assert!(matches!(
            vfs.resolve("sub/mod.typ", "../../outside.typ"),
            Err(VfsError::PermissionDenied(_))
        ));
        assert!(matches!(
            vfs.read_text("../outside.typ"),
            Err(VfsError::PermissionDenied(_))
        ));
        assert!(matches!(vfs.read_text("sub"), Err(VfsError::ReadError(_))));
        assert!(matches!(
            vfs.read_text("missing.typ"),
            Err(VfsError::NotFound(_))
        ));
        assert_eq!(vfs.read_text("/root.typ").unwrap(), "#let ok = 1\n");

        let _ = std::fs::remove_dir_all(&outer);
    }
}
