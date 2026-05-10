// File: src\output.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description: 
// License: MIT

use crate::colors::{bold_hex, hex_color};
use crate::config::Config;
use crate::searcher::SearchMatch;
use colored::Colorize;
use std::time::Duration;

pub struct Printer<'a> {
    cfg: &'a Config,
}

impl<'a> Printer<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Self { cfg }
    }

    pub fn print_banner(&self) {
        eprintln!(
            "⚡ {} v{}",
            bold_hex("fsearch", &self.cfg.color_info),
            bold_hex(env!("CARGO_PKG_VERSION"), &self.cfg.color_count),
        );
    }

    pub fn print_searching(&self, dir: &str, pattern: &str) {
        eprintln!(
            "🔍 Searching {} for {}",
            bold_hex(dir, &self.cfg.color_path),
            bold_hex(pattern, &self.cfg.color_pattern),
        );
    }

    pub fn print_results(
        &self,
        results: &[SearchMatch],
        _search_in_files: bool,
        elapsed: Duration,
    ) {
        if results.is_empty() {
            println!(
                "\n🕳️  {}\n",
                bold_hex("No results found.", &self.cfg.color_warn)
            );
            return;
        }

        let count_str = results.len().to_string();
        let ms = elapsed.as_millis();

        println!(
            "\n{} {}  {} {} {}",
            "📋".bold(),
            bold_hex("FOUND:", &self.cfg.color_header),
            bold_hex(&count_str, &self.cfg.color_count),
            hex_color("result(s)", &self.cfg.color_info),
            hex_color(format!("[{ms}ms]"), "#888888"),
        );
        println!();

        let zfill = count_str.len();

        for (idx, item) in results.iter().enumerate() {
            let num = format!("{:0>width$}", idx + 1, width = zfill);
            let num_str = bold_hex(&num, &self.cfg.color_index);

            match item {
                SearchMatch::Path(path) => {
                    println!(
                        "{}. {}",
                        num_str,
                        bold_hex(path.display().to_string(), &self.cfg.color_path)
                    );
                }
                SearchMatch::Content { path, lines } => {
                    println!(
                        "{}. {}",
                        num_str,
                        bold_hex(path.display().to_string(), &self.cfg.color_path)
                    );
                    for (line_num, line_text) in lines {
                        println!(
                            "   {} {} {}",
                            bold_hex(format!("{:>5}", line_num), &self.cfg.color_line_num),
                            hex_color("│", "#555555"),
                            hex_color(line_text.trim_end(), &self.cfg.color_line_text),
                        );
                    }
                }
            }
        }
        println!();
    }

    pub fn print_error(&self, msg: &str) {
        eprintln!("❌ {} {}", bold_hex("Error:", &self.cfg.color_error), msg);
    }

    pub fn print_warn(&self, msg: &str) {
        eprintln!("⚠️  {}", hex_color(msg, &self.cfg.color_warn));
    }

    pub fn print_info(&self, msg: &str) {
        eprintln!("ℹ️  {}", hex_color(msg, &self.cfg.color_info));
    }
}
