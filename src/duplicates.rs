//! Duplicate-file detection across one or more directories.
//!
//! # Strategy
//!
//! 1. **Collect** — walk every supplied root, applying size/binary/glob filters.
//! 2. **Size pass** — group files by byte count; only groups with ≥ 2 members
//!    proceed to the hash pass (makes zero extra IO for unique-size files).
//! 3. **Hash pass** — within each size group, compute the configured digest
//!    (MD5 for speed, SHA-256 for integrity) in parallel with Rayon.
//! 4. **Group** — files sharing the same digest form a [`DuplicateGroup`].
//!
//! Multiple root directories are walked in parallel; results are
//! de-duplicated by canonical path so overlapping trees never produce
//! false duplicates.

use crate::binary::is_binary;
use crate::config::Config;
use crate::error::{FsearchError, FsearchResult};
use glob::Pattern;
use md5::{Digest as _, Md5};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

// ── Public types ──────────────────────────────────────────────────────────────

/// A group of files that are byte-for-byte identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Hex-encoded content digest (or `"name:<name>"` / `"size:<n>"` for
    /// the corresponding detection modes).
    pub hash: String,
    /// File size shared by all members.
    pub size: u64,
    /// All paths in this group (≥ 2).
    pub paths: Vec<PathBuf>,
    /// Total wasted space = `size × (paths.len() − 1)`.
    pub wasted_bytes: u64,
}

impl DuplicateGroup {
    /// Human-readable wasted space, e.g. `"4.2 MiB"`.
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
    pub files_scanned: usize,
    pub dirs_scanned: usize,
    pub groups_found: usize,
    /// Duplicate files across all groups, not counting one "original" per group.
    pub duplicate_files: usize,
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
    /// Size-based: files that share the same byte count (fast, imprecise).
    Size,
}

/// All parameters for a duplicate-detection run.
#[derive(Debug, Clone)]
pub struct DuplicateOptions {
    /// One or more root directories to scan. At least one must be given.
    /// Files found across **all** roots are pooled together before grouping,
    /// so duplicates that live in different trees are detected correctly.
    pub base_dirs: Vec<PathBuf>,

