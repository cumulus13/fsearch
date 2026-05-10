// File: src\cli.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use clap::{ArgAction, Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Method {
    /// walkdir + rayon parallel (default, fastest)
    #[value(name = "1")]
    Walkdir = 1,
    /// Manual recursive (deterministic order)
    #[value(name = "2")]
    Recursive = 2,
}

#[derive(Parser, Debug)]
#[command(
    name = "fsearch",
    bin_name = "fsearch",
    version,
    about = "⚡ Fast file and content search utility",
    long_about = "⚡ fsearch — blazingly fast, cross-platform file & content search\n\n\
    Search for filenames or text inside files with rich coloured output,\n\
    depth control, glob filters, parallel execution and a config file.",
    author = "Hadi Cahyadi <cumulus13@gmail.com>",
    after_help = "EXAMPLES:\n\
    \n  # Find Python files\n  fsearch '*.py'\
    \n\n  # Find 'TODO' inside source files (depth 5)\n  fsearch TODO -f -i '*.py,*.rs' -d 5\
    \n\n  # Case-sensitive search\n  fsearch README -C\
    \n\n  # Initialise a default config file\n  fsearch --init-config\n"
)]
pub struct Cli {
    /// Pattern to search for (supports `*` and `?` wildcards)
    #[arg(value_name = "PATTERN", required_unless_present_any = ["init_config", "show_config"])]
    pub pattern: Option<String>,

    /// Search method: 1 = walkdir+rayon (fast), 2 = recursive
    #[arg(short = 'm', long, value_name = "METHOD", default_value = "1")]
    pub method: Method,

    /// Case-insensitive matching (default)
    #[arg(short = 'c', long = "case-insensitive", action = ArgAction::SetTrue)]
    pub case_insensitive: bool,

    /// Case-sensitive matching (overrides -c)
    #[arg(short = 'C', long = "case-sensitive", action = ArgAction::SetTrue)]
    pub case_sensitive: bool,

    /// Maximum search depth (0 = current directory only)
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

    /// Only include files matching these glob patterns (comma-separated, e.g. "*.py,*.rs")
    #[arg(short = 'i', long, value_name = "PATTERNS", default_value = "")]
    pub include: String,

    /// Additional directory names to exclude, comma-separated (e.g. "dist,build")
    #[arg(short = 'x', long, value_name = "DIRS", default_value = "")]
    pub exclude: String,

    /// Limit number of results (0 = unlimited)
    #[arg(short = 'n', long, value_name = "N", default_value = "0")]
    pub max_results: usize,

    /// Print searching status messages and timing
    #[arg(short = 'v', long, action = ArgAction::SetTrue)]
    pub verbose: bool,

    /// Write a default config file to ~/.config/fsearch/config.toml
    #[arg(long = "init-config", action = ArgAction::SetTrue)]
    pub init_config: bool,

    /// Print the active configuration as TOML and exit
    #[arg(long = "show-config", action = ArgAction::SetTrue)]
    pub show_config: bool,
}
