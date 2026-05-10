// File: src\searcher.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use crate::binary::is_binary;
use crate::config::Config;
use crate::error::{SearchError, SearchResult};
use glob::Pattern;
use rayon::prelude::*;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// ── Public types ──────────────────────────────────────────────────────────────

/// A single matched line: (1-based line number, line text).
pub type LineMatch = (usize, String);

/// One result item returned by every search function.
#[derive(Debug, Clone)]
pub enum SearchMatch {
    /// Filename-only match — just the path.
    Path(PathBuf),
    /// Content match — path plus the matched lines.
    Content {
        path: PathBuf,
        lines: Vec<LineMatch>,
    },
}

impl SearchMatch {
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        match self {
            Self::Path(p) => p,
            Self::Content { path, .. } => path,
        }
    }
}

// ── Search options ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub base_dir: PathBuf,
    pub pattern: String,
    pub max_depth: u32,
    pub include_dirs: bool,
    pub case_insensitive: bool,
    pub search_in_files: bool,
    #[allow(dead_code)]
    pub use_regex: bool,
    pub include_patterns: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub max_line_length: usize,
    pub binary_check_bytes: usize,
    pub max_results: usize,
}

impl SearchOptions {
    #[allow(dead_code)]
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            base_dir: PathBuf::from("."),
            pattern: String::new(),
            max_depth: cfg.default_depth,
            include_dirs: cfg.include_dirs,
            case_insensitive: cfg.case_insensitive,
            search_in_files: false,
            use_regex: false,
            include_patterns: parse_patterns(&cfg.default_include, cfg.case_insensitive),
            exclude_dirs: cfg.excluded_dirs(),
            max_line_length: cfg.max_line_length,
            binary_check_bytes: cfg.binary_check_bytes,
            max_results: cfg.max_results,
        }
    }
}

// ── Pattern helpers ───────────────────────────────────────────────────────────

/// Split a comma-separated pattern string into a `Vec<String>`,
/// optionally lowercasing each entry for case-insensitive matching.
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

fn matches_include(name: &str, patterns: &[String], case_insensitive: bool) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let name_cmp = if case_insensitive {
        name.to_lowercase()
    } else {
        name.to_string()
    };
    patterns.iter().any(|p| {
        Pattern::new(p)
            .map(|pat| pat.matches(&name_cmp))
            .unwrap_or(false)
    })
}

fn is_excluded_dir(name: &str, excludes: &[String]) -> bool {
    excludes
        .iter()
        .any(|ex| Pattern::new(ex).map(|p| p.matches(name)).unwrap_or(false) || ex == name)
}

fn name_matches(entry_name: &str, pattern: &str, case_insensitive: bool) -> bool {
    let (name, pat) = if case_insensitive {
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
    case_insensitive: bool,
    max_line_length: usize,
    binary_check_bytes: usize,
) -> Vec<LineMatch> {
    if is_binary(path, binary_check_bytes) {
        return vec![];
    }

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let pat = if case_insensitive {
        pattern.to_lowercase()
    } else {
        pattern.to_string()
    };

    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(idx, line_result)| {
            let line = line_result.ok()?;
            if line.len() > max_line_length {
                return None;
            }
            let line_cmp = if case_insensitive {
                line.to_lowercase()
            } else {
                line.clone()
            };
            if line_cmp.contains(&pat) {
                Some((idx + 1, line))
            } else {
                None
            }
        })
        .collect()
}

// ── Method 1 — walkdir + rayon (parallel) ────────────────────────────────────

/// Fast parallel search using `walkdir` + `rayon`.
pub fn fast_find(
    opts: &SearchOptions,
    interrupted: Arc<AtomicBool>,
) -> SearchResult<Vec<SearchMatch>> {
    validate_opts(opts)?;

    let walker = WalkDir::new(&opts.base_dir)
        .max_depth(opts.max_depth as usize + 1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy().to_string();
                if e.depth() > 0 && is_excluded_dir(&name, &opts.exclude_dirs) {
                    return false;
                }
            }
            true
        });

    let entries: Vec<_> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.depth() > 0) // skip the root itself
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

            // Apply include-pattern filter to files
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

    Ok(apply_limit(results, opts.max_results))
}

// ── Method 2 — manual recursive (deterministic) ──────────────────────────────

/// Deterministic single-threaded recursive search.
pub fn recursive_find(
    opts: &SearchOptions,
    interrupted: Arc<AtomicBool>,
) -> SearchResult<Vec<SearchMatch>> {
    validate_opts(opts)?;
    let mut matches = Vec::new();
    walk_dir(&opts.base_dir, opts, 0, &mut matches, &interrupted);
    Ok(apply_limit(matches, opts.max_results))
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

fn validate_opts(opts: &SearchOptions) -> SearchResult<()> {
    if !opts.base_dir.exists() {
        return Err(SearchError::DirectoryNotFound(
            opts.base_dir.display().to_string(),
        ));
    }
    if !opts.base_dir.is_dir() {
        return Err(SearchError::NotADirectory(
            opts.base_dir.display().to_string(),
        ));
    }
    Ok(())
}

fn apply_limit(mut v: Vec<SearchMatch>, limit: usize) -> Vec<SearchMatch> {
    if limit > 0 && v.len() > limit {
        v.truncate(limit);
    }
    v
}