    /// Maximum recursion depth per directory (0 = only that directory).
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
    /// Construct from a [`Config`] with explicit root directories.
    pub fn from_config(cfg: &Config, base_dirs: Vec<PathBuf>) -> FsearchResult<Self> {
        Ok(Self {
            base_dirs,
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

    /// Fluent builder — start with one or more directories.
    pub fn builder(base_dirs: impl IntoDirs) -> DuplicateOptionsBuilder {
        DuplicateOptionsBuilder::new(base_dirs.into_dirs())
    }
}

/// Trait that lets [`DuplicateOptions::builder`] accept either a single path
/// or a `Vec` of paths.
pub trait IntoDirs {
    fn into_dirs(self) -> Vec<PathBuf>;
}

impl IntoDirs for PathBuf {
    fn into_dirs(self) -> Vec<PathBuf> {
        vec![self]
    }
}
impl IntoDirs for &str {
    fn into_dirs(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl IntoDirs for String {
    fn into_dirs(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl IntoDirs for &Path {
    fn into_dirs(self) -> Vec<PathBuf> {
        vec![self.to_path_buf()]
    }
}
// hidden — only the Vec impl below is the "real" multi-path entry
#[doc(hidden)]
impl IntoDirs for &PathBuf {
    fn into_dirs(self) -> Vec<PathBuf> {
        vec![self.into()]
    }
}

impl<P: Into<PathBuf>> IntoDirs for Vec<P> {
    fn into_dirs(self) -> Vec<PathBuf> {
        self.into_iter().map(Into::into).collect()
    }
}

/// Fluent builder for [`DuplicateOptions`].
pub struct DuplicateOptionsBuilder(DuplicateOptions);

impl DuplicateOptionsBuilder {
    fn new(base_dirs: Vec<PathBuf>) -> Self {
        Self(DuplicateOptions {
            base_dirs,
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

    /// Add another root directory to the scan.
    pub fn add_dir(mut self, d: impl Into<PathBuf>) -> Self {
        self.0.base_dirs.push(d.into());
        self
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

/// Find duplicate files across all directories in `opts.base_dirs`.
///
/// Returns `(groups, summary)`. Groups are sorted by `wasted_bytes` descending.
pub fn find_duplicates(
    opts: &DuplicateOptions,
    interrupted: Arc<AtomicBool>,
) -> FsearchResult<(Vec<DuplicateGroup>, DuplicateSummary)> {
    // Validate all roots up-front so we never start a partial scan.
    if opts.base_dirs.is_empty() {
        return Err(FsearchError::DirectoryNotFound(
            "(no directories specified)".into(),
        ));
    }
    for d in &opts.base_dirs {
        if !d.exists() {
            return Err(FsearchError::DirectoryNotFound(d.display().to_string()));
        }
        if !d.is_dir() {
            return Err(FsearchError::NotADirectory(d.display().to_string()));
        }
    }

    let dirs_scanned = opts.base_dirs.len();

    // Collect all candidate files from every root, de-duplicated by path.
    let files = collect_files(opts, &interrupted);
    let total_scanned = files.len();

    if interrupted.load(Ordering::Relaxed) {
        return Err(FsearchError::Interrupted);
    }

    let mut groups = match opts.mode {
        DuplicateMode::Content => by_content(files, opts, &interrupted)?,
        DuplicateMode::Name => by_name(files, opts),
        DuplicateMode::Size => by_size_only(files),
    };

    // Sort: most wasteful groups first
    groups.sort_unstable_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));

    if opts.max_results > 0 && groups.len() > opts.max_results {
        groups.truncate(opts.max_results);
    }

    let summary = DuplicateSummary {
        files_scanned: total_scanned,
        dirs_scanned,
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
    // Shared seen-set prevents counting the same file twice when the caller
    // supplies overlapping directories (e.g. `/home` and `/home/user`).
    let seen: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

    opts.base_dirs
        .par_iter()
        .flat_map(|base| {
            if interrupted.load(Ordering::Relaxed) {
                return vec![];
            }
            WalkDir::new(base)
                .max_depth(opts.max_depth as usize + 1)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    if e.file_type().is_dir() && e.depth() > 0 {
                        let name = e.file_name().to_string_lossy().to_string();
                        if is_excluded_dir_local(&name, &opts.exclude_dirs) {
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
                    if !matches_include_local(&name, &opts.include_patterns) {
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
                    let path = e.path().to_path_buf();
                    // Deduplicate across roots
                    {
                        let mut guard = seen.lock().unwrap();
                        if !guard.insert(path.clone()) {
                            return None;
                        }
                    }
                    let size = e.metadata().ok()?.len();
                    Some(FileEntry { path, size })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

// ── Content mode ──────────────────────────────────────────────────────────────

fn by_content(
    files: Vec<FileEntry>,
    opts: &DuplicateOptions,
    interrupted: &AtomicBool,
) -> FsearchResult<Vec<DuplicateGroup>> {
    // Size pre-filter: skip files whose size is unique — they cannot be dups.
    let mut size_count: HashMap<u64, usize> = HashMap::new();
    for f in &files {
        *size_count.entry(f.size).or_default() += 1;
    }

    let candidates: Vec<(u64, PathBuf)> = files
        .into_iter()
        .filter(|f| size_count.get(&f.size).copied().unwrap_or(0) > 1)
        .map(|f| (f.size, f.path))
        .collect();

    if candidates.is_empty() {
        return Ok(vec![]);
    }

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

    let mut hash_map: HashMap<String, (u64, Vec<PathBuf>)> = HashMap::new();
    for (hash, size, path) in hashed {
        hash_map
            .entry(hash)
            .or_insert_with(|| (size, vec![]))
            .1
            .push(path);
    }

    Ok(hash_map
        .into_iter()
        .filter(|(_, (_, paths))| paths.len() > 1)
        .map(|(hash, (size, mut paths))| {
            paths.sort(); // stable ordering within each group
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
            let mut paths: Vec<PathBuf> = entries.into_iter().map(|(p, _)| p).collect();
            paths.sort();
            let wasted = if opts.mode == DuplicateMode::Name {
                size.saturating_mul(paths.len() as u64 - 1)
            } else {
                0
            };
            DuplicateGroup {
                hash: format!("name:{name}"),
                size,
                wasted_bytes: wasted,
                paths,
            }
        })
        .collect()
}

// ── Size-only mode ────────────────────────────────────────────────────────────

fn by_size_only(files: Vec<FileEntry>) -> Vec<DuplicateGroup> {
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for f in files {
        size_map.entry(f.size).or_default().push(f.path);
    }
    size_map
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(size, mut paths)| {
            paths.sort();
            let wasted = size * (paths.len() as u64 - 1);
            DuplicateGroup {
                hash: format!("size:{size}"),
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

// ── Private helpers ───────────────────────────────────────────────────────────

fn is_excluded_dir_local(name: &str, excludes: &[String]) -> bool {
    excludes
        .iter()
        .any(|ex| Pattern::new(ex).map(|p| p.matches(name)).unwrap_or(false) || ex == name)
}

fn matches_include_local(name: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true;
    }
    patterns.iter().any(|p| {
        Pattern::new(p)
            .map(|pat| pat.matches(name))
            .unwrap_or(false)
    })
}

/// Format bytes as a human-readable IEC string.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let exp = ((bytes as f64).log(1024.0).floor() as usize).min(UNITS.len() - 1);
    let val = bytes as f64 / 1024_f64.powi(exp as i32);
    if exp == 0 {
        format!("{bytes} B")
    } else {
        format!("{val:.1} {}", UNITS[exp])
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::File::create(&p).unwrap().write_all(content).unwrap();
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
    fn detects_identical_files_single_dir() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "a.txt", b"hello world");
        make_file(tmp.path(), "b.txt", b"hello world");
        make_file(tmp.path(), "c.txt", b"different");

        let opts = DuplicateOptions::builder(tmp.path()).max_depth(1).build();
        let (groups, summary) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(summary.duplicate_files, 1);
        assert_eq!(summary.dirs_scanned, 1);
    }

    #[test]
    fn detects_duplicates_across_multiple_dirs() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        make_file(dir1.path(), "x.txt", b"shared content");
        make_file(dir2.path(), "y.txt", b"shared content");
        make_file(dir1.path(), "unique.txt", b"only here");

        let opts = DuplicateOptions::builder(vec![dir1.path(), dir2.path()])
            .max_depth(1)
            .build();
        let (groups, summary) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(groups.len(), 1, "cross-dir duplicates should be detected");
        assert_eq!(summary.dirs_scanned, 2);
    }

    #[test]
    fn no_false_positives_across_dirs() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        make_file(dir1.path(), "a.txt", b"aaa");
        make_file(dir2.path(), "b.txt", b"bbb");

        let opts = DuplicateOptions::builder(vec![dir1.path(), dir2.path()]).build();
        let (groups, _) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn overlapping_dirs_no_self_duplicates() {
        // Supplying a dir and one of its subdirs should not produce self-duplicates.
        let root = TempDir::new().unwrap();
        let sub = root.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        make_file(root.path(), "top.txt", b"hello world");
        make_file(&sub, "sub.txt", b"hello world");

        let opts = DuplicateOptions::builder(vec![root.path(), sub.as_path()])
            .max_depth(5)
            .build();
        let (groups, _) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        // Only 1 group with exactly 2 members — not 3
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
    }

    #[test]
    fn name_mode_across_dirs() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        make_file(dir1.path(), "readme.txt", b"v1");
        make_file(dir2.path(), "readme.txt", b"v2");

        let opts = DuplicateOptions::builder(vec![dir1.path(), dir2.path()])
            .mode(DuplicateMode::Name)
            .build();
        let (groups, _) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(!groups.is_empty());
    }

    #[test]
    fn hash_file_md5_and_sha256() {
        let tmp = TempDir::new().unwrap();
        let p = make_file(tmp.path(), "f.txt", b"test content");
        let md5 = hash_file(&p, HashAlgorithm::Md5, 4096).unwrap();
        let sha256 = hash_file(&p, HashAlgorithm::Sha256, 4096).unwrap();
        assert_eq!(md5.len(), 32);
        assert_eq!(sha256.len(), 64);
    }

    #[test]
    fn size_filter_excludes_small_files() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "s1.txt", b"hi");
        make_file(tmp.path(), "s2.txt", b"hi");

        let opts = DuplicateOptions::builder(tmp.path()).min_size(1000).build();
        let (groups, _) = find_duplicates(&opts, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(groups.is_empty());
    }
}
