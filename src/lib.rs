//! # fsearch
//!
//! A blazingly fast, cross-platform library for file search and duplicate
//! detection.
//!
//! ## Quick start
//!
//! ### File search
//! ```no_run
//! use fsearch::searcher::{fast_find, SearchOptions};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let opts = SearchOptions::builder("*.rs")
//!     .base_dir("./src")
//!     .max_depth(5)
//!     .case_insensitive(true)
//!     .build();
//!
//! let interrupted = Arc::new(AtomicBool::new(false));
//! let results = fast_find(&opts, interrupted).unwrap();
//! for m in &results {
//!     println!("{}", m.path().display());
//! }
//! ```
//!
//! ### Content search
//! ```no_run
//! use fsearch::searcher::{fast_find, SearchOptions};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let opts = SearchOptions::builder("TODO")
//!     .base_dir(".")
//!     .max_depth(10)
//!     .search_in_files(true)
//!     .include_patterns(vec!["*.rs".into(), "*.py".into()])
//!     .build();
//!
//! let interrupted = Arc::new(AtomicBool::new(false));
//! let results = fast_find(&opts, interrupted).unwrap();
//! ```
//!
//! ### Duplicate detection
//! ```no_run
//! use fsearch::duplicates::{find_duplicates, DuplicateOptions, DuplicateMode, HashAlgorithm};
//! use std::sync::{Arc, atomic::AtomicBool};
//!
//! let opts = DuplicateOptions::builder(".")
//!     .max_depth(10)
//!     .mode(DuplicateMode::Content)
//!     .algorithm(HashAlgorithm::Sha256)
//!     .min_size(1024) // skip files smaller than 1 KiB
//!     .build();
//!
//! let interrupted = Arc::new(AtomicBool::new(false));
//! let (groups, summary) = find_duplicates(&opts, interrupted).unwrap();
//! println!("Found {} duplicate groups, wasted {}", summary.groups_found, summary.wasted_human());
//! ```

pub mod binary;
pub mod colors;
pub mod config;
pub mod duplicates;
pub mod error;
pub mod output;
pub mod searcher;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use config::Config;
pub use duplicates::{
    find_duplicates, DuplicateGroup, DuplicateMode, DuplicateOptions, DuplicateSummary,
    HashAlgorithm,
};
pub use error::{FsearchError, FsearchResult};
pub use searcher::{fast_find, recursive_find, LineMatch, SearchMatch, SearchOptions};
