// File: src\config.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// fsearch configuration.
///
/// Loaded with the following priority (highest first):
///   1. `./fsearch.toml`          — project-local override
///   2. `~/.config/fsearch/config.toml`
///   3. Built-in defaults (this struct's `Default` impl)
///
/// Run `fsearch --init-config` to write an annotated default file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // ── Search behaviour ──────────────────────────────────────────────────────
    /// Default maximum directory depth (CLI `-d` overrides this)
    pub default_depth: u32,
    /// Default search method: 1 = walkdir+rayon, 2 = recursive
    pub default_method: u8,
    /// Default to case-insensitive matching
    pub case_insensitive: bool,
    /// Include directory entries in filename-search results
    pub include_dirs: bool,
    /// Maximum bytes read when probing a file for binary content
    pub binary_check_bytes: usize,
    /// Lines longer than this are skipped during content search
    pub max_line_length: usize,
    /// Number of parallel threads (0 = use Rayon default = logical CPUs)
    pub threads: usize,

    // ── Output / display ──────────────────────────────────────────────────────
    /// Print a spinner / status line while searching
    pub verbose: bool,
    /// Show file sizes next to results  (reserved for future use)
    pub show_size: bool,
    /// Show last-modified timestamps next to results (reserved for future use)
    pub show_modified: bool,
    /// Maximum number of results to print (0 = unlimited)
    pub max_results: usize,

    // ── Colour palette (hex strings like "#FF00FF") ───────────────────────────
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

    // ── Default filters ───────────────────────────────────────────────────────
    /// Comma-separated glob patterns always excluded (e.g. ".git,node_modules")
    pub exclude_dirs: String,
    /// Comma-separated glob patterns to include by default (empty = all files)
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
        if let Some(config_dir) = dirs::config_dir() {
            let user_cfg = config_dir.join("fsearch").join("config.toml");
            if let Ok(cfg) = Self::load_from_path(user_cfg) {
                return cfg;
            }
        }
        Self::default()
    }

    fn load_from_path(path: PathBuf) -> Result<Self> {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Self =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    /// Write an annotated default config to `~/.config/fsearch/config.toml`.
    /// Called by `fsearch --init-config`.
    pub fn write_default() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("cannot determine user config directory")?
            .join("fsearch");
        std::fs::create_dir_all(&config_dir).context("creating config directory")?;
        let path = config_dir.join("config.toml");

        let text =
            toml::to_string_pretty(&Self::default()).context("serialising default config")?;

        // Prepend header comment; comment-out every value line so the file
        // is self-documenting but all defaults are still active.
        let commented = format!(
            "# fsearch configuration\n\
             # Location: {}\n\
             # All values are defaults. Remove the leading '#' to override.\n\n",
            path.display()
        ) + &text
            .lines()
            .map(|l| {
                if l.starts_with('[') || l.is_empty() {
                    l.to_string()
                } else {
                    format!("# {}", l)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        std::fs::write(&path, commented).context("writing config file")?;
        Ok(path)
    }

    /// Returns the list of directory-name globs to skip during traversal.
    pub fn excluded_dirs(&self) -> Vec<String> {
        self.exclude_dirs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}
