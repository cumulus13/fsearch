//! Duplicate-file detection.
//!
//! # Strategy
//!
//! 1. **Size pass** — group all files by byte size (free, no IO beyond `stat`).
//! 2. **Hash pass** — within each size group, compute the configured hash
//!    (MD5 for speed, SHA-256 for integrity) and group by digest.
//! 3. Result is a `Vec<DuplicateGroup>` — each group holds two or more paths
//!    that are byte-for-byte identical.
//!
//! The expensive hash pass is parallelised with Rayon.  Both passes respect
//! the `interrupted` flag so Ctrl-C works cleanly.

use crate::binary::is_binary;
use crate::config::Config;
use crate::error::{FsearchError, FsearchResult};
use glob::Pattern;
use md5::{Digest as _, Md5};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// ── Public types ──────────────────────────────────────────────────────────────

/// A group of files that are byte-for-byte identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Hex-encoded content digest.
    pub hash: String,
    /// File size shared by all members.
    pub size: u64,
    /// All paths in this group (≥ 2).
    pub paths: Vec<PathBuf>,
    /// Total wasted space = `size * (paths.len() - 1)`.
    pub wasted_bytes: u64,
}

impl DuplicateGroup {
    /// Human-readable wasted space (e.g. `"4.2 MiB"`).
    pub fn wasted_human(&self) -> String {
        human_bytes(self.wasted_bytes)
    }

    /// Human-readable file size.
    pub fn size_human(&self) -> String {
        human_bytes(self.size)
    }
}

/// Summary statistics for a duplicate scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicateSummary {
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Number of duplicate groups found.
    pub groups_found: usize,
    /// Total duplicate files (across all groups, not counting originals).
    pub duplicate_files: usize,
    /// Total wasted bytes.
    pub wasted_bytes: u64,
}

impl DuplicateSummary {
    pub fn wasted_human(&self) -> String {
        human_bytes(self.wasted_bytes)
    }
}

// ── Options ───────────────────────────────────────────────────────────────────

/// Hashing algorithm for content comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HashAlgorithm {
    Md5,
    #[default]
    Sha256,
}

impl HashAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
        }
    }
}

impl std::str::FromStr for HashAlgorithm {
    type Err = FsearchError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md5" => Ok(Self::Md5),
            "sha256" => Ok(Self::Sha256),
            other => Err(FsearchError::UnsupportedHashAlgorithm(other.into())),
        }
    }
}

/// Controls how duplicates are identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DuplicateMode {
    /// Hash-based: files with identical content (default).
    #[default]
    Content,
    /// Name-based: files with the same filename (different directories).
    Name,
    /// Size-based: files that share the same byte count (fast but imprecise).
    Size,
}

/// All parameters for a single duplicate-detection run.
#[derive(Debug, Clone)]
pub struct DuplicateOptions {
    /// Root directory to scan.
    pub base_dir: PathBuf,
    /// Maximum recursion depth (0 = only `base_dir`).
    pub max_depth: u32,
    /// Detection mode.
    pub mode: DuplicateMode,
    /// Hashing algorithm (only used in [`DuplicateMode::Content`]).
    pub algorithm: HashAlgorithm,
    /// Read buffer size for hashing (bytes).
    pub buffer_size: usize,
    /// Skip files smaller than this (bytes). 0 = include all.
    pub min_size: u64,
    /// Skip files larger than this (bytes). 0 = no limit.
    pub max_size: u64,
    /// Skip binary files.
    pub skip_binary: bool,
    /// Binary-check probe length.
    pub binary_check_bytes: usize,
    /// Only include files matching these glob patterns (empty = all).
    pub include_patterns: Vec<String>,
    /// Directory names to skip entirely.
    pub exclude_dirs: Vec<String>,
    /// Cap results (0 = unlimited).
    pub max_results: usize,
}

impl DuplicateOptions {
    /// Construct from a [`Config`].
    pub fn from_config(cfg: &Config, base_dir: PathBuf) -> FsearchResult<Self> {
        Ok(Self {
            base_dir,
            max_depth: cfg.default_depth,
            mode: DuplicateMode::Content,
            algorithm: cfg.hash_algorithm.parse::<HashAlgorithm>()?,
            buffer_size: cfg.hash_buffer_size,
            min_size: cfg.dup_min_size,
            max_size: cfg.dup_max_size,
            skip_binary: false,
            binary_check_bytes: cfg.binary_check_bytes,
            include_patterns: crate::config::split_csv(&cfg.default_include),
            exclude_dirs: cfg.excluded_dirs(),
            max_results: cfg.max_results,
        })
    }

