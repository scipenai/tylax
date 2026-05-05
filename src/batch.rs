//! Filesystem batch conversion API.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{
    latex_document_to_typst_with_options, latex_to_typst_with_options, typst_to_latex_with_options,
    L2TOptions, T2LOptions,
};

/// Batch conversion direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatchDirection {
    /// Infer direction from each source file extension.
    Auto,
    /// Convert `.tex` files to `.typ`.
    #[default]
    LatexToTypst,
    /// Convert `.typ` files to `.tex`.
    TypstToLatex,
}

/// Filesystem batch conversion options.
#[derive(Debug, Clone)]
pub struct BatchOptions {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub direction: BatchDirection,
    pub recursive: bool,
    pub full_document: bool,
    pub output_extension: Option<String>,
    pub excludes: Vec<String>,
    pub l2t_options: L2TOptions,
    pub t2l_options: T2LOptions,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            output_dir: PathBuf::new(),
            direction: BatchDirection::default(),
            recursive: false,
            full_document: false,
            output_extension: None,
            excludes: Vec::new(),
            l2t_options: L2TOptions::default(),
            t2l_options: T2LOptions::default(),
        }
    }
}

/// Summary of a batch conversion run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReport {
    pub results: Vec<BatchFileResult>,
    pub success_count: usize,
    pub error_count: usize,
}

/// Result for one planned input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchFileResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub direction: BatchDirection,
    pub status: BatchFileStatus,
}

/// Per-file conversion status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchFileStatus {
    Converted,
    Failed(String),
}

/// Errors that abort the whole batch before per-file conversion starts.
#[derive(Debug)]
pub enum BatchError {
    InvalidInput {
        path: PathBuf,
    },
    InvalidExclude {
        pattern: String,
        message: String,
    },
    CreateOutputRoot {
        path: PathBuf,
        source: io::Error,
    },
    Discover {
        path: PathBuf,
        source: io::Error,
    },
    OutputCollision {
        output_path: PathBuf,
        first_input: PathBuf,
        second_input: PathBuf,
    },
}

impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatchError::InvalidInput { path } => {
                write!(f, "input path does not exist: {}", path.display())
            }
            BatchError::InvalidExclude { pattern, message } => {
                write!(f, "invalid exclude glob '{}': {}", pattern, message)
            }
            BatchError::CreateOutputRoot { path, source } => {
                write!(
                    f,
                    "failed to create output directory '{}': {}",
                    path.display(),
                    source
                )
            }
            BatchError::Discover { path, source } => {
                write!(
                    f,
                    "failed to read directory '{}': {}",
                    path.display(),
                    source
                )
            }
            BatchError::OutputCollision {
                output_path,
                first_input,
                second_input,
            } => write!(
                f,
                "multiple inputs map to '{}': '{}' and '{}'",
                output_path.display(),
                first_input.display(),
                second_input.display()
            ),
        }
    }
}

impl std::error::Error for BatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BatchError::CreateOutputRoot { source, .. } | BatchError::Discover { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct PlannedFile {
    input_path: PathBuf,
    output_path: PathBuf,
    direction: BatchDirection,
}

#[derive(Debug)]
struct DiscoverContext<'a> {
    options: &'a BatchOptions,
    excludes: &'a GlobSet,
    output_root: Option<PathBuf>,
}

/// Convert a file or directory tree according to [`BatchOptions`].
pub fn convert_batch(options: &BatchOptions) -> Result<BatchReport, BatchError> {
    if !options.input.exists() {
        return Err(BatchError::InvalidInput {
            path: options.input.clone(),
        });
    }

    let excludes = build_excludes(&options.excludes)?;
    fs::create_dir_all(&options.output_dir).map_err(|source| BatchError::CreateOutputRoot {
        path: options.output_dir.clone(),
        source,
    })?;

    let plans = discover_plans(options, &excludes)?;
    check_collisions(&plans)?;

    let mut results = Vec::with_capacity(plans.len());
    let mut success_count = 0;
    let mut error_count = 0;

    for plan in plans {
        let status = match convert_one(&plan, options) {
            Ok(()) => {
                success_count += 1;
                BatchFileStatus::Converted
            }
            Err(err) => {
                error_count += 1;
                BatchFileStatus::Failed(err.to_string())
            }
        };

        results.push(BatchFileResult {
            input_path: plan.input_path,
            output_path: plan.output_path,
            direction: plan.direction,
            status,
        });
    }

    Ok(BatchReport {
        results,
        success_count,
        error_count,
    })
}

fn build_excludes(patterns: &[String]) -> Result<GlobSet, BatchError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|err| BatchError::InvalidExclude {
            pattern: pattern.clone(),
            message: err.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|err| BatchError::InvalidExclude {
        pattern: patterns.join(", "),
        message: err.to_string(),
    })
}

