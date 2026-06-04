// File: src\cli.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "fsearch",
    bin_name = "fsearch",
    version,
    about = "⚡ Fast file search & duplicate finder",
    long_about = "⚡ fsearch — blazingly fast, cross-platform file & content search\n\n\
    Subcommands:\n\
    \n  fsearch find   <PATTERN>   Search for files/content\
    \n  fsearch dup    [PATH]      Find duplicate files\
    \n  fsearch config             Config helpers\
    \n\nRun `fsearch <SUBCOMMAND> --help` for details.",
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

// ── find args ─────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(
    about = "🔍 Search for files by name or content",
    after_help = "EXAMPLES:\n\
    \n  fsearch find '*.rs'               # files by glob\
    \n  fsearch find TODO -f -i '*.py'    # content search\
    \n  fsearch find README -C -d 5       # case-sensitive, depth 5\n"
)]
pub struct FindArgs {
    /// Pattern to search for (supports `*` and `?` wildcards)
    pub pattern: String,

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

    /// Directory to search in (default: current directory)
    #[arg(short = 'p', long, value_name = "PATH")]
    pub path: Option<String>,

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

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SearchMethod {
    #[value(name = "1")]
    Walkdir = 1,
    #[value(name = "2")]
    Recursive = 2,
}

// ── dup args ──────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(
    about = "🔁 Find duplicate files",
    after_help = "EXAMPLES:\n\
    \n  fsearch dup ~/Downloads                # content duplicates\
    \n  fsearch dup . --mode name              # same filename in different dirs\
    \n  fsearch dup . --mode size              # same size (fast, imprecise)\
    \n  fsearch dup . --algo md5 -d 5          # use MD5, depth 5\
    \n  fsearch dup . -i '*.jpg,*.png'         # only images\
    \n  fsearch dup . --min-size 1048576       # files >= 1 MiB only\n"
)]
pub struct DupArgs {
    /// Directory to scan (default: current directory)
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: String,

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

// ── config args ───────────────────────────────────────────────────────────────

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
