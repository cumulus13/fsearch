// File: src\searcher.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT

//! Core file-search logic — exposed as a library API.
//!
//! Two search engines are provided:
//! * [`fast_find`]  — walkdir + rayon (parallel, default)
//! * [`recursive_find`] — manual DFS (deterministic order)

use crate::binary::is_binary;
use crate::config::{split_csv, Config};
use crate::error::{FsearchError, FsearchResult};
use glob::Pattern;
use rayon::prelude::*;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single matched line inside a file: `(1-based line number, line text)`.
pub type LineMatch = (usize, String);

/// One result item returned by every search function.
#[derive(Debug, Clone)]
pub enum SearchMatch {
    /// Filename-only match.
    Path(PathBuf),
    /// Content match with the lines that contain the pattern.
    Content {
        path: PathBuf,
        lines: Vec<LineMatch>,
    },
}

impl SearchMatch {
    /// The matched path regardless of variant.
    pub fn path(&self) -> &Path {
        match self {
            Self::Path(p) => p,
            Self::Content { path, .. } => path,
        }
    }

    /// Returns `true` when this is a content (in-file) match.
    pub fn is_content(&self) -> bool {
        matches!(self, Self::Content { .. })
    }
}

// ── Search options ────────────────────────────────────────────────────────────

/// All parameters that control a single search operation.
///
/// Build via [`SearchOptions::builder`] or construct directly.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Directory to start the search from.
    pub base_dir: PathBuf,
    /// Pattern to match (supports `*` and `?` wildcards, or plain substring).
    pub pattern: String,
    /// Maximum recursion depth (0 = only `base_dir` itself).
    pub max_depth: u32,
    /// Include directory entries in filename-search results.
    pub include_dirs: bool,
    /// Match case-insensitively.
    pub case_insensitive: bool,
    /// Search for `pattern` inside file contents instead of matching names.
    pub search_in_files: bool,
    /// Only include files that match these glob patterns (empty = all).
    pub include_patterns: Vec<String>,
    /// Directory names to skip entirely during traversal.
    pub exclude_dirs: Vec<String>,
    /// Lines longer than this are skipped during content search.
    pub max_line_length: usize,
    /// Bytes read to probe for binary content.
    pub binary_check_bytes: usize,
    /// Cap the number of results returned (0 = unlimited).
    pub max_results: usize,
}

impl SearchOptions {
    /// Construct a minimal [`SearchOptions`] from a [`Config`] and a pattern.
    pub fn from_config(cfg: &Config, base_dir: PathBuf, pattern: String) -> Self {
        Self {
            base_dir,
            pattern,
            max_depth: cfg.default_depth,
            include_dirs: cfg.include_dirs,
            case_insensitive: cfg.case_insensitive,
            search_in_files: false,
            include_patterns: split_csv(&cfg.default_include),
            exclude_dirs: cfg.excluded_dirs(),
            max_line_length: cfg.max_line_length,
            binary_check_bytes: cfg.binary_check_bytes,
            max_results: cfg.max_results,
        }
    }

    /// Fluent builder — start from a pattern and current directory.
    pub fn builder(pattern: impl Into<String>) -> SearchOptionsBuilder {
        SearchOptionsBuilder::new(pattern.into())
    }
}

/// Fluent builder for [`SearchOptions`].
pub struct SearchOptionsBuilder(SearchOptions);

impl SearchOptionsBuilder {
    fn new(pattern: String) -> Self {
        Self(SearchOptions {
            base_dir: PathBuf::from("."),
            pattern,
            max_depth: 1,
            include_dirs: true,
            case_insensitive: true,
            search_in_files: false,
            include_patterns: vec![],
            exclude_dirs: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                ".svn".into(),
                "__pycache__".into(),
                ".hg".into(),
                ".cache".into(),
            ],
            max_line_length: 10_000,
            binary_check_bytes: 1024,
            max_results: 0,
        })
    }

    pub fn base_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.0.base_dir = p.into();
        self
    }
    pub fn max_depth(mut self, d: u32) -> Self {
        self.0.max_depth = d;
        self
    }
    pub fn include_dirs(mut self, v: bool) -> Self {
        self.0.include_dirs = v;
        self
    }
    pub fn case_insensitive(mut self, v: bool) -> Self {
        self.0.case_insensitive = v;
        self
    }
    pub fn search_in_files(mut self, v: bool) -> Self {
        self.0.search_in_files = v;
        self
    }
    pub fn include_patterns(mut self, p: Vec<String>) -> Self {
        self.0.include_patterns = p;
        self
    }
    pub fn exclude_dirs(mut self, d: Vec<String>) -> Self {
        self.0.exclude_dirs = d;
        self
    }
    pub fn max_results(mut self, n: usize) -> Self {
        self.0.max_results = n;
        self
    }
    pub fn build(self) -> SearchOptions {
        self.0
    }
}

// ── Pattern helpers ───────────────────────────────────────────────────────────

/// Parse a comma-separated pattern string into a `Vec<String>`,
/// optionally lower-casing each entry.
pub fn parse_patterns(raw: &str, case_insensitive: bool) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if case_insensitive {
                s.to_lowercase()
            } else {
                s
            }
        })
        .collect()
}

fn matches_include(name: &str, patterns: &[String], ci: bool) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let cmp = if ci {
        name.to_lowercase()
    } else {
        name.to_string()
    };
    patterns.iter().any(|p| {
        Pattern::new(p)
            .map(|pat| pat.matches(&cmp))
            .unwrap_or(false)
    })
}

