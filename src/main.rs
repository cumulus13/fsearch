// File: src\main.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-10
// Description: 
// License: MIT

mod binary;
mod cli;
mod colors;
mod config;
mod error;
mod output;
mod searcher;

use clap::Parser;
use clap_version_flag::colorful_version;
use cli::{Cli, Method};
use config::Config;
use output::Printer;
use searcher::{fast_find, parse_patterns, recursive_find, SearchOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    // Enable ANSI colours on Windows
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();

    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && (args[1] == "-V" || args[1] == "--version") {
        let version = colorful_version!();
        version.print_and_exit();
    }

    let cli = Cli::parse();
    let cfg = Config::load();
    let printer = Printer::new(&cfg);

    // ── Special commands ──────────────────────────────────────────────────────
    if cli.init_config {
        match Config::write_default() {
            Ok(path) => {
                printer.print_info(&format!("✅ Default config written to: {}", path.display()));
            }
            Err(e) => {
                printer.print_error(&e.to_string());
                std::process::exit(1);
            }
        }
        return;
    }

    if cli.show_config {
        match toml::to_string_pretty(&cfg) {
            Ok(s) => println!("{}", s),
            Err(e) => printer.print_error(&e.to_string()),
        }
        return;
    }

    // ── Ctrl-C handler ────────────────────────────────────────────────────────
    let interrupted = Arc::new(AtomicBool::new(false));
    {
        let flag = Arc::clone(&interrupted);
        ctrlc::set_handler(move || {
            flag.store(true, Ordering::Relaxed);
        })
        .ok();
    }

    // ── Build SearchOptions ───────────────────────────────────────────────────
    let pattern = match &cli.pattern {
        Some(p) => p.clone(),
        None => {
            printer.print_error("PATTERN argument is required");
            std::process::exit(1);
        }
    };

    let base_dir = PathBuf::from(cli.path.as_deref().unwrap_or("."));

    // Case sensitivity: CLI flags > config default
    let case_insensitive = if cli.case_sensitive {
        false
    } else if cli.case_insensitive {
        true
    } else {
        cfg.case_insensitive
    };

    // Include patterns: CLI > config default
    let include_patterns = if !cli.include.is_empty() {
        parse_patterns(&cli.include, case_insensitive)
    } else {
        parse_patterns(&cfg.default_include, case_insensitive)
    };

    // Exclude dirs: CLI appends to config defaults
    let mut exclude_dirs = cfg.excluded_dirs();
    if !cli.exclude.is_empty() {
        let extra: Vec<String> = cli
            .exclude
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        exclude_dirs.extend(extra);
    }

    let max_results = if cli.max_results > 0 {
        cli.max_results
    } else {
        cfg.max_results
    };

    let opts = SearchOptions {
        base_dir: base_dir.clone(),
        pattern: pattern.clone(),
        max_depth: cli.depth,
        include_dirs: !cli.no_dir,
        case_insensitive,
        search_in_files: cli.search_in_files,
        use_regex: false,
        include_patterns,
        exclude_dirs,
        max_line_length: cfg.max_line_length,
        binary_check_bytes: cfg.binary_check_bytes,
        max_results,
    };

    // ── Verbose banner ────────────────────────────────────────────────────────
    if cli.verbose || cfg.verbose {
        printer.print_banner();
        printer.print_searching(&base_dir.display().to_string(), &pattern);
        if !opts.include_patterns.is_empty() {
            printer.print_info(&format!(
                "Include filter: {}",
                opts.include_patterns.join(", ")
            ));
        }
        printer.print_info(&format!(
            "Depth: {}  |  Case-insensitive: {}  |  Content search: {}",
            cli.depth, case_insensitive, cli.search_in_files
        ));
    }

    // ── Configure Rayon thread pool ───────────────────────────────────────────
    if cfg.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(cfg.threads)
            .build_global()
            .ok();
    }

    // ── Execute search ────────────────────────────────────────────────────────
    let start = Instant::now();

    let results = match cli.method {
        Method::Walkdir => fast_find(&opts, Arc::clone(&interrupted)),
        Method::Recursive => recursive_find(&opts, Arc::clone(&interrupted)),
    };

    let elapsed = start.elapsed();

    if interrupted.load(Ordering::Relaxed) {
        printer.print_warn("Search interrupted by user (Ctrl-C)");
        std::process::exit(130);
    }

    match results {
        Ok(matches) => {
            printer.print_results(&matches, cli.search_in_files, elapsed);
            // Exit code 1 when nothing found (grep convention)
            if matches.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            printer.print_error(&e.to_string());
            std::process::exit(1);
        }
    }
}
