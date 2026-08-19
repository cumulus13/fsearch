// File: src\cli.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT
//! CLI argument definitions (clap 4).
//!
//! Both `find` and `dup` accept **one or more paths** — either positional
//! arguments or repeated `-p`/`--path` flags, or a mix of both.

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use clap_color_help::default_styles;

// ── Top-level ─────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "fsearch",
    bin_name = "fsearch",
    version,
    about = "⚡ Fast file search & duplicate finder",
    styles = default_styles(),
    long_about = "⚡ fsearch — blazingly fast, cross-platform file & content search\n\n\
    Subcommands:\n\
    \n  fsearch find  <PATTERN> [PATHS…]   Search for files / content\
    \n  fsearch dup   [PATHS…]             Find duplicate files\
    \n  fsearch config                     Config helpers\
    \n\nRun `fsearch <SUBCOMMAND> --help` for full option details.",
    author = "Hadi Cahyadi <cumulus13@gmail.com>"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

// ── Subcommands ───────────────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 🔍 Search for files by name or content
    #[command(alias = "f")]
    Find(FindArgs),

    /// 🔁 Find duplicate files
    #[command(alias = "d", alias = "dupes", alias = "duplicates")]
    Dup(DupArgs),

    /// ⚙️  Configuration helpers
    #[command(alias = "cfg")]
    Config(ConfigArgs),
}

// ── find ──────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(
    about = "🔍 Search for files by name or content",
    after_help = "EXAMPLES:\n\
    \n  # Single directory (default = current dir)\
    \n  fsearch find '*.rs'\
    \n\n  # Multiple directories as positional args\
    \n  fsearch find '*.rs' ./src ./tests ./benches\
    \n\n  # Multiple directories via -p flag (repeatable)\
    \n  fsearch find '*.rs' -p ./src -p ./tests\
    \n\n  # Mix positional + flag\
    \n  fsearch find TODO -f -i '*.py' ./lib -p ./scripts -d 5\
    \n\n  # Case-sensitive, depth 5\
    \n  fsearch find README -C -d 5\n"
)]
pub struct FindArgs {
    /// Pattern to search for (supports `*` and `?` wildcards)
    pub pattern: String,

    /// Directories to search (positional, repeatable; default: current dir)
    #[arg(value_name = "PATH", num_args = 0..)]
    pub paths: Vec<String>,

    /// Additional search directories (repeatable flag: -p ./src -p ./lib)
    #[arg(short = 'p', long = "path", value_name = "PATH", action = ArgAction::Append)]
    pub path_flags: Vec<String>,

    /// Search method: 1 = walkdir+rayon (fast), 2 = recursive
    #[arg(short = 'm', long, value_name = "1|2", default_value = "1")]
    pub method: SearchMethod,

    /// Case-insensitive matching (default on)
    #[arg(short = 'c', long = "case-insensitive", action = ArgAction::SetTrue)]
    pub case_insensitive: bool,

    /// Case-sensitive matching (overrides -c)
    #[arg(short = 'C', long = "case-sensitive", action = ArgAction::SetTrue)]
    pub case_sensitive: bool,

    /// Maximum directory depth (0 = current dir only)
    #[arg(short = 'd', long = "deep", value_name = "DEPTH", default_value = "1")]
    pub depth: u32,

    /// Exclude directories from results (files only)
    #[arg(short = 'D', long = "no-dir", action = ArgAction::SetTrue)]
    pub no_dir: bool,

    /// Search for text inside files instead of matching filenames
    #[arg(short = 'f', long = "file", action = ArgAction::SetTrue)]
    pub search_in_files: bool,

    /// Only include files matching these glob patterns (comma-separated)
    #[arg(short = 'i', long, value_name = "GLOBS", default_value = "")]
    pub include: String,

    /// Additional directory names to skip (comma-separated)
    #[arg(short = 'x', long, value_name = "DIRS", default_value = "")]
    pub exclude: String,

    /// Limit number of results (0 = unlimited)
    #[arg(short = 'n', long, value_name = "N", default_value = "0")]
    pub max_results: usize,

    /// Print status messages and timing
    #[arg(short = 'v', long, action = ArgAction::SetTrue)]
    pub verbose: bool,
}

