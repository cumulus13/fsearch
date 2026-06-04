// File: src\main.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-10
// Description:
// License: MIT

//! `fsearch` CLI — thin wrapper around the `fsearch` library crate.

use clap::Parser;
use clap_version_flag::colorful_version;
use cli::{Cli, Command, ConfigAction, DupAlgo, DupMode, SearchMethod};
use fsearch::config::Config;
use fsearch::duplicates::{DuplicateMode, DuplicateOptions, HashAlgorithm};
use fsearch::output::Printer;
use fsearch::searcher::{fast_find, parse_patterns, recursive_find, SearchOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

mod cli;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && (args[1] == "-V" || args[1] == "--version") {
        let version = colorful_version!();
        version.print_and_exit();
    }

    // Enable ANSI colours on Windows terminals
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();

    let args = Cli::parse();
    let cfg = Config::load();

    match args.command {
        Command::Find(find) => run_find(find, &cfg),
        Command::Dup(dup) => run_dup(dup, &cfg),
        Command::Config(c) => run_config(c, &cfg),
    }
}

// ── find subcommand ───────────────────────────────────────────────────────────

fn run_find(args: cli::FindArgs, cfg: &Config) {
    let printer = Printer::new(cfg);

    let base_dir = PathBuf::from(args.path.as_deref().unwrap_or("."));

    let case_insensitive = if args.case_sensitive {
        false
    } else if args.case_insensitive {
        true
    } else {
        cfg.case_insensitive
    };

    let include_patterns = if !args.include.is_empty() {
        parse_patterns(&args.include, case_insensitive)
    } else {
        parse_patterns(&cfg.default_include, case_insensitive)
    };

    // Exclude dirs: CLI appends to config defaults
    let mut exclude_dirs = cfg.excluded_dirs();
    if !args.exclude.is_empty() {
        exclude_dirs.extend(
            args.exclude
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }

    let opts = SearchOptions {
        base_dir: base_dir.clone(),
        pattern: args.pattern.clone(),
        max_depth: args.depth,
        include_dirs: !args.no_dir,
        case_insensitive,
        search_in_files: args.search_in_files,
        include_patterns,
        exclude_dirs,
        max_line_length: cfg.max_line_length,
        binary_check_bytes: cfg.binary_check_bytes,
        max_results: if args.max_results > 0 {
            args.max_results
        } else {
            cfg.max_results
        },
    };

    if args.verbose || cfg.verbose {
        printer.print_banner();
        printer.print_searching(&base_dir.display().to_string(), &args.pattern);
        if !opts.include_patterns.is_empty() {
            printer.print_info(&format!(
                "Include filter: {}",
                opts.include_patterns.join(", ")
            ));
        }
        printer.print_info(&format!(
            "Depth: {}  |  Case-insensitive: {}  |  Content: {}",
            args.depth, case_insensitive, args.search_in_files
        ));
    }

    configure_rayon(cfg.threads);

    let interrupted = make_interrupt_flag();
    let start = Instant::now();

    let result = match args.method {
        SearchMethod::Walkdir => fast_find(&opts, Arc::clone(&interrupted)),
        SearchMethod::Recursive => recursive_find(&opts, Arc::clone(&interrupted)),
    };

    let elapsed = start.elapsed();
    check_interrupted(&interrupted, &printer);

    match result {
        Ok(matches) => {
            printer.print_results(&matches, args.search_in_files, elapsed);
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

// ── dup subcommand ────────────────────────────────────────────────────────────

fn run_dup(args: cli::DupArgs, cfg: &Config) {
    let printer = Printer::new(cfg);
    let base_dir = PathBuf::from(&args.path);

    let mode = match args.mode {
        DupMode::Content => DuplicateMode::Content,
        DupMode::Name => DuplicateMode::Name,
        DupMode::Size => DuplicateMode::Size,
    };
    let algorithm = match args.algo {
        DupAlgo::Md5 => HashAlgorithm::Md5,
        DupAlgo::Sha256 => HashAlgorithm::Sha256,
    };

    let mut exclude_dirs = cfg.excluded_dirs();
    if !args.exclude.is_empty() {
        exclude_dirs.extend(
            args.exclude
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }

    let include_patterns = if !args.include.is_empty() {
        args.include
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        vec![]
    };

    let opts = DuplicateOptions {
        base_dir: base_dir.clone(),
        max_depth: args.depth,
        mode,
        algorithm,
        buffer_size: cfg.hash_buffer_size,
        min_size: args.min_size,
        max_size: args.max_size,
        skip_binary: args.skip_binary,
        binary_check_bytes: cfg.binary_check_bytes,
        include_patterns,
        exclude_dirs,
        max_results: if args.max_results > 0 {
            args.max_results
        } else {
            cfg.max_results
        },
    };

    if args.verbose || cfg.verbose {
        printer.print_banner();
        printer.print_scanning_dups(
            &base_dir.display().to_string(),
            &format!("{:?}", mode).to_lowercase(),
            algorithm.as_str(),
        );
    }

    configure_rayon(cfg.threads);

    let interrupted = make_interrupt_flag();
    let start = Instant::now();

    let result = fsearch::duplicates::find_duplicates(&opts, Arc::clone(&interrupted));
    let elapsed = start.elapsed();
    check_interrupted(&interrupted, &printer);

    match result {
        Ok((groups, summary)) => {
            printer.print_duplicates(&groups, &summary, elapsed);
            if groups.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            printer.print_error(&e.to_string());
            std::process::exit(1);
        }
    }
}

// ── config subcommand ─────────────────────────────────────────────────────────

fn run_config(args: cli::ConfigArgs, cfg: &Config) {
    let printer = Printer::new(cfg);
    match args.action {
        ConfigAction::Init => match Config::write_default() {
            Ok(p) => printer.print_info(&format!("✅ Config written to: {}", p.display())),
            Err(e) => {
                printer.print_error(&e.to_string());
                std::process::exit(1);
            }
        },
        ConfigAction::Show => match toml::to_string_pretty(cfg) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                printer.print_error(&e.to_string());
                std::process::exit(1);
            }
        },
        ConfigAction::Path => {
            if let Some(dir) = dirs::config_dir() {
                println!("{}", dir.join("fsearch").join("config.toml").display());
            } else {
                printer.print_error("Cannot determine user config directory");
                std::process::exit(1);
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn configure_rayon(threads: usize) {
    if threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }
}

fn make_interrupt_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::clone(&flag);
    ctrlc::set_handler(move || flag2.store(true, Ordering::Relaxed)).ok();
    flag
}

fn check_interrupted(flag: &Arc<AtomicBool>, printer: &Printer<'_>) {
    if flag.load(Ordering::Relaxed) {
        printer.print_warn("Operation interrupted by user (Ctrl-C)");
        std::process::exit(130);
    }
}
