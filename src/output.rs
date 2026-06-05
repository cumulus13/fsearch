// File: src\output.rs
// Author: Hadi Cahyadi <cumulus13@gmail.com>
// Date: 2026-05-11
// Description:
// License: MIT

//! Coloured terminal output for both search and duplicate results.

use crate::colors::{bold_hex, hex_color};
use crate::config::Config;
use crate::duplicates::{DuplicateGroup, DuplicateSummary};
use crate::searcher::SearchMatch;
use colored::Colorize;
use std::path::Path;
use std::time::Duration;

pub struct Printer<'a> {
    cfg: &'a Config,
}

impl<'a> Printer<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Self { cfg }
    }

    // ── Banners / status ──────────────────────────────────────────────────────

    pub fn print_banner(&self) {
        eprintln!(
            "⚡ {} v{}",
            bold_hex("fsearch", &self.cfg.color_info),
            bold_hex(env!("CARGO_PKG_VERSION"), &self.cfg.color_count),
        );
    }

    pub fn print_searching(&self, dirs: &[&Path], pattern: &str) {
        let dir_list = dirs
            .iter()
            .map(|d| {
                format!(
                    "{}",
                    bold_hex(d.display().to_string(), &self.cfg.color_path)
                )
            })
            .collect::<Vec<_>>()
            .join(hex_color(", ", "#888888").to_string().as_str());
        eprintln!(
            "🔍 Searching [{}] for {}",
            dir_list,
            bold_hex(pattern, &self.cfg.color_pattern),
        );
    }

    pub fn print_scanning_dups(&self, dirs: &[&Path], mode: &str, algo: &str) {
        let dir_list = dirs
            .iter()
            .map(|d| {
                format!(
                    "{}",
                    bold_hex(d.display().to_string(), &self.cfg.color_path)
                )
            })
            .collect::<Vec<_>>()
            .join(hex_color(", ", "#888888").to_string().as_str());
        eprintln!(
            "🔎 Scanning [{}]  [mode: {}  algo: {}]",
            dir_list,
            bold_hex(mode, &self.cfg.color_pattern),
            bold_hex(algo, &self.cfg.color_info),
        );
    }

    // ── Search results ────────────────────────────────────────────────────────

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

        let count = results.len().to_string();
        let ms = elapsed.as_millis();

        println!(
            "\n{} {}  {} {} {}",
            "📋".bold(),
            bold_hex("FOUND:", &self.cfg.color_header),
            bold_hex(&count, &self.cfg.color_count),
            hex_color("result(s)", &self.cfg.color_info),
            hex_color(format!("[{ms}ms]"), "#888888"),
        );
        println!();

        let zfill = count.len();
        for (idx, item) in results.iter().enumerate() {
            let num = bold_hex(
                format!("{:0>w$}", idx + 1, w = zfill),
                &self.cfg.color_index,
            );
            match item {
                SearchMatch::Path(path) => {
                    println!(
                        "{}. {}",
                        num,
                        bold_hex(path.display().to_string(), &self.cfg.color_path)
                    );
                }
                SearchMatch::Content { path, lines } => {
                    println!(
                        "{}. {}",
                        num,
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

    // ── Duplicate results ─────────────────────────────────────────────────────

    pub fn print_duplicates(
        &self,
        groups: &[DuplicateGroup],
        summary: &DuplicateSummary,
        elapsed: Duration,
    ) {
        let ms = elapsed.as_millis();

        if groups.is_empty() {
            println!(
                "\n✅  {} {}\n",
                bold_hex("No duplicates found.", &self.cfg.color_info),
                hex_color(format!("[{ms}ms]"), "#888888"),
            );
            self.print_dup_summary(summary);
            return;
        }

        println!(
            "\n{} {}  {} {} {}",
            "🔁".bold(),
            bold_hex("DUPLICATE GROUPS:", &self.cfg.color_header),
            bold_hex(groups.len().to_string(), &self.cfg.color_count),
            hex_color("group(s)", &self.cfg.color_info),
            hex_color(format!("[{ms}ms]"), "#888888"),
        );
        println!();

        let zfill = groups.len().to_string().len();
        for (idx, group) in groups.iter().enumerate() {
            let num = bold_hex(
                format!("{:0>w$}", idx + 1, w = zfill),
                &self.cfg.color_dup_group,
            );
            // Truncate hash for display (16 chars is enough to distinguish)
            let hash_preview = &group.hash[..group.hash.len().min(16)];
            println!(
                "{}. {} {}  {} {}  {} {}",
                num,
                hex_color("size:", "#888888"),
                bold_hex(group.size_human(), &self.cfg.color_dup_size),
                hex_color("wasted:", "#888888"),
                bold_hex(group.wasted_human(), &self.cfg.color_warn),
                hex_color("hash:", "#888888"),
                hex_color(hash_preview, "#666666"),
            );

            // Individual paths
            for (pi, path) in group.paths.iter().enumerate() {
                let marker = if pi == 0 { "  ◉" } else { "  ○" };
                println!(
                    "{}  {}",
                    hex_color(marker, &self.cfg.color_dup_group),
                    hex_color(path.display().to_string(), &self.cfg.color_dup_path),
                );
            }
            println!();
        }

        self.print_dup_summary(summary);
    }

    fn print_dup_summary(&self, s: &DuplicateSummary) {
        println!(
            "{}  scanned {} file(s) in {} dir(s)  ·  {} group(s)  ·  {} duplicate(s)  ·  {} wasted",
            bold_hex("📊 Summary:", &self.cfg.color_header),
            bold_hex(s.files_scanned.to_string(), &self.cfg.color_count),
            bold_hex(s.dirs_scanned.to_string(), &self.cfg.color_count),
            bold_hex(s.groups_found.to_string(), &self.cfg.color_count),
            bold_hex(s.duplicate_files.to_string(), &self.cfg.color_count),
            bold_hex(s.wasted_human(), &self.cfg.color_warn),
        );
        println!();
    }

    // ── Diagnostics ───────────────────────────────────────────────────────────

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