impl FindArgs {
    /// Merge positional `paths` + `-p` flags into one deduplicated list.
    /// Falls back to `["."]` when nothing is specified.
    pub fn resolved_paths(&self) -> Vec<String> {
        let mut all: Vec<String> = self
            .paths
            .iter()
            .chain(self.path_flags.iter())
            .cloned()
            .collect();
        // deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        all.retain(|p| seen.insert(p.clone()));
        if all.is_empty() {
            vec![".".into()]
        } else {
            all
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SearchMethod {
    #[value(name = "1")]
    Walkdir = 1,
    #[value(name = "2")]
    Recursive = 2,
}

// ── dup ───────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(
    about = "🔁 Find duplicate files",
    after_help = "EXAMPLES:\n\
    \n  # Single directory\
    \n  fsearch dup ~/Downloads\
    \n\n  # Multiple directories (union — dupes across all)\
    \n  fsearch dup ~/Documents ~/Downloads ~/Desktop\
    \n\n  # Multiple via -p flag\
    \n  fsearch dup -p ~/Music -p /mnt/backup/Music\
    \n\n  # Mix positional + flag\
    \n  fsearch dup ~/Photos -p /mnt/nas/Photos\
    \n\n  # Name mode, deep search\
    \n  fsearch dup ./src ./vendor --mode name -d 10\
    \n\n  # Images >= 100 KiB, MD5\
    \n  fsearch dup ~/Photos --algo md5 -i '*.jpg,*.png' --min-size 102400\n"
)]
pub struct DupArgs {
    /// Directories to scan (positional, repeatable; default: current dir)
    #[arg(value_name = "PATH", num_args = 0..)]
    pub paths: Vec<String>,

    /// Additional scan directories (repeatable flag: -p ./dir1 -p ./dir2)
    #[arg(short = 'p', long = "path", value_name = "PATH", action = ArgAction::Append)]
    pub path_flags: Vec<String>,

    /// Detection mode
    #[arg(long, value_name = "MODE", default_value = "content")]
    pub mode: DupMode,

    /// Hashing algorithm for content mode
    #[arg(long, value_name = "ALGO", default_value = "sha256")]
    pub algo: DupAlgo,

    /// Maximum directory depth
    #[arg(short = 'd', long = "deep", value_name = "DEPTH", default_value = "10")]
    pub depth: u32,

    /// Only include files matching these glob patterns (comma-separated)
    #[arg(short = 'i', long, value_name = "GLOBS", default_value = "")]
    pub include: String,

    /// Additional directory names to skip (comma-separated)
    #[arg(short = 'x', long, value_name = "DIRS", default_value = "")]
    pub exclude: String,

    /// Skip files smaller than N bytes
    #[arg(long, value_name = "BYTES", default_value = "1")]
    pub min_size: u64,

    /// Skip files larger than N bytes (0 = no limit)
    #[arg(long, value_name = "BYTES", default_value = "0")]
    pub max_size: u64,

    /// Skip binary files during content hashing
    #[arg(long, action = ArgAction::SetTrue)]
    pub skip_binary: bool,

    /// Limit number of groups shown (0 = unlimited)
    #[arg(short = 'n', long, value_name = "N", default_value = "0")]
    pub max_results: usize,

    /// Print status messages and timing
    #[arg(short = 'v', long, action = ArgAction::SetTrue)]
    pub verbose: bool,
}

impl DupArgs {
    /// Merge positional `paths` + `-p` flags into one deduplicated list.
    /// Falls back to `["."]` when nothing is specified.
    pub fn resolved_paths(&self) -> Vec<String> {
        let mut all: Vec<String> = self
            .paths
            .iter()
            .chain(self.path_flags.iter())
            .cloned()
            .collect();
        let mut seen = std::collections::HashSet::new();
        all.retain(|p| seen.insert(p.clone()));
        if all.is_empty() {
            vec![".".into()]
        } else {
            all
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DupMode {
    /// Byte-for-byte identical content (uses hashing)
    Content,
    /// Same filename in different directories
    Name,
    /// Same file size (fast but imprecise)
    Size,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DupAlgo {
    Md5,
    Sha256,
}

// ── config ────────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(about = "⚙️  Configuration helpers")]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Write an annotated default config to ~/.config/fsearch/config.toml
    Init,
    /// Print the active configuration as TOML
    Show,
    /// Print the path where the config file would be stored
    Path,
}