fn is_excluded_dir(name: &str, excludes: &[String]) -> bool {
    excludes
        .iter()
        .any(|ex| Pattern::new(ex).map(|p| p.matches(name)).unwrap_or(false) || ex == name)
}

fn name_matches(entry_name: &str, pattern: &str, ci: bool) -> bool {
    let (name, pat) = if ci {
        (entry_name.to_lowercase(), pattern.to_lowercase())
    } else {
        (entry_name.to_string(), pattern.to_string())
    };
    if pat.contains('*') || pat.contains('?') {
        Pattern::new(&pat)
            .map(|p| p.matches(&name))
            .unwrap_or(false)
    } else {
        name.contains(&pat)
    }
}

// ── Content search ────────────────────────────────────────────────────────────

fn search_in_file(
    path: &Path,
    pattern: &str,
    ci: bool,
    max_line: usize,
    check_bytes: usize,
) -> Vec<LineMatch> {
    if is_binary(path, check_bytes) {
        return vec![];
    }
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let pat = if ci {
        pattern.to_lowercase()
    } else {
        pattern.to_string()
    };
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(i, lr)| {
            let line = lr.ok()?;
            if line.len() > max_line {
                return None;
            }
            let cmp = if ci {
                line.to_lowercase()
            } else {
                line.clone()
            };
            if cmp.contains(&pat) {
                Some((i + 1, line))
            } else {
                None
            }
        })
        .collect()
}

// ── Method 1 — walkdir + rayon ────────────────────────────────────────────────

/// Search using `walkdir` + `rayon` (parallel, recommended for large trees).
pub fn fast_find(
    opts: &SearchOptions,
    interrupted: Arc<AtomicBool>,
) -> FsearchResult<Vec<SearchMatch>> {
    validate(opts)?;

    let entries: Vec<_> = WalkDir::new(&opts.base_dir)
        .max_depth(opts.max_depth as usize + 1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() && e.depth() > 0 {
                let name = e.file_name().to_string_lossy().to_string();
                if e.depth() > 0 && is_excluded_dir(&name, &opts.exclude_dirs) {
                    return false;
                }
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.depth() > 0)
        .collect();

    let results: Vec<SearchMatch> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if interrupted.load(Ordering::Relaxed) {
                return None;
            }
            let is_dir = entry.file_type().is_dir();
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().to_path_buf();

            if !is_dir && !matches_include(&name, &opts.include_patterns, opts.case_insensitive) {
                return None;
            }
            if opts.search_in_files {
                if is_dir {
                    return None;
                }
                let lines = search_in_file(
                    &path,
                    &opts.pattern,
                    opts.case_insensitive,
                    opts.max_line_length,
                    opts.binary_check_bytes,
                );
                if lines.is_empty() {
                    None
                } else {
                    Some(SearchMatch::Content { path, lines })
                }
            } else {
                if is_dir && !opts.include_dirs {
                    return None;
                }
                if name_matches(&name, &opts.pattern, opts.case_insensitive) {
                    Some(SearchMatch::Path(path))
                } else {
                    None
                }
            }
        })
        .collect();

    Ok(cap(results, opts.max_results))
}

// ── Method 2 — manual recursive ──────────────────────────────────────────────

/// Search using a manual DFS (single-threaded, deterministic ordering).
pub fn recursive_find(
    opts: &SearchOptions,
    interrupted: Arc<AtomicBool>,
) -> FsearchResult<Vec<SearchMatch>> {
    validate(opts)?;
    let mut matches = Vec::new();
    walk_dir(&opts.base_dir, opts, 0, &mut matches, &interrupted);
    Ok(cap(matches, opts.max_results))
}

fn walk_dir(
    dir: &Path,
    opts: &SearchOptions,
    depth: u32,
    matches: &mut Vec<SearchMatch>,
    interrupted: &AtomicBool,
) {
    if depth > opts.max_depth || interrupted.load(Ordering::Relaxed) {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if interrupted.load(Ordering::Relaxed) {
            break;
        }
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();

        if file_type.is_dir() {
            if is_excluded_dir(&name, &opts.exclude_dirs) {
                continue;
            }
            if !opts.search_in_files
                && opts.include_dirs
                && name_matches(&name, &opts.pattern, opts.case_insensitive)
            {
                matches.push(SearchMatch::Path(path.clone()));
            }
            walk_dir(&path, opts, depth + 1, matches, interrupted);
        } else if file_type.is_file() {
            if !matches_include(&name, &opts.include_patterns, opts.case_insensitive) {
                continue;
            }
            if opts.search_in_files {
                let lines = search_in_file(
                    &path,
                    &opts.pattern,
                    opts.case_insensitive,
                    opts.max_line_length,
                    opts.binary_check_bytes,
                );
                if !lines.is_empty() {
                    matches.push(SearchMatch::Content { path, lines });
                }
            } else if name_matches(&name, &opts.pattern, opts.case_insensitive) {
                matches.push(SearchMatch::Path(path));
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn validate(opts: &SearchOptions) -> FsearchResult<()> {
    if !opts.base_dir.exists() {
        return Err(FsearchError::DirectoryNotFound(
            opts.base_dir.display().to_string(),
        ));
    }
    if !opts.base_dir.is_dir() {
        return Err(FsearchError::NotADirectory(
            opts.base_dir.display().to_string(),
        ));
    }
    Ok(())
}

fn cap(mut v: Vec<SearchMatch>, limit: usize) -> Vec<SearchMatch> {
    if limit > 0 && v.len() > limit {
        v.truncate(limit);
    }
    v
}
