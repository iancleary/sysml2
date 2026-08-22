use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

/// Version of the machine-readable `sysml check` report.
pub const CHECK_REPORT_SCHEMA_VERSION: u32 = 1;

/// Deterministic result of checking one or more SysML source files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    pub schema_version: u32,
    pub tool_version: String,
    /// The strongest claim made by this report. Version 1 checks syntax only.
    pub validation_level: String,
    pub valid: bool,
    pub files: Vec<CheckFileReport>,
}

/// Syntax-check result for one source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckFileReport {
    pub path: String,
    pub valid: bool,
    pub diagnostics: Vec<CheckDiagnostic>,
}

/// One stable, machine-readable source diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub span: CheckSpan,
}

/// A source range with 1-based lines and 1-based UTF-8 byte columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CheckSpan {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// Failure to discover or read the requested model inputs.
#[derive(Debug)]
pub enum CheckError {
    NoInputs,
    NoModels(PathBuf),
    UnsupportedInput(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    ParserInitialization(String),
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputs => write!(
                formatter,
                "at least one SysML file or directory is required"
            ),
            Self::NoModels(path) => {
                write!(formatter, "no .sysml files found under {}", path.display())
            }
            Self::UnsupportedInput(path) => write!(
                formatter,
                "unsupported input {}; expected a .sysml file or directory",
                path.display()
            ),
            Self::Io { path, source } => {
                write!(formatter, "failed to access {}: {source}", path.display())
            }
            Self::ParserInitialization(message) => {
                write!(
                    formatter,
                    "failed to initialize the SysML parser: {message}"
                )
            }
        }
    }
}

impl Error for CheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Check `.sysml` files or directories recursively using the current syntax
/// frontend. Directory entries and output files are sorted for deterministic
/// reports. Directory symlinks encountered during recursion are not followed.
pub fn check_paths(paths: &[PathBuf]) -> Result<CheckReport, CheckError> {
    let sources = load_model_sources(paths)?;
    check_sources(&sources)
}

pub(crate) fn load_model_sources(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>, CheckError> {
    let files = discover_model_files(paths)?;
    files
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).map_err(|source| CheckError::Io {
                path: path.clone(),
                source,
            })?;
            Ok((path, source))
        })
        .collect()
}

pub(crate) fn check_sources(sources: &[(PathBuf, String)]) -> Result<CheckReport, CheckError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sysml::LANGUAGE.into())
        .map_err(|error| CheckError::ParserInitialization(error.to_string()))?;

    let mut file_reports = Vec::with_capacity(sources.len());
    for (path, source) in sources {
        file_reports.push(check_source(&mut parser, path, source));
    }

    let valid = file_reports.iter().all(|file| file.valid);
    Ok(CheckReport {
        schema_version: CHECK_REPORT_SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        validation_level: "syntax".to_owned(),
        valid,
        files: file_reports,
    })
}

pub(crate) fn discover_model_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, CheckError> {
    if paths.is_empty() {
        return Err(CheckError::NoInputs);
    }

    let mut files = Vec::new();
    for path in paths {
        let before = files.len();
        collect_model_files(path, &mut files, true)?;
        if files.len() == before && path.is_dir() {
            return Err(CheckError::NoModels(path.clone()));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_model_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    explicit: bool,
) -> Result<(), CheckError> {
    let metadata = fs::metadata(path).map_err(|source| CheckError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if metadata.is_file() {
        if is_sysml_file(path) {
            files.push(path.to_path_buf());
            return Ok(());
        }
        return if explicit {
            Err(CheckError::UnsupportedInput(path.to_path_buf()))
        } else {
            Ok(())
        };
    }

    if !metadata.is_dir() {
        return Err(CheckError::UnsupportedInput(path.to_path_buf()));
    }

    let mut entries = fs::read_dir(path)
        .map_err(|source| CheckError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| CheckError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let file_type = entry.file_type().map_err(|source| CheckError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_symlink() {
            continue;
        }
        collect_model_files(&entry.path(), files, false)?;
    }
    Ok(())
}

fn is_sysml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sysml"))
}

fn check_source(parser: &mut Parser, path: &Path, source: &str) -> CheckFileReport {
    let mut diagnostics = Vec::new();
    match parser.parse(source, None) {
        Some(tree) => collect_syntax_diagnostics(tree.root_node(), &mut diagnostics),
        None => diagnostics.push(CheckDiagnostic {
            severity: "error".to_owned(),
            code: "syntax.parser_failed".to_owned(),
            message: "the parser did not produce a syntax tree".to_owned(),
            span: CheckSpan {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
        }),
    }

    CheckFileReport {
        path: normalized_path(path),
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

fn collect_syntax_diagnostics(node: Node<'_>, diagnostics: &mut Vec<CheckDiagnostic>) {
    if node.is_error() {
        diagnostics.push(diagnostic_for_node(
            node,
            "syntax.error",
            "invalid or unexpected SysML syntax",
        ));
        return;
    }

    if node.is_missing() {
        diagnostics.push(diagnostic_for_node(
            node,
            "syntax.missing",
            &format!("missing {}", node.kind()),
        ));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_syntax_diagnostics(child, diagnostics);
    }
}

fn diagnostic_for_node(node: Node<'_>, code: &str, message: &str) -> CheckDiagnostic {
    let start = node.start_position();
    let end = node.end_position();
    CheckDiagnostic {
        severity: "error".to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
        span: CheckSpan {
            start_line: start.row + 1,
            start_column: start.column + 1,
            end_line: end.row + 1,
            end_column: end.column + 1,
        },
    }
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_inline(source: &str) -> CheckFileReport {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_sysml::LANGUAGE.into())
            .expect("SysML parser should initialize");
        check_source(&mut parser, Path::new("inline.sysml"), source)
    }

    #[test]
    fn accepts_stable_short_name_and_declared_interface_name() {
        let report = check_inline(
            "package Demo {
                interface def Link;
                part def System {
                    interface <'IF-DATA-001'> link : Link connect a to b;
                }
            }",
        );

        assert!(report.valid, "{:?}", report.diagnostics);
    }

    #[test]
    fn rejects_unclosed_model_body() {
        let report = check_inline("package Demo { part def Component;");

        assert!(!report.valid);
        assert!(report
            .diagnostics
            .iter()
            .all(|item| item.severity == "error"));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.code.starts_with("syntax.")));
    }
}
