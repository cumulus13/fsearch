// File: src\config.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT

//! Configuration loading and persistence for `fsearch`.
//!
//! Priority (highest first):
//! 1. `./fsearch.toml`
//! 2. `~/.config/fsearch/config.toml`
//! 3. Built-in defaults

use crate::error::{FsearchError, FsearchResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Complete fsearch configuration.
///
/// All fields are optional at the TOML level — missing keys fall back to
/// the values in [`Config::default`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // ── Search ────────────────────────────────────────────────────────────────
    /// Default max depth for `-d` flag.
    pub default_depth: u32,
    /// 1 = walkdir+rayon, 2 = recursive.
    pub default_method: u8,
    /// Match case-insensitively by default.
    pub case_insensitive: bool,
    /// Include directory entries in filename-search results.
    pub include_dirs: bool,
    /// Bytes to probe when checking for binary content.
    pub binary_check_bytes: usize,
    /// Lines longer than this are skipped in content search.
    pub max_line_length: usize,
    /// Rayon thread count (0 = all logical CPUs).
    pub threads: usize,

    // ── Duplicate detection ───────────────────────────────────────────────────
    /// Hashing algorithm: `"md5"` or `"sha256"`.
    pub hash_algorithm: String,
    /// Buffer size (bytes) used when streaming files for hashing.
    pub hash_buffer_size: usize,
    /// Minimum file size (bytes) to consider for duplicate detection (0 = all).
    pub dup_min_size: u64,
    /// Maximum file size (bytes) to consider (0 = unlimited).
    pub dup_max_size: u64,

    // ── Output ────────────────────────────────────────────────────────────────
    /// Print verbose status to stderr.
    pub verbose: bool,
    /// Show file sizes next to results.
    pub show_size: bool,
    /// Show last-modified timestamps next to results.
    pub show_modified: bool,
    /// Maximum results to return (0 = unlimited).
    pub max_results: usize,

    // ── Colours ───────────────────────────────────────────────────────────────
    pub color_index: String,
    pub color_path: String,
    pub color_line_num: String,
    pub color_line_text: String,
    pub color_header: String,
    pub color_count: String,
    pub color_error: String,
    pub color_warn: String,
    pub color_info: String,
    pub color_pattern: String,
    pub color_dup_group: String,
    pub color_dup_path: String,
    pub color_dup_size: String,

    // ── Filters ───────────────────────────────────────────────────────────────
    /// Comma-separated directory names always skipped during traversal.
    pub exclude_dirs: String,
    /// Comma-separated glob patterns included by default (empty = all).
    pub default_include: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_depth: 1,
            default_method: 1,
            case_insensitive: true,
            include_dirs: true,
            binary_check_bytes: 1024,
            max_line_length: 10_000,
            threads: 0,
            hash_algorithm: "sha256".into(),
            hash_buffer_size: 65_536, // 64 KiB
            dup_min_size: 1,
            dup_max_size: 0,
            verbose: false,
            show_size: false,
            show_modified: false,
            max_results: 0,
            color_index: "#FF88FF".into(),
            color_path: "#FFFF00".into(),
            color_line_num: "#FF4444".into(),
            color_line_text: "#00FFFF".into(),
            color_header: "#FFFFFF".into(),
            color_count: "#00FFFF".into(),
            color_error: "#FF3333".into(),
            color_warn: "#FFAA00".into(),
            color_info: "#00FF88".into(),
            color_pattern: "#FF00FF".into(),
            color_dup_group: "#FF8800".into(),
            color_dup_path: "#FFFF00".into(),
            color_dup_size: "#88FF88".into(),
            exclude_dirs: ".git,node_modules,.svn,__pycache__,.hg,target,.cache".into(),
            default_include: "".into(),
        }
    }
}

impl Config {
    /// Load config: local override → user config → defaults.
    pub fn load() -> Self {
        if let Ok(cfg) = Self::load_from_path(PathBuf::from("fsearch.toml")) {
            return cfg;
        }
        if let Some(dir) = dirs::config_dir() {
            if let Ok(cfg) = Self::load_from_path(dir.join("fsearch").join("config.toml")) {
                return cfg;
            }
        }
        Self::default()
    }

    /// Load from an explicit path — useful for testing and library consumers.
    pub fn load_from_path(path: PathBuf) -> FsearchResult<Self> {
        let text = std::fs::read_to_string(&path).map_err(|e| FsearchError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        toml::from_str(&text).map_err(|e| FsearchError::ConfigParse {
            path: path.display().to_string(),
            source: e,
        })
    }

    /// Write an annotated default config file and return its path.
    pub fn write_default() -> FsearchResult<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| FsearchError::Config("cannot find user config directory".into()))?
            .join("fsearch");
        std::fs::create_dir_all(&dir).map_err(|e| FsearchError::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
        let path = dir.join("config.toml");
        let raw = toml::to_string_pretty(&Self::default())
            .map_err(|e| FsearchError::Config(e.to_string()))?;
        let annotated = format!(
            "# fsearch configuration  ({})\n\
             # All values are defaults. Remove the leading '#' to override.\n\n",
            path.display()
        ) + &raw
            .lines()
            .map(|l| {
                if l.starts_with('[') || l.is_empty() {
                    l.to_string()
                } else {
                    format!("# {l}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, annotated).map_err(|e| FsearchError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(path)
    }

    /// Parsed list of directory-name globs to skip.
    pub fn excluded_dirs(&self) -> Vec<String> {
        split_csv(&self.exclude_dirs)
    }
}

/// Split a comma-separated string, trimming whitespace and dropping empties.
pub(crate) fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}