fn discover_plans(
    options: &BatchOptions,
    excludes: &GlobSet,
) -> Result<Vec<PlannedFile>, BatchError> {
    let output_root = options.output_dir.canonicalize().ok();
    let ctx = DiscoverContext {
        options,
        excludes,
        output_root,
    };
    let input = &options.input;
    if input.is_file() {
        let relative = input
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("output"));
        if matches_exclude(excludes, &relative) {
            return Ok(Vec::new());
        }
        return Ok(plan_file(options, input, &relative).into_iter().collect());
    }

    if !input.is_dir() {
        return Err(BatchError::InvalidInput {
            path: input.clone(),
        });
    }

    let mut plans = Vec::new();
    discover_dir(input, input, &ctx, &mut plans)?;
    plans.sort_by(|a, b| a.input_path.cmp(&b.input_path));
    Ok(plans)
}

fn discover_dir(
    root: &Path,
    dir: &Path,
    ctx: &DiscoverContext<'_>,
    plans: &mut Vec<PlannedFile>,
) -> Result<(), BatchError> {
    let entries = fs::read_dir(dir).map_err(|source| BatchError::Discover {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| BatchError::Discover {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| BatchError::Discover {
            path: path.clone(),
            source,
        })?;
        let relative = path
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());

        if matches_exclude(ctx.excludes, &relative) {
            continue;
        }

        if file_type.is_dir() {
            if is_output_root(&path, ctx.output_root.as_deref()) {
                continue;
            }
            if ctx.options.recursive {
                discover_dir(root, &path, ctx, plans)?;
            }
        } else if file_type.is_file() {
            if let Some(plan) = plan_file(ctx.options, &path, &relative) {
                plans.push(plan);
            }
        }
    }

    Ok(())
}

fn is_output_root(path: &Path, output_root: Option<&Path>) -> bool {
    output_root
        .and_then(|root| path.canonicalize().ok().map(|path| path == root))
        .unwrap_or(false)
}

fn matches_exclude(excludes: &GlobSet, relative: &Path) -> bool {
    let normalized = normalize_relative_path(relative);
    excludes.is_match(&normalized)
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn plan_file(options: &BatchOptions, input_path: &Path, relative: &Path) -> Option<PlannedFile> {
    let direction = resolve_direction(options.direction, input_path)?;
    let extension = options
        .output_extension
        .as_deref()
        .unwrap_or_else(|| default_output_extension(direction))
        .trim_start_matches('.');
    let output_relative = relative.with_extension(extension);
    let output_path = options.output_dir.join(output_relative);

    Some(PlannedFile {
        input_path: input_path.to_path_buf(),
        output_path,
        direction,
    })
}

fn resolve_direction(direction: BatchDirection, input_path: &Path) -> Option<BatchDirection> {
    let extension = input_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match direction {
        BatchDirection::Auto => match extension.as_deref() {
            Some("tex") => Some(BatchDirection::LatexToTypst),
            Some("typ") => Some(BatchDirection::TypstToLatex),
            _ => None,
        },
        BatchDirection::LatexToTypst if extension.as_deref() == Some("tex") => {
            Some(BatchDirection::LatexToTypst)
        }
        BatchDirection::TypstToLatex if extension.as_deref() == Some("typ") => {
            Some(BatchDirection::TypstToLatex)
        }
        _ => None,
    }
}

fn default_output_extension(direction: BatchDirection) -> &'static str {
    match direction {
        BatchDirection::LatexToTypst => "typ",
        BatchDirection::TypstToLatex => "tex",
        BatchDirection::Auto => unreachable!("auto direction must be resolved before output"),
    }
}

fn check_collisions(plans: &[PlannedFile]) -> Result<(), BatchError> {
    let mut seen: HashMap<PathBuf, &PathBuf> = HashMap::new();
    for plan in plans {
        if let Some(first_input) = seen.insert(plan.output_path.clone(), &plan.input_path) {
            return Err(BatchError::OutputCollision {
                output_path: plan.output_path.clone(),
                first_input: first_input.clone(),
                second_input: plan.input_path.clone(),
            });
        }
    }
    Ok(())
}

fn convert_one(plan: &PlannedFile, options: &BatchOptions) -> io::Result<()> {
    let content = fs::read_to_string(&plan.input_path)?;
    let converted = match plan.direction {
        BatchDirection::LatexToTypst => {
            if options.full_document {
                latex_document_to_typst_with_options(&content, &options.l2t_options)
            } else {
                latex_to_typst_with_options(&content, &options.l2t_options)
            }
        }
        BatchDirection::TypstToLatex => {
            let mut t2l_options = options.t2l_options.clone();
            t2l_options.full_document = options.full_document;
            typst_to_latex_with_options(&content, &t2l_options)
        }
        BatchDirection::Auto => unreachable!("auto direction must be resolved before conversion"),
    };

    if let Some(parent) = plan.output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&plan.output_path, converted)
}