    /// Fluent builder entry-point.
    pub fn builder(base_dir: impl Into<PathBuf>) -> DuplicateOptionsBuilder {
        DuplicateOptionsBuilder::new(base_dir.into())
    }
}

/// Fluent builder for [`DuplicateOptions`].
pub struct DuplicateOptionsBuilder(DuplicateOptions);

impl DuplicateOptionsBuilder {
    fn new(base_dir: PathBuf) -> Self {
        Self(DuplicateOptions {
            base_dir,
            max_depth: 10,
            mode: DuplicateMode::Content,
            algorithm: HashAlgorithm::Sha256,
            buffer_size: 65_536,
            min_size: 1,
            max_size: 0,
            skip_binary: false,
            binary_check_bytes: 1024,
            include_patterns: vec![],
            exclude_dirs: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                ".svn".into(),
                "__pycache__".into(),
                ".hg".into(),
                ".cache".into(),
            ],
            max_results: 0,
        })
    }
    pub fn max_depth(mut self, d: u32) -> Self {
        self.0.max_depth = d;
        self
    }
    pub fn mode(mut self, m: DuplicateMode) -> Self {
        self.0.mode = m;
        self
    }
    pub fn algorithm(mut self, a: HashAlgorithm) -> Self {
        self.0.algorithm = a;
        self
    }
    pub fn buffer_size(mut self, b: usize) -> Self {
        self.0.buffer_size = b;
        self
    }
    pub fn min_size(mut self, s: u64) -> Self {
        self.0.min_size = s;
        self
    }
    pub fn max_size(mut self, s: u64) -> Self {
        self.0.max_size = s;
        self
    }
    pub fn skip_binary(mut self, v: bool) -> Self {
        self.0.skip_binary = v;
        self
    }
    pub fn include_patterns(mut self, p: Vec<String>) -> Self {
        self.0.include_patterns = p;
        self
    }
    pub fn exclude_dirs(mut self, d: Vec<String>) -> Self {
        self.0.exclude_dirs = d;
        self
    }
    pub fn max_results(mut self, n: usize) -> Self {
        self.0.max_results = n;
        self
    }
    pub fn build(self) -> DuplicateOptions {
        self.0
    }
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Find duplicate files under `opts.base_dir`.
///
/// Returns a `(Vec<DuplicateGroup>, DuplicateSummary)` pair.
/// Groups are sorted by `wasted_bytes` descending so the most wasteful
/// duplicates appear first.
pub fn find_duplicates(
    opts: &DuplicateOptions,
    interrupted: Arc<AtomicBool>,
) -> FsearchResult<(Vec<DuplicateGroup>, DuplicateSummary)> {
    if !opts.base_dir.exists() {
        return Err(FsearchError::DirectoryNotFound(
            opts.base_dir.display().to_string(),
        ));
    }
    if !opts.base_dir.is_dir() {
        return Err(FsearchError::NotADirectory(
            opts.base_dir.display().to_string(),
        ));
    }

    // ── Collect all candidate files ───────────────────────────────────────────
    let files = collect_files(opts, &interrupted);
    let total_scanned = files.len();

    if interrupted.load(Ordering::Relaxed) {
        return Err(FsearchError::Interrupted);
    }

    // ── Dispatch to the chosen mode ───────────────────────────────────────────
    let mut groups = match opts.mode {
        DuplicateMode::Content => by_content(files, opts, &interrupted)?,
        DuplicateMode::Name => by_name(files, opts),
        DuplicateMode::Size => by_size_only(files, opts),
    };

    // Sort groups: most wasteful first
    groups.sort_unstable_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));

    // Cap results
    if opts.max_results > 0 && groups.len() > opts.max_results {
        groups.truncate(opts.max_results);
    }

    let summary = DuplicateSummary {
        files_scanned: total_scanned,
        groups_found: groups.len(),
        duplicate_files: groups.iter().map(|g| g.paths.len() - 1).sum(),
        wasted_bytes: groups.iter().map(|g| g.wasted_bytes).sum(),
    };

    Ok((groups, summary))
}

// ── File collection ───────────────────────────────────────────────────────────

struct FileEntry {
    path: PathBuf,
    size: u64,
}

fn collect_files(opts: &DuplicateOptions, interrupted: &AtomicBool) -> Vec<FileEntry> {
    WalkDir::new(&opts.base_dir)
        .max_depth(opts.max_depth as usize + 1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() && e.depth() > 0 {
                let name = e.file_name().to_string_lossy().to_string();
                if is_excluded_dir_dup(&name, &opts.exclude_dirs) {
                    return false;
                }
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            if interrupted.load(Ordering::Relaxed) {
                return false;
            }
            if !e.file_type().is_file() {
                return false;
            }

            let name = e.file_name().to_string_lossy().to_string();
            if !matches_include_dup(&name, &opts.include_patterns) {
                return false;
            }

            if let Ok(meta) = e.metadata() {
                let sz = meta.len();
                if opts.min_size > 0 && sz < opts.min_size {
                    return false;
                }
                if opts.max_size > 0 && sz > opts.max_size {
                    return false;
                }
            }

            if opts.skip_binary && is_binary(e.path(), opts.binary_check_bytes) {
                return false;
            }
            true
        })
        .filter_map(|e| {
            let size = e.metadata().ok()?.len();
            Some(FileEntry {
                path: e.path().to_path_buf(),
                size,
            })
        })
        .collect()
}

// ── Content mode ──────────────────────────────────────────────────────────────

fn by_content(
    files: Vec<FileEntry>,
    opts: &DuplicateOptions,
    interrupted: &AtomicBool,
) -> FsearchResult<Vec<DuplicateGroup>> {
    // ── Size pass: only hash files whose size appears more than once ──────────
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for f in files {
        size_map.entry(f.size).or_default().push(f.path);
    }
    let candidates: Vec<(u64, PathBuf)> = size_map
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map(|(sz, paths)| paths.into_iter().map(move |p| (sz, p)))
        .collect();

    if candidates.is_empty() {
        return Ok(vec![]);
    }

    // ── Hash pass (parallel) ──────────────────────────────────────────────────
    let buf = opts.buffer_size;
    let algo = opts.algorithm;

    let hashed: Vec<(String, u64, PathBuf)> = candidates
        .into_par_iter()
        .filter_map(|(size, path)| {
            if interrupted.load(Ordering::Relaxed) {
                return None;
            }
            let digest = hash_file(&path, algo, buf).ok()?;
            Some((digest, size, path))
        })
        .collect();

    // ── Group by hash ─────────────────────────────────────────────────────────
    let mut hash_map: HashMap<String, (u64, Vec<PathBuf>)> = HashMap::new();
    for (hash, size, path) in hashed {
        let entry = hash_map.entry(hash).or_insert_with(|| (size, vec![]));
        entry.1.push(path);
    }

    Ok(hash_map
        .into_iter()
        .filter(|(_, (_, paths))| paths.len() > 1)
        .map(|(hash, (size, paths))| {
            let wasted = size * (paths.len() as u64 - 1);
            DuplicateGroup {
                hash,
                size,
                paths,
                wasted_bytes: wasted,
            }
        })
        .collect())
}

// ── Name mode ─────────────────────────────────────────────────────────────────

fn by_name(files: Vec<FileEntry>, opts: &DuplicateOptions) -> Vec<DuplicateGroup> {
    let mut name_map: HashMap<String, Vec<(PathBuf, u64)>> = HashMap::new();
    for f in files {
        let name = f
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        name_map.entry(name).or_default().push((f.path, f.size));
    }
    name_map
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(name, entries)| {
            let size = entries.first().map(|(_, s)| *s).unwrap_or(0);
            let paths: Vec<PathBuf> = entries.into_iter().map(|(p, _)| p).collect();
            let wasted = size.saturating_mul(paths.len() as u64 - 1);
            DuplicateGroup {
                hash: format!("name:{}", name),
                size,
                wasted_bytes: if opts.mode == DuplicateMode::Name {
                    wasted
                } else {
                    0
                },
                paths,
            }
        })
        .collect()
}

// ── Size-only mode ────────────────────────────────────────────────────────────

fn by_size_only(files: Vec<FileEntry>, _opts: &DuplicateOptions) -> Vec<DuplicateGroup> {
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for f in files {
        size_map.entry(f.size).or_default().push(f.path);
    }
    size_map
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(size, paths)| {
            let wasted = size * (paths.len() as u64 - 1);
            DuplicateGroup {
                hash: format!("size:{}", size),
                size,
                paths,
                wasted_bytes: wasted,
            }
        })
        .collect()
}

// ── Hashing ───────────────────────────────────────────────────────────────────

/// Hash a file with the chosen algorithm; returns a lower-hex digest string.
pub fn hash_file(path: &Path, algo: HashAlgorithm, buf_size: usize) -> FsearchResult<String> {
    let file = fs::File::open(path).map_err(|e| FsearchError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut reader = BufReader::with_capacity(buf_size, file);
    let mut buf = vec![0u8; buf_size];

    match algo {
        HashAlgorithm::Md5 => {
            let mut h = Md5::new();
            loop {
                let n = reader.read(&mut buf).map_err(|e| FsearchError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
                if n == 0 {
                    break;
                }
                md5::Digest::update(&mut h, &buf[..n]);
            }
            Ok(format!("{:x}", md5::Digest::finalize(h)))
        }
        HashAlgorithm::Sha256 => {
            let mut h = Sha256::new();
            loop {
                let n = reader.read(&mut buf).map_err(|e| FsearchError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
                if n == 0 {
                    break;
                }
                sha2::Digest::update(&mut h, &buf[..n]);
            }
            Ok(format!("{:x}", sha2::Digest::finalize(h)))
        }
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn is_excluded_dir_dup(name: &str, excludes: &[String]) -> bool {
    excludes
        .iter()
        .any(|ex| Pattern::new(ex).map(|p| p.matches(name)).unwrap_or(false) || ex == name)
}

fn matches_include_dup(name: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true;
    }
    patterns.iter().any(|p| {
        Pattern::new(p)
            .map(|pat| pat.matches(name))
            .unwrap_or(false)
    })
}

/// Format bytes as a human-readable string (IEC units).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let exp = (bytes as f64).log(1024.0).floor() as usize;
    let exp = exp.min(UNITS.len() - 1);
    let val = bytes as f64 / 1024_f64.powi(exp as i32);
    if exp == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", val, UNITS[exp])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn human_bytes_formatting() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn detects_identical_files() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "a.txt", b"hello world");
        make_file(tmp.path(), "b.txt", b"hello world");
        make_file(tmp.path(), "c.txt", b"different content");

        let opts = DuplicateOptions::builder(tmp.path()).max_depth(1).build();
        let interrupted = Arc::new(AtomicBool::new(false));
        let (groups, summary) = find_duplicates(&opts, interrupted).unwrap();

        assert_eq!(groups.len(), 1, "one duplicate group expected");
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(summary.duplicate_files, 1);
    }

    #[test]
    fn no_false_positives_for_unique_files() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "x.txt", b"aaa");
        make_file(tmp.path(), "y.txt", b"bbb");

        let opts = DuplicateOptions::builder(tmp.path()).build();
        let interrupted = Arc::new(AtomicBool::new(false));
        let (groups, _) = find_duplicates(&opts, interrupted).unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn name_mode_groups_by_filename() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        make_file(tmp.path(), "readme.txt", b"v1");
        make_file(&sub, "readme.txt", b"v2");

        let opts = DuplicateOptions::builder(tmp.path())
            .max_depth(2)
            .mode(DuplicateMode::Name)
            .build();
        let interrupted = Arc::new(AtomicBool::new(false));
        let (groups, _) = find_duplicates(&opts, interrupted).unwrap();
        assert!(!groups.is_empty(), "name duplicates expected");
    }

    #[test]
    fn hash_file_md5_and_sha256() {
        let tmp = TempDir::new().unwrap();
        let p = make_file(tmp.path(), "f.txt", b"test content");
        let md5 = hash_file(&p, HashAlgorithm::Md5, 4096).unwrap();
        let sha256 = hash_file(&p, HashAlgorithm::Sha256, 4096).unwrap();
        assert_eq!(md5.len(), 32, "md5 should be 32 hex chars");
        assert_eq!(sha256.len(), 64, "sha256 should be 64 hex chars");
    }

    #[test]
    fn size_filter_excludes_small_files() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "small1.txt", b"hi");
        make_file(tmp.path(), "small2.txt", b"hi");

        let opts = DuplicateOptions::builder(tmp.path())
            .min_size(1000) // bigger than our test files
            .build();
        let interrupted = Arc::new(AtomicBool::new(false));
        let (groups, _) = find_duplicates(&opts, interrupted).unwrap();
        assert!(groups.is_empty(), "small files should be filtered out");
    }
}
